//! Bilingua filter for Adobe FrameMaker MIF (`.mif`), version 8 and later (Unicode, UTF-8). See the README.
//!
//! MIF is a nested statement grammar, not XML: `<Name args... <Sub ...> >` with `` `strings' ``, `#` comments and
//! opaque inset facet lines. Every `Para` statement with text is a unit (a paragraph wrapped over several
//! `ParaLine` statements is one); variables, cross-references, markers, footnote and table anchors and any other
//! statement are point tags, and a font change switched on and restored in the paragraph is an open/close pair.
//! An untouched merge returns the original bytes; a translated paragraph is rebuilt as one ParaLine and spliced
//! into the original text.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod esc;
mod font;
mod lang;
mod para;
mod render;
mod scan;

pub use esc::{decode_string, encode_string};
pub use lang::fm_language;

use bilingua_model::guard::{self, LimitError, LimitKind};
use bilingua_model::*;
use para::{analyse, Outcome, HIDDEN};
use scan::{scan, ScanError};

const ID: &str = "mif";
/// Statements allowed per allowed unit before scanning (MIF is verbose: a body paragraph with an inline Pgf
/// override is easily 100 statements).
const STATEMENTS_PER_UNIT: usize = 200;
const BOM8: &[u8] = &[0xEF, 0xBB, 0xBF];
const UTF8: &str = "utf-8";
const UTF8_BOM: &str = "utf-8-bom";

fn parse_err(msg: impl Into<String>) -> Error {
    Error::Parse {
        format: ID.into(),
        msg: msg.into(),
    }
}

/// The Adobe FrameMaker MIF filter.
pub struct Mif;

/// What the `<MIFFile version>` header says.
struct Header {
    kind: String,
    version: String,
}

/// Reads the first statement of the file: optional BOM, white space and `#` comment lines, then `<Kind version`.
fn read_header(data: &[u8]) -> Option<Header> {
    let b = &data[..data.len().min(8192)];
    let n = b.len();
    let ws = |c: u8| matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c);
    let mut i = if b.starts_with(BOM8) { 3 } else { 0 };
    loop {
        while i < n && ws(b[i]) {
            i += 1;
        }
        if i < n && b[i] == b'#' {
            while i < n && b[i] != b'\n' && b[i] != b'\r' {
                i += 1;
            }
            if i >= n {
                return None;
            }
            continue;
        }
        break;
    }
    if i >= n || b[i] != b'<' {
        return None;
    }
    i += 1;
    while i < n && ws(b[i]) {
        i += 1;
    }
    let ks = i;
    while i < n && b[i].is_ascii_alphabetic() {
        i += 1;
    }
    if i == ks {
        return None;
    }
    let kind = String::from_utf8_lossy(&b[ks..i]).into_owned();
    let ws_start = i;
    while i < n && ws(b[i]) {
        i += 1;
    }
    if i == ws_start {
        return None;
    }
    if i < n && b[i] == b'`' {
        i += 1;
    }
    let vs = i;
    if i >= n || !b[i].is_ascii_digit() {
        return None;
    }
    while i < n && (b[i].is_ascii_digit() || b[i] == b'.') {
        i += 1;
    }
    Some(Header {
        kind,
        version: String::from_utf8_lossy(&b[vs..i]).into_owned(),
    })
}

/// The MIF version of a document file (`"2019"`, `"9.00"`), or an error for anything that is not a MIF document.
pub fn mif_version(data: &[u8]) -> Result<String, Error> {
    let h = read_header(data).ok_or_else(|| {
        parse_err(
            "This file could not be read as FrameMaker MIF: it must start with a <MIFFile version> statement.",
        )
    })?;
    if h.kind != "MIFFile" {
        return Err(parse_err(format!(
            "This is a FrameMaker MIF {} file. Only document MIF files are supported: upload each chapter's MIF file instead.",
            h.kind
        )));
    }
    let major: u64 = h
        .version
        .chars()
        .take_while(char::is_ascii_digit)
        .fold(0u64, |a, d| {
            a.saturating_mul(10).saturating_add(d.to_digit(10).unwrap_or(0) as u64)
        });
    if major < 8 {
        return Err(Error::Encoding(format!(
            "This is a MIF {} file. MIF 7 and older are not Unicode and are not supported: open the document in FrameMaker 8 or later and save it as MIF again.",
            h.version
        )));
    }
    Ok(h.version)
}

fn scan_err(e: ScanError) -> Error {
    match e {
        ScanError::Syntax(m) => parse_err(m),
        ScanError::TooDeep => LimitError::new(
            LimitKind::XmlDepth,
            "This MIF file is nested too deeply.",
        )
        .into(),
    }
}

/// Every paragraph statement in file order (iterative; a Para never nests another Para).
fn paragraphs(text: &str, tree: &scan::Tree) -> Vec<u32> {
    let mut out = Vec::new();
    let mut stack: Vec<u32> = Vec::new();
    let mut cur = tree.nodes[0].first_child;
    loop {
        if cur == scan::NONE {
            match stack.pop() {
                Some(next) => {
                    cur = next;
                    continue;
                }
                None => break,
            }
        }
        let node = cur;
        let next = tree.nodes[node as usize].next;
        if tree.name(text, node) == "Para" {
            out.push(node);
            cur = next;
        } else if tree.has_children(node) {
            stack.push(next);
            cur = tree.nodes[node as usize].first_child;
        } else {
            cur = next;
        }
    }
    out
}

impl Mif {
    fn parse_text<'a>(bytes: &'a [u8], limits: &Limits) -> Result<(&'a str, bool), Error> {
        guard::check_input(bytes, limits)?;
        mif_version(bytes)?;
        let (body, bom) = match bytes.strip_prefix(BOM8) {
            Some(r) => (r, true),
            None => (bytes, false),
        };
        let text = std::str::from_utf8(body).map_err(|e| {
            let at = e.valid_up_to();
            Error::Encoding(format!(
                "This MIF file is not valid UTF-8 (byte 0x{:02x} at position {at}). MIF 8 and later are UTF-8: save it as MIF again from FrameMaker 8 or later.",
                body[at]
            ))
        })?;
        Ok((text, bom))
    }
}

impl Filter for Mif {
    fn id(&self) -> &'static str {
        ID
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["mif"]
    }

    /// True when the file starts with a `<MIFFile version>` statement (BOM, white space and comments allowed
    /// before it). The check reads the first 8 KB only, so it is cheap and never claims anything else.
    fn sniff(&self, bytes: &[u8]) -> bool {
        read_header(bytes).is_some_and(|h| h.kind == "MIFFile")
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        let limits = opts.limits();
        let (text, bom) = Mif::parse_text(bytes, &limits)?;
        let budget = STATEMENTS_PER_UNIT.saturating_mul(limits.max_units);
        if text.bytes().filter(|&b| b == b'<').count() > budget {
            return Err(LimitError::new(
                LimitKind::XmlElements,
                format!(
                    "This MIF file has more than {budget} statements ({STATEMENTS_PER_UNIT} per allowed segment). Split it into smaller files."
                ),
            )
            .into());
        }
        let tree = scan(text, limits.max_xml_depth).map_err(scan_err)?;
        let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };
        let pgf = font::pgf_fonts(text, &tree);
        let mut parts: Vec<SkelPart> = Vec::new();
        let mut units: Vec<Unit> = Vec::new();
        let (mut pos, mut line_pos, mut line) = (0usize, 0usize, 1usize);
        let mut skipped = 0usize;
        for node in paragraphs(text, &tree) {
            let a = match analyse(text, &tree, node, &pgf, nl) {
                Outcome::Unit(a) => a,
                Outcome::NoLines => continue,
                Outcome::Skip { marker_chars } => {
                    skipped += usize::from(marker_chars);
                    continue;
                }
            };
            let (s, e) = a.span;
            if s < pos {
                continue;
            }
            guard::check_unit_count(units.len() + 1, &limits)?;
            if s > pos {
                parts.push(SkelPart::Raw(text[pos..s].to_string()));
            }
            pos = e;
            line += text.as_bytes()[line_pos..s]
                .iter()
                .filter(|&&b| b == b'\n')
                .count();
            line_pos = s;
            let mut u = Unit::new(format!("u{}", units.len() + 1), a.inlines);
            u.codes = a.codes;
            u.context.kind = Some("paragraph".into());
            if !a.style.is_empty() {
                u.context.style = Some(a.style);
            }
            u.context.location = Some(format!("line {line}"));
            parts.push(SkelPart::Unit(units.len()));
            units.push(u);
        }
        if pos < text.len() {
            parts.push(SkelPart::Raw(text[pos..].to_string()));
        }
        let doc = Document {
            format: ID.into(),
            source_lang: opts.source_lang.clone(),
            target_lang: opts.target_lang.clone(),
            granularity: opts.granularity,
            units,
            skeleton: Skeleton::text(
                parts,
                if bom { UTF8_BOM } else { UTF8 },
                bilingua_model::parser_info!(),
            ),
            ..Default::default()
        };
        let warnings = if skipped > 0 {
            vec![Warning::new(
                "skipped_marker_paragraphs",
                "",
                format!("{skipped} paragraph(s) that already contain the marker characters were left untranslated."),
            )]
        } else {
            Vec::new()
        };
        guard::finish(doc, warnings, &limits)
    }

    fn merge(&self, doc: &Document) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text {
            parts, encoding, ..
        } = &doc.skeleton
        else {
            return Err(Error::Write("mif needs a text skeleton".into()));
        };
        if encoding != UTF8 && encoding != UTF8_BOM {
            return Err(Error::Write(format!(
                "unknown encoding {encoding:?} in the mif skeleton"
            )));
        }
        let mut out = String::new();
        let mut changed = false;
        let mut translated = false;
        for p in parts {
            match p {
                SkelPart::Raw(s) => out.push_str(s),
                SkelPart::Unit(i) => {
                    let u = doc
                        .units
                        .get(*i)
                        .ok_or_else(|| Error::Write(format!("no unit {i}")))?;
                    let original = u
                        .codes
                        .get(HIDDEN)
                        .and_then(|h| h.standalone.as_deref())
                        .ok_or_else(|| Error::Write(format!("unit {} lost its paragraph", u.id)))?;
                    if u.segments.iter().any(|s| !s.target.is_empty()) {
                        translated = true;
                        let o = u.output();
                        if !same_inlines(&o, &u.source()) {
                            changed = true;
                            out.push_str(&render::render(u, &o)?);
                            continue;
                        }
                    }
                    out.push_str(original);
                }
            }
        }
        let stamp = translated
            .then(|| doc.target_lang.as_deref().and_then(fm_language))
            .flatten();
        if changed || stamp.is_some() {
            // every rebuild must still be well-formed MIF (and the language words are found in it)
            let tree = scan(&out, usize::MAX).map_err(|e| match e {
                ScanError::Syntax(m) => Error::Write(format!("the rebuilt file is not well-formed MIF: {m}")),
                ScanError::TooDeep => Error::Write("the rebuilt file is nested too deeply".into()),
            })?;
            if let Some(lang) = stamp {
                out = lang::stamp(&out, &tree, lang);
            }
        }
        let mut bytes = Vec::with_capacity(out.len() + 3);
        if encoding == UTF8_BOM {
            bytes.extend_from_slice(BOM8);
        }
        bytes.extend_from_slice(out.as_bytes());
        Ok(bytes)
    }
}

/// Two inline lists with the same text and the same tags (id and kind; displays are cosmetic).
fn same_inlines(a: &[Inline], b: &[Inline]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| match (x, y) {
            (Inline::Text(p), Inline::Text(q)) => p == q,
            (Inline::Tag { id: i, kind: k, .. }, Inline::Tag { id: j, kind: l, .. }) => i == j && k == l,
            _ => false,
        })
}

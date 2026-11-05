//! Bilingua QuarkXPress Tags (XPress Tags, `.xtg` and `.tag`) filter. See the README.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod attr;
mod codec;
mod lex;
mod para;
mod render;
mod tables;

use bilingua_model::*;
use lex::Item;

/// The XPress Tags filter.
pub struct Xtg;

const MARKERS: [char; 2] = ['\u{27E6}', '\u{27E7}'];

/// Walks the lines of a decoded text: header tags, definitions, `@Name:` style applies, lexing.
struct Walker {
    style: String,
}

enum Line {
    /// Not a paragraph (definition, no letters or digits, or token markers in the text).
    Skip { marker: bool },
    Para,
}

impl Walker {
    /// Lexes line `a..b` into `items`.
    fn line(&mut self, text: &str, mut a: usize, b: usize, line_no: usize, items: &mut Vec<Item>) -> Result<Line, Error> {
        items.clear();
        if line_no == 1 {
            a += lex::header_len(&text[a..b]);
        }
        if lex::is_definition(&text[a..b]) {
            return Ok(Line::Skip { marker: false });
        }
        if let Some((name, len)) = lex::style_apply(&text[a..b]) {
            self.style = if name == "$" { "Normal".into() } else { name.to_string() };
            a += len;
        }
        lex::lex(text, a, b, line_no, items)?;
        let (mut alnum, mut marker) = (false, false);
        for it in items.iter().filter(|i| !i.is_tag) {
            match it.lit {
                Some(c) => alnum |= c.is_alphanumeric(),
                None => {
                    let s = &text[it.a..it.b];
                    alnum = alnum || s.chars().any(char::is_alphanumeric);
                    marker = marker || s.contains(MARKERS);
                }
            }
        }
        if !alnum {
            return Ok(Line::Skip { marker: false });
        }
        if marker {
            return Ok(Line::Skip { marker: true });
        }
        Ok(Line::Para)
    }
}

/// Calls `f(a, b, line_no)` for every line (CR, LF or CRLF), without the line break.
fn for_lines(text: &str, mut f: impl FnMut(usize, usize, usize) -> Result<(), Error>) -> Result<(), Error> {
    let b = text.as_bytes();
    let (mut pos, mut line_no) = (0usize, 0usize);
    while pos < b.len() {
        let (body_end, line_end) = match b[pos..].iter().position(|&c| c == b'\n' || c == b'\r') {
            None => (b.len(), b.len()),
            Some(p) => {
                let e = pos + p;
                let mut le = e + 1;
                if b[e] == b'\r' && b.get(le) == Some(&b'\n') {
                    le += 1;
                }
                (e, le)
            }
        };
        line_no += 1;
        f(pos, body_end, line_no)?;
        pos = line_end;
    }
    Ok(())
}

fn check_tags(text: &str, limits: &Limits) -> Result<(), Error> {
    let cap = lex::TAGS_PER_UNIT.saturating_mul(limits.max_units);
    if text.as_bytes().iter().filter(|&&c| c == b'<').count() > cap {
        return Err(LimitError::new(
            LimitKind::Units,
            format!(
                "This XPress Tags file has more than {cap} tags ({} x the unit limit). Split it into smaller files.",
                lex::TAGS_PER_UNIT
            ),
        )
        .into());
    }
    Ok(())
}

impl Filter for Xtg {
    fn id(&self) -> &'static str {
        "xtg"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["xtg", "tag"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        let head = &bytes[..bytes.len().min(256)];
        let text: String = if head.starts_with(&[0xFF, 0xFE]) || head.starts_with(b"<\0") {
            let h = head.strip_prefix(&[0xFF, 0xFE][..]).unwrap_or(head);
            encoding_rs::UTF_16LE.decode_without_bom_handling(h).0.into_owned()
        } else if head.starts_with(&[0xFE, 0xFF]) || head.starts_with(b"\0<") {
            let h = head.strip_prefix(&[0xFE, 0xFF][..]).unwrap_or(head);
            encoding_rs::UTF_16BE.decode_without_bom_handling(h).0.into_owned()
        } else {
            String::from_utf8_lossy(head.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(head)).into_owned()
        };
        lex::header_len(&text) > 0
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        let limits = opts.limits();
        let d = codec::decode(bytes, opts)?;
        let text = d.text.as_str();
        check_tags(text, &limits)?;
        let mut units: Vec<Unit> = Vec::new();
        let mut parts: Vec<SkelPart> = Vec::new();
        let mut warnings = d.warnings;
        let (mut pos, mut skipped) = (0usize, 0usize);
        let mut walker = Walker { style: String::new() };
        let mut items: Vec<Item> = Vec::new();
        for_lines(text, |a, b, line_no| {
            match walker.line(text, a, b, line_no, &mut items)? {
                Line::Skip { marker } => skipped += usize::from(marker),
                Line::Para => {
                    let built = para::build(text, &items);
                    let mut u = Unit::new(format!("u{}", units.len() + 1), built.inlines);
                    u.codes = built.codes;
                    u.hidden = built.hidden;
                    u.set_hidden(para::RAW, &text[built.start..built.end]);
                    if !walker.style.is_empty() {
                        u.context.style = Some(walker.style.clone());
                    }
                    u.context.location = Some(format!("line {line_no}"));
                    if built.start > pos {
                        parts.push(SkelPart::Raw(text[pos..built.start].to_string()));
                    }
                    pos = built.end;
                    parts.push(SkelPart::Unit(units.len()));
                    units.push(u);
                    guard::check_unit_count(units.len(), &limits)?;
                }
            }
            Ok(())
        })?;
        if pos < text.len() {
            parts.push(SkelPart::Raw(text[pos..].to_string()));
        }
        if skipped > 0 {
            warnings.push(Warning::new(
                "skipped_paragraphs",
                "paragraphs",
                format!("{skipped} paragraph(s) that already contain the characters U+27E6 or U+27E7 were left out."),
            ));
        }
        let doc = Document::new("xtg", units, Skeleton::text(parts, d.enc.tag(), bilingua_model::parser_info!())).with_options(opts);
        guard::finish(doc, warnings, &limits)
    }

    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("xtg needs a text skeleton".into()));
        };
        let enc = codec::Enc::from_tag(encoding).ok_or_else(|| Error::Write(format!("unknown encoding {encoding}")))?;
        let mut out = String::new();
        for p in parts {
            match p {
                SkelPart::Raw(s) => out.push_str(s),
                SkelPart::Unit(i) => {
                    let u = doc.units.get(*i).ok_or_else(|| Error::Write(format!("skeleton points at missing unit {i}")))?;
                    write_unit(u, &mut out);
                }
            }
        }
        // every rebuild must still lex as XPress Tags
        let mut w = Walker { style: String::new() };
        let mut items = Vec::new();
        for_lines(&out, |a, b, n| w.line(&out, a, b, n, &mut items).map(|_| ()))
            .map_err(|e| Error::Write(format!("the merged file is not valid XPress Tags: {e}")))?;
        Ok(bilingua_model::enc::utf8_output(&out, encoding, opts).unwrap_or_else(|| codec::encode(&out, enc)))
    }
}

/// Same text and the same codes in the same order (the display of a code does not matter, its id and kind do).
fn same_content(a: &[Inline], b: &[Inline]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| match (x, y) {
            (Inline::Text(p), Inline::Text(q)) => p == q,
            (Inline::Tag { id: i, kind: k, .. }, Inline::Tag { id: j, kind: l, .. }) => i == j && k == l,
            _ => false,
        })
}

fn write_unit(u: &Unit, out: &mut String) {
    let raw = u.hidden(para::RAW).unwrap_or("");
    if !u.translate || u.segments.iter().all(|s| !s.has_target()) {
        out.push_str(raw);
        return;
    }
    let (src, new) = (u.source(), u.output());
    if same_content(&src, &new) {
        out.push_str(raw);
        return;
    }
    out.push_str(&render::render(&u.codes, &u.hidden, &new));
}

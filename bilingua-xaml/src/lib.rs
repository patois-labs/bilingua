//! Bilingua XAML filter: string resources of WPF, UWP, WinUI, .NET MAUI and Avalonia XAML files. See the README.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod ph;
mod scan;
mod textenc;

use bilingua_model::*;

/// The XAML filter. `view_attributes` also translates the `Content`, `Text`, `ToolTip`, `Header` and `Title`
/// attributes of plain views (off by default, as in LocOS); they come after every resource string.
#[derive(Debug, Clone, Copy, Default)]
pub struct Xaml {
    pub view_attributes: bool,
}

impl Xaml {
    /// The filter with view attributes on.
    pub fn with_view_attributes() -> Xaml {
        Xaml { view_attributes: true }
    }
}

pub use bilingua_model::Limits;

// Hidden codes (ids start with `~`, never a placeholder id).
const RAW: &str = "~raw"; // the source span as written
const META: &str = "~meta"; // `t` text, `tc` text with CRLF, `a"` / `a'` attribute, plus `e` when `{}` escaped

fn hidden(text: impl Into<String>) -> CodeData {
    CodeData { standalone: Some(text.into()), ..Default::default() }
}

fn code(u: &Unit, key: &str) -> Option<String> {
    u.codes.get(key).and_then(|c| c.standalone.clone())
}

fn render(u: &Unit, v: &[Inline]) -> String {
    let mut s = String::new();
    for i in v {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { id, display, .. } => match u.codes.get(id).and_then(|c| c.standalone.as_deref()) {
                Some(c) => s.push_str(c),
                None => s.push_str(display),
            },
        }
    }
    s
}

fn tags_of(value: &str) -> (Vec<Inline>, std::collections::BTreeMap<String, CodeData>) {
    let mut inl = Vec::new();
    let mut codes = std::collections::BTreeMap::new();
    let mut n = 0usize;
    for piece in ph::split(value) {
        match piece {
            ph::Piece::Text(t) => inl.push(Inline::text(t)),
            ph::Piece::Ph(p) => {
                n += 1;
                let id = n.to_string();
                codes.insert(id.clone(), CodeData { standalone: Some(p.to_string()), label: Some("placeholder".into()), ..Default::default() });
                inl.push(if locos::is_named_placeholder(p) { Inline::free_tag(id, p) } else { Inline::tag(id, TagKind::Standalone, p) });
            }
        }
    }
    (inl, codes)
}

/// Keeps XML 1.0 characters, turning pasted Word breaks into newlines.
fn xml_target_text(s: &str) -> String {
    s.chars()
        .map(|c| if matches!(c, '\u{0B}' | '\u{0C}') { '\n' } else { c })
        .filter(|c| !matches!(*c as u32, 0..=8 | 0x0E..=0x1F | 0xFFFE | 0xFFFF))
        .collect()
}

fn escape_text(s: &str, crlf: bool) -> String {
    let s = s.replace("\r\n", "\n").replace('\r', "\n");
    let s = s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    if crlf {
        s.replace('\n', "\r\n")
    } else {
        s
    }
}

fn escape_attr(s: &str, quote: u8) -> String {
    let s = s.replace('&', "&amp;").replace('<', "&lt;");
    let s = if quote == b'"' { s.replace('"', "&quot;") } else { s.replace('\'', "&apos;") };
    s.replace('\t', "&#9;").replace('\n', "&#10;").replace('\r', "&#13;")
}

impl Xaml {
    /// Like [`Filter::extract`] with explicit limits.
    pub fn extract_with(&self, bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<Document, Error> {
        guard::check_input(bytes, limits)?;
        guard::check_xml(bytes, limits)?;
        let dec = enc::decode_text(bytes, &enc::DecodeOptions { declared: true, source_lang: opts.source_lang.as_deref(), limits: *limits })?;
        let text = &dec.text;
        if !matches!(enc::base(&dec.encoding), "utf-8" | "utf-8-bom" | "utf-16le" | "utf-16be") && textenc::encode(&dec.text, &dec.encoding)? != bytes {
            // double-mapped bytes of the CJK code pages read to the same character and write back as the other
            // spelling; an untouched merge must be the original bytes, so the file is refused, never altered
            return Err(Error::Encoding("This XAML file uses byte sequences of its legacy encoding that cannot be written back unchanged. Re-save the file as UTF-8.".into()));
        }
        let items = scan::walk(text, self.view_attributes, limits)?;
        let mut units: Vec<Unit> = Vec::new();
        let (mut n_tr, mut n_lock) = (0usize, 0usize);
        let mut order: Vec<(usize, usize, usize)> = Vec::new(); // (start, end, unit index)
        for it in &items {
            if !it.translatable && !opts.include_locked {
                continue;
            }
            guard::check_unit_count(units.len() + 1, limits)?;
            if it.value.len() > limits.max_unit_chars {
                guard::check_unit_chars(it.value.chars().count(), limits)?;
            }
            let mut unit = if it.translatable {
                n_tr += 1;
                let (inl, codes) = tags_of(&it.value);
                let mut u = Unit::new(format!("u{n_tr}"), inl);
                u.codes = codes;
                u
            } else {
                n_lock += 1;
                let mut u = Unit::new(format!("k{n_lock}"), vec![Inline::text(it.value.clone())]);
                u.translate = false;
                u.lock_reason = Some("markup extension or no letters".into());
                u
            };
            unit.codes.insert(RAW.into(), hidden(&text[it.start..it.end]));
            let mut meta = if it.attr { if it.quote == b'"' { "a\"" } else { "a'" }.to_string() } else if it.crlf { "tc".into() } else { "t".into() };
            if it.escaped {
                meta.push('e');
            }
            unit.codes.insert(META.into(), hidden(meta));
            unit.context.kind = Some(it.kind.clone());
            unit.context.style = Some(it.element.clone());
            unit.context.key = it.key.clone();
            unit.context.notes = it.note.iter().cloned().collect();
            order.push((it.start, it.end, units.len()));
            units.push(unit);
        }
        // skeleton in file order; line numbers counted along the way
        order.sort_unstable();
        let mut parts = Vec::with_capacity(order.len() * 2 + 1);
        let (mut pos, mut line, mut counted) = (0usize, 1usize, 0usize);
        for (start, end, idx) in order {
            line += text.as_bytes()[counted..start].iter().filter(|b| **b == b'\n').count();
            counted = start;
            units[idx].context.location = Some(format!("line {line}"));
            if start > pos {
                parts.push(SkelPart::Raw(text[pos..start].to_string()));
            }
            pos = end;
            parts.push(SkelPart::Unit(idx));
        }
        if pos < text.len() {
            parts.push(SkelPart::Raw(text[pos..].to_string()));
        }
        let doc = Document {
            format: "xaml".into(),
            name: None,
            source_lang: opts.source_lang.clone(),
            target_lang: opts.target_lang.clone(),
            granularity: opts.granularity,
            units,
            skeleton: Skeleton::text(parts, dec.encoding, bilingua_model::parser_info!()),
            warnings: Vec::new(),
        };
        guard::finish(doc, dec.warnings, limits)
    }
}

fn head_text(bytes: &[u8]) -> String {
    let head = &bytes[..bytes.len().min(4096)];
    if head.starts_with(&[0xFF, 0xFE]) {
        return encoding_rs::UTF_16LE.decode_without_bom_handling(&head[2..]).0.into_owned();
    }
    if head.starts_with(&[0xFE, 0xFF]) {
        return encoding_rs::UTF_16BE.decode_without_bom_handling(&head[2..]).0.into_owned();
    }
    String::from_utf8_lossy(head.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(head)).into_owned()
}

impl Filter for Xaml {
    fn id(&self) -> &'static str {
        "xaml"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["xaml"]
    }

    /// Reasonably tight: XML that names a XAML, WPF/UWP, MAUI or Avalonia namespace. Still put it after the
    /// extension-specific formats and before generic XML.
    fn sniff(&self, bytes: &[u8]) -> bool {
        let h = head_text(bytes);
        let t = h.trim_start();
        t.starts_with('<')
            && (h.contains("schemas.microsoft.com/winfx/")
                || h.contains("schemas.microsoft.com/dotnet/")
                || h.contains("github.com/avaloniaui")
                || h.contains("<ResourceDictionary")
                || h.contains("schemas.microsoft.com/xaml"))
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        self.extract_with(bytes, opts, &opts.limits())
    }

    fn merge(&self, doc: &Document) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("xaml needs a text skeleton".into()));
        };
        let refs = char_refs(parts, encoding);
        let mut out = String::new();
        for p in parts {
            match p {
                SkelPart::Raw(s) => out.push_str(s),
                SkelPart::Unit(i) => {
                    let u = doc.units.get(*i).ok_or_else(|| Error::Write(format!("skeleton points at missing unit {i}")))?;
                    write_unit(u, refs, &mut out);
                }
            }
        }
        textenc::encode(&out, encoding)
    }
}

/// Characters a translated span writes as character references whatever the engine's writer would do.
#[derive(Clone, Copy, PartialEq)]
enum CharRefs {
    None,
    /// UTF-8 bytes under an `encoding="us-ascii"` declaration: everything outside ASCII (a .NET reader decodes
    /// the declared ASCII and would turn UTF-8 bytes into `?`).
    NonAscii,
    /// GBK / GB2312: the euro sign (the engine writes GBK through GB18030, whose A2 E3 GBK readers cannot read).
    Euro,
}

fn char_refs(parts: &[SkelPart], encoding: &str) -> CharRefs {
    let base = enc::base(encoding);
    if base == "utf-8" {
        if let Some(SkelPart::Raw(head)) = parts.first() {
            if let Some(decl) = head.strip_prefix("<?xml").and_then(|r| r.split("?>").next()) {
                let d = decl.to_ascii_lowercase().replace(' ', "");
                if ["us-ascii", "ascii"].iter().any(|l| d.contains(&format!("encoding=\"{l}\"")) || d.contains(&format!("encoding='{l}'"))) {
                    return CharRefs::NonAscii;
                }
            }
        }
    } else if bilingua_model::encoding_rs::Encoding::for_label(base.as_bytes()) == Some(bilingua_model::encoding_rs::GBK) {
        return CharRefs::Euro;
    }
    CharRefs::None
}

fn with_char_refs(s: String, refs: CharRefs) -> String {
    let hit = |c: char| match refs {
        CharRefs::None => false,
        CharRefs::NonAscii => !c.is_ascii(),
        CharRefs::Euro => c == '\u{20AC}',
    };
    if !s.chars().any(hit) {
        return s;
    }
    let mut out = String::with_capacity(s.len() + 16);
    for c in s.chars() {
        if hit(c) {
            out.push_str(&format!("&#{};", c as u32));
        } else {
            out.push(c);
        }
    }
    out
}

fn write_unit(u: &Unit, refs: CharRefs, out: &mut String) {
    let raw = code(u, RAW).unwrap_or_default();
    if !u.translate || u.segments.iter().all(|s| s.target.is_empty()) {
        out.push_str(&raw);
        return;
    }
    let new = render(u, &u.output());
    if new == render(u, &u.source()) {
        out.push_str(&raw);
        return;
    }
    let new = xml_target_text(&new);
    let meta = code(u, META).unwrap_or_default();
    let escaped = meta.ends_with('e');
    if let Some(rest) = meta.strip_prefix('a') {
        let quote = if rest.starts_with('"') { b'"' } else { b'\'' };
        let mut v = new;
        if !escaped && v.trim_start().starts_with('{') {
            v.insert_str(0, "{}"); // never let a translation turn into a markup extension
        }
        out.push_str(&with_char_refs(escape_attr(&v, quote), refs));
    } else {
        out.push_str(&with_char_refs(escape_text(&new, meta.starts_with("tc")), refs));
    }
}

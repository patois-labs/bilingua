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

/// Size limits (the shape of `bilingua_model::Limits` once B6 is in the tree). Refusals are
/// `Error::Unsupported("input limit: ...")`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_input_bytes: u64,
    pub max_units: usize,
    pub max_unit_chars: usize,
    pub max_xml_depth: usize,
    pub max_xml_elements: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits { max_input_bytes: 100 << 20, max_units: 50_000, max_unit_chars: 100_000, max_xml_depth: 256, max_xml_elements: 4_000_000 }
    }
}

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
        if bytes.len() as u64 > limits.max_input_bytes {
            return Err(scan::limit_err(format!("the file is larger than {} bytes", limits.max_input_bytes)));
        }
        let dec = textenc::decode(bytes)?;
        let text = &dec.text;
        let items = scan::walk(text, self.view_attributes, limits)?;
        let mut units: Vec<Unit> = Vec::new();
        let (mut n_tr, mut n_lock) = (0usize, 0usize);
        let mut order: Vec<(usize, usize, usize)> = Vec::new(); // (start, end, unit index)
        for it in &items {
            if !it.translatable && !opts.include_locked {
                continue;
            }
            if units.len() >= limits.max_units {
                return Err(scan::limit_err(format!("the file has more than {} units", limits.max_units)));
            }
            if it.value.len() > limits.max_unit_chars && it.value.chars().count() > limits.max_unit_chars {
                return Err(scan::limit_err(format!("a unit is longer than {} characters", limits.max_unit_chars)));
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
        Ok(Document {
            format: "xaml".into(),
            name: None,
            source_lang: opts.source_lang.clone(),
            target_lang: opts.target_lang.clone(),
            granularity: opts.granularity,
            units,
            skeleton: Skeleton::text(parts, dec.tag, bilingua_model::parser_info!()),
        })
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
        self.extract_with(bytes, opts, &Limits::default())
    }

    fn merge(&self, doc: &Document) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("xaml needs a text skeleton".into()));
        };
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
        textenc::encode(&out, encoding)
    }
}

fn write_unit(u: &Unit, out: &mut String) {
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
        out.push_str(&escape_attr(&v, quote));
    } else {
        out.push_str(&escape_text(&new, meta.starts_with("tc")));
    }
}

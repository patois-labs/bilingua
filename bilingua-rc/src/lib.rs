//! Bilingua Windows resource script (`.rc`) filter. See the README.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod accel;
mod codec;
mod langid;
mod lex;
mod lit;
mod sbcs;
mod script;
mod winlocale;

use accel::Item;
use bilingua_model::*;
use regex::Regex;
use script::Script;
use std::collections::HashSet;
use std::sync::OnceLock;

/// The Windows resource script filter.
pub struct Rc;

// Reserved code ids (start with `~`, never a numeric tag id).
const RAW: &str = "raw"; // the literal as written, quotes and L prefix included
const ACC: &str = "acc"; // present on menu items and dialog controls: the `&` accelerator is kept

fn limit_err(msg: String) -> Error {
    LimitError::new(LimitKind::Units, msg).into()
}

fn items_of(v: &[Inline]) -> Vec<Item> {
    let mut out = Vec::new();
    for i in v {
        match i {
            Inline::Text(t) => out.extend(t.chars().map(Item::Ch)),
            Inline::Tag { id, .. } => out.push(Item::Code(id.clone())),
        }
    }
    out
}

fn raw_of(u: &Unit, id: &str) -> Option<String> {
    u.codes.get(id).and_then(|c| c.standalone.clone())
}

fn render(u: &Unit, items: &[Item], wide: bool) -> String {
    let mut s = String::with_capacity(items.len() + 4);
    s.push_str(if wide { "L\"" } else { "\"" });
    for it in items {
        match it {
            Item::Ch(c) => lit::escape_char(*c, wide, &mut s),
            // an unknown code is dropped (same tolerance as the other formats)
            Item::Code(id) => {
                if let Some(raw) = u.codes.get(id).and_then(|c| c.standalone.as_deref()) {
                    s.push_str(raw)
                }
            }
        }
    }
    s.push('"');
    s
}

/// Refuses a rendered literal that would not read back as the same unit: it must lex as one string token, and
/// when the translation has a letter or digit, one must survive outside the codes. Translated text is written
/// as is (LocOS parity), so a `%` the translator put before a letter can join it into a printf placeholder
/// (`%E`, `%d`); when that swallows every letter, the literal would read back as codes only and the unit
/// would vanish from the next extract. That file is refused rather than written.
fn check_rendered(u: &Unit, target: &[Item], s: &str) -> Result<(), Error> {
    let toks = lex::lex(s);
    let one_literal = matches!(toks.as_slice(), [t] if t.kind == lex::Kind::Str && t.start == 0 && t.end == s.len());
    if !one_literal {
        return Err(Error::Unwritable { unit: Some(u.id.clone()), reason: "the translation does not form one string literal".into() });
    }
    // a short escape (`\x4`, `\1`) followed by a translated digit would read as a longer one (`\x41`, `\12`) (#791)
    let maxd = if s.starts_with('L') { 4 } else { 2 };
    for w in target.windows(2) {
        let (Item::Code(id), Item::Ch(c)) = (&w[0], &w[1]) else { continue };
        let Some(raw) = u.codes.get(id).and_then(|c| c.standalone.as_deref()) else { continue };
        let joins = match raw.as_bytes() {
            [b'\\', b'x' | b'X', d @ ..] => d.len() < maxd && c.is_ascii_hexdigit(),
            [b'\\', o, d @ ..] if (b'0'..=b'7').contains(o) => d.len() + 1 < 3 && ('0'..='7').contains(c),
            _ => false,
        };
        if joins {
            return Err(Error::Unwritable {
                unit: Some(u.id.clone()),
                reason: format!("the character after the escape {raw} would join it into a different character; put a space or other text between them"),
            });
        }
    }
    let has_text = target.iter().any(|it| matches!(it, Item::Ch(c) if c.is_alphanumeric()));
    let (pieces, _) = lit::pieces(s);
    let keeps_text = pieces.iter().any(|p| matches!(p, lit::Piece::Text(t) if t.chars().any(char::is_alphanumeric)));
    if has_text && !keeps_text {
        return Err(Error::Unwritable {
            unit: Some(u.id.clone()),
            reason: "every letter of the translation would be read as a format placeholder (a % right before a \
                     letter, such as %E or %d), so the string would be lost; put a space after the % or rephrase"
                .into(),
        });
    }
    Ok(())
}

/// Writes one unit; true when it changed the literal.
fn write_unit(u: &Unit, out: &mut String) -> Result<bool, Error> {
    let raw = u.hidden(RAW).unwrap_or("");
    if !u.translate || u.segments.iter().all(|s| !s.has_target()) {
        out.push_str(raw);
        return Ok(false);
    }
    let (src, new) = (u.source(), u.output());
    if src == new {
        out.push_str(raw);
        return Ok(false);
    }
    let mut target = items_of(&new);
    if u.has_hidden(ACC) {
        target = accel::fix(&items_of(&src), target, &|id| raw_of(u, id));
    }
    let s = render(u, &target, raw.starts_with('L'));
    let changed = s != raw;
    if changed {
        check_rendered(u, &target, &s)?;
    }
    out.push_str(&s);
    Ok(changed)
}

fn stamp_edits(sc: &Script, text: &str, locale: &str) -> Vec<(usize, usize, String)> {
    let Some(lcid) = langid::langid(locale) else { return Vec::new() };
    let mut edits = Vec::new();
    let norm: HashSet<String> =
        sc.langs.iter().map(|&(a, b)| text[a..b].chars().filter(|c| !c.is_whitespace()).collect::<String>().to_uppercase()).collect();
    if norm.len() == 1 {
        let val = format!("0x{:02x}, 0x{:02x}", lcid & 0x3FF, lcid >> 10);
        edits.extend(sc.langs.iter().map(|&(a, b)| (a, b, val.clone())));
    }
    if sc.ver_blocks.len() == 1 {
        let (a, b) = sc.ver_blocks[0];
        let new = format!("{lcid:04x}");
        let upper = text[a..(a + 8).min(text.len())].chars().any(|c| "ABCDEF".contains(c));
        edits.push((a, b, if upper { new.to_uppercase() } else { new }));
    }
    if sc.ver_trans.len() == 1 {
        let (a, b) = sc.ver_trans[0];
        let old = &text[a..b];
        edits.push((a, b, if old.to_ascii_lowercase().starts_with("0x") { format!("0x{lcid:x}") } else { lcid.to_string() }));
    }
    edits
}

impl Filter for Rc {
    fn id(&self) -> &'static str {
        "rc"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["rc"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| {
            Regex::new(
                r#"(?mi)^[ \t]*(?:STRINGTABLE\b|#[ \t]*include[ \t]*["<](?:winres|afxres|windows)\.h|#[ \t]*pragma[ \t]+code_page\b|[A-Za-z_0-9"]+[ \t]+(?:DIALOGEX?|MENUEX?|VERSIONINFO|ACCELERATORS|DESIGNINFO)\b)"#,
            )
            .expect("sniff pattern")
        });
        let head = &bytes[..bytes.len().min(8192)];
        let text = if head.starts_with(&[0xFF, 0xFE]) || head.starts_with(&[0xFE, 0xFF]) {
            let e = if head[0] == 0xFF { encoding_rs::UTF_16LE } else { encoding_rs::UTF_16BE };
            e.decode_without_bom_handling(&head[2..]).0.into_owned()
        } else {
            String::from_utf8_lossy(head).into_owned()
        };
        re.is_match(&text)
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        let limits = opts.limits();
        guard::check_input(bytes, &limits)?;
        let d = codec::decode(bytes)?;
        guard::check_lines(&d.text, &limits)?;
        let quotes = d.text.bytes().filter(|&b| b == b'"').count();
        if quotes > 40usize.saturating_mul(limits.max_units) {
            return Err(limit_err(format!(
                "This resource script has far more strings than the limit allows ({}). Split it into smaller files.",
                limits.max_units
            )));
        }
        let mut warnings = Vec::new();
        if let (Some(e), Some(lang)) = (codec::rs_encoding(&d.tag), opts.source_lang.as_deref()) {
            if let Some(w) = enc::warn_if_garbled(&d.text, e, lang, None) {
                warnings.push(w);
            }
        }
        if codec::round_trip_differs(&d.tag, &d.text, bytes) {
            warnings.push(Warning::new(
                "text_encoding",
                "text encoding",
                "Some bytes of this file have more than one spelling in its code page, so saving it may normalise them.",
            ));
        }
        let sc = Script::new(&d.text, true, opts.include_locked, &limits)?;
        let mut units: Vec<Unit> = Vec::with_capacity(sc.units.len());
        let mut parts: Vec<SkelPart> = Vec::new();
        let mut pos = 0usize;
        let (mut n_tr, mut n_lock) = (0usize, 0usize);
        for f in &sc.units {
            let (inl, codes) = script::inlines(&f.pieces);
            let mut u = if f.locked {
                n_lock += 1;
                let mut u = Unit::new(format!("k{n_lock}"), inl);
                u.translate = false;
                u.lock_reason = Some("no translatable text".into());
                u
            } else {
                n_tr += 1;
                Unit::new(format!("u{n_tr}"), inl)
            };
            for (id, raw) in codes {
                let label = if lit::is_format(&raw) { "format" } else { "escape" };
                u.codes.insert(id, CodeData::new().with_standalone(raw).with_label(label.to_string()));
            }
            u.set_hidden(RAW, d.text[f.start..f.end].to_string());
            if matches!(f.style, "rc:menu" | "rc:control") && !f.locked {
                u.set_hidden(ACC, "1");
            }
            u.context.kind = Some(f.style.trim_start_matches("rc:").to_string());
            u.context.style = Some(f.style.to_string());
            u.context.location = Some(format!("line {}", sc.line(f.start)));
            if f.start > pos {
                parts.push(SkelPart::Raw(d.text[pos..f.start].to_string()));
            }
            pos = f.end;
            parts.push(SkelPart::Unit(units.len()));
            units.push(u);
        }
        if pos < d.text.len() {
            parts.push(SkelPart::Raw(d.text[pos..].to_string()));
        }
        let doc = Document::new("rc", units, Skeleton::text(parts, d.tag, bilingua_model::parser_info!())).with_options(opts);
        guard::finish(doc, warnings, &limits)
    }

    /// `stamp_target_always` writes the target LANGUAGE even when nothing is translated. `output_encoding: Utf8`
    /// writes UTF-8 (a UTF-8 BOM is kept; a UTF-16 BOM or code page is not); `Keep` keeps the file's encoding.
    fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("rc needs a text skeleton".into()));
        };
        let known = codec::known_tag(encoding);
        if !known {
            return Err(Error::Write(format!("unknown encoding {encoding}")));
        }
        let mut out = String::new();
        let mut edited = false;
        for p in parts {
            match p {
                SkelPart::Raw(s) => out.push_str(s),
                SkelPart::Unit(i) => {
                    let u = doc.units.get(*i).ok_or_else(|| Error::Write(format!("skeleton points at missing unit {i}")))?;
                    edited |= write_unit(u, &mut out)?;
                }
            }
        }
        // the target language: only when something was translated
        let translated = doc.units.iter().any(|u| u.translate && u.segments.iter().any(|s| s.has_target()));
        if let Some(loc) = opts.stamp_lang(doc, translated) {
            let sc = Script::new(&out, false, false, &Limits::default())?;
            let mut edits = stamp_edits(&sc, &out, loc);
            edits.retain(|(a, b, v)| out[*a..*b] != *v);
            if !edits.is_empty() {
                edits.sort();
                let mut s = String::with_capacity(out.len() + 16);
                let mut pos = 0;
                for (a, b, v) in &edits {
                    s.push_str(&out[pos..*a]);
                    s.push_str(v);
                    pos = *b;
                }
                s.push_str(&out[pos..]);
                out = s;
                edited = true;
            }
        }
        if let Some(b) = bilingua_model::enc::utf8_output(&out, encoding, opts) {
            return Ok(b);
        }
        Ok(codec::encode(&out, encoding, edited))
    }
}

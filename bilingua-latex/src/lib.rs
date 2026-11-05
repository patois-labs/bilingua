//! Bilingua LaTeX filter (`.tex`, `.latex`, `.ltx`). See the README.
//!
//! A scanner over the source, never a TeX engine: nothing is executed, expanded or followed. The units are the
//! prose of the document body (paragraphs, section and caption arguments, `\item[labels]`, table cells), with text
//! commands (`\emph`, `\textbf`, `\footnote`, `\href{url}{text}` ...) as paired tags and every other command, comment
//! and piece of math as a standalone tag. The preamble, math, verbatim environments and comments stay in the
//! skeleton byte for byte.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod chars;
mod inline;
mod scan;

use bilingua_model::*;
use regex::Regex;
use std::sync::OnceLock;

/// The hidden code of a unit: `standalone` = the source text as written.
const RAW: &str = "~raw";

/// The LaTeX filter. `extra_inline` adds inline text commands by name (LocOS `LATEX_TEXT_COMMANDS`): their last
/// argument is text and the command stays in the markup. It changes the units of a file, so give the same list when
/// extracting again.
#[derive(Debug, Clone, Default)]
pub struct Latex {
    pub extra_inline: Vec<String>,
}

impl Latex {
    /// A filter with extra inline text commands (names of ASCII letters only, 1 to 50 of them; others are ignored).
    pub fn with_inline_commands<I: IntoIterator<Item = S>, S: AsRef<str>>(names: I) -> Latex {
        Latex {
            extra_inline: names
                .into_iter()
                .map(|n| n.as_ref().trim().to_string())
                .filter(|n| (1..=50).contains(&n.len()) && n.bytes().all(|b| b.is_ascii_alphabetic()))
                .collect(),
        }
    }
}

fn inputenc_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(\\usepackage\s*\[)\s*(latin[1-9]|latin10|cp125[0-8]|cp437|cp850|cp852|cp858|cp866|cp1250|ansinew|applemac|macce|koi8-[ru]|next|decmulti)\s*(\]\s*\{inputenc\})",
        )
        .expect("inputenc pattern")
    })
}

fn sniff_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?m)^[ \t]*\\(?:documentclass|usepackage|RequirePackage|begin\s*\{document\})").expect("sniff pattern"))
}

fn limit_err(msg: String) -> Error {
    LimitError::new(LimitKind::InputBytes, msg).into()
}

impl Filter for Latex {
    fn id(&self) -> &'static str {
        "tex"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["tex", "latex", "ltx"]
    }

    /// Strict: a `\documentclass`, `\usepackage`, `\RequirePackage` or `\begin{document}` at a line start in the first
    /// 8 KB. A chapter file without any of them is found by its extension only.
    fn sniff(&self, bytes: &[u8]) -> bool {
        let head = &bytes[..bytes.len().min(8192)];
        sniff_re().is_match(&String::from_utf8_lossy(head))
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        let limits = opts.limits();
        let mut dec = enc::decode_text(bytes, &enc::DecodeOptions::new(opts))?;
        let text = std::mem::take(&mut dec.text);
        if text.len() >= u32::MAX as usize {
            return Err(limit_err("This file is too large to read; split it into smaller files.".into()));
        }
        let tx = scan::Txt::new(&text);
        let mut sc = scan::Scan::new(&tx, limits);
        sc.run()?;
        let mut units: Vec<Unit> = Vec::new();
        let mut parts: Vec<SkelPart> = Vec::new();
        let mut pos = 0usize;
        let (mut line_pos, mut line) = (0usize, 1usize);
        let (mut n_tr, mut n_lock) = (0usize, 0usize);
        for s in &sc.slots {
            let raw = &text[s.a..s.b];
            let (pieces, map) = inline::tokenize_slot(raw, &self.extra_inline);
            let tr = inline::translatable(&pieces);
            if !(tr || opts.include_locked && !pieces.is_empty()) {
                continue;
            }
            let (inl, codes) = inline::to_inlines(&pieces, &map);
            let mut u = if tr {
                n_tr += 1;
                Unit::new(format!("u{n_tr}"), inl)
            } else {
                n_lock += 1;
                let mut u = Unit::new(format!("k{n_lock}"), inl);
                u.translate = false;
                u.lock_reason = Some("no translatable text".into());
                u
            };
            u.codes.extend(codes);
            u.codes.insert(RAW.into(), CodeData { standalone: Some(raw.to_string()), ..Default::default() });
            line += text.as_bytes()[line_pos..s.a].iter().filter(|&&b| b == b'\n').count();
            line_pos = s.a;
            u.context.kind = Some(s.kind.to_string());
            u.context.style = Some("tex".into());
            u.context.location = Some(format!("line {line}"));
            if s.a > pos {
                parts.push(SkelPart::Raw(text[pos..s.a].to_string()));
            }
            pos = s.b;
            parts.push(SkelPart::Unit(units.len()));
            units.push(u);
        }
        if pos < text.len() {
            parts.push(SkelPart::Raw(text[pos..].to_string()));
        }
        let doc = Document {
            format: "tex".into(),
            source_lang: opts.source_lang.clone(),
            target_lang: opts.target_lang.clone(),
            granularity: opts.granularity,
            units,
            skeleton: Skeleton::text(parts, dec.encoding.as_str(), bilingua_model::parser_info!()),
            ..Default::default()
        };
        guard::finish(doc, std::mem::take(&mut dec.warnings), &limits)
    }

    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("tex needs a text skeleton".into()));
        };
        let mut out = String::new();
        let mut changed = false;
        for p in parts {
            match p {
                SkelPart::Raw(s) => out.push_str(s),
                SkelPart::Unit(i) => {
                    let u = doc.units.get(*i).ok_or_else(|| Error::Write(format!("skeleton points at missing unit {i}")))?;
                    let raw = u.codes.get(RAW).and_then(|c| c.standalone.as_deref()).unwrap_or("");
                    if !u.translate || u.segments.iter().all(|s| s.target.is_empty()) {
                        out.push_str(raw);
                        continue;
                    }
                    let (src, new) = (u.source(), u.output());
                    if inline::same(&new, &src) {
                        out.push_str(raw);
                    } else {
                        out.push_str(&inline::render(u, &new));
                        changed = true;
                    }
                }
            }
        }
        let bytes = enc::encode_text(&out, encoding)?;
        let utf8_out = opts.output_encoding == bilingua_model::OutputEncoding::Utf8;
        if enc::is_legacy(encoding) && (utf8_out || (changed && fell_back_to_utf8(&bytes, &out, encoding))) {
            // The translation did not fit the file's code page and went out as UTF-8: an `inputenc` option that
            // names the old code page would now be false. Only the preamble is rewritten: the same line in the body
            // (a verbatim example, a comment) is text and stays as written.
            let cut = scan::document_start(&out).unwrap_or(out.len());
            let head = inputenc_re().replace_all(&out[..cut], |c: &regex::Captures| format!("{}utf8{}", &c[1], &c[3]));
            let fixed = format!("{head}{}", &out[cut..]);
            return enc::encode_text(&fixed, "utf-8");
        }
        if let Some(b) = enc::utf8_output(&out, encoding, opts) {
            return Ok(b);
        }
        Ok(bytes)
    }
}

/// True when `bytes` (from `encode_text` for a legacy `encoding`) are UTF-8 rather than that code page.
fn fell_back_to_utf8(bytes: &[u8], text: &str, encoding: &str) -> bool {
    if text.is_ascii() {
        return false;
    }
    let base = enc::base(encoding);
    let back = if base == "iso-8859-1" {
        Some(bytes.iter().map(|&b| b as char).collect::<String>())
    } else {
        encoding_rs::Encoding::for_label(base.as_bytes()).and_then(|e| enc::decode_strict(e, bytes).ok())
    };
    back.is_none_or(|b| b != text)
}

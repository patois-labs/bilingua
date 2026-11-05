//! Reading and writing text files in the encoding they came in (Localization OS `read_text_upload`, decisions 22
//! and B-bytes).
//!
//! **Reading** ([`decode_text`]): UTF-8 first (a UTF-8 BOM is kept); then a UTF-16 BOM; then, for formats that
//! declare their charset (HTML, XML, PO: [`DecodeOptions::declared`]), the declared legacy charset; then a
//! binary-file check; then the Windows code page of the project's source language (Greek read as Windows-1253,
//! Japanese as Shift_JIS, ...) with Windows-1252 as the default. When the letters of a legacy decode look wrong
//! ([`crate::garble`]) the result carries a [`Warning`], never an error. A file no encoding can read is refused
//! with [`Error::Encoding`] and a "re-save as UTF-8" message.
//!
//! **Writing** ([`encode_text`]): the merged text goes back out in the original's envelope: its byte-order mark, its
//! line endings (when it used one style throughout) and its encoding when every character fits; a translation
//! that does not fit a legacy code page is written as UTF-8 instead (the declaration, if any, then says so).
//!
//! The envelope travels in the skeleton's `encoding` string, which stays `"utf-8"` / `"utf-8-bom"` for the common
//! files. Grammar: `base[;nl=crlf|cr][;label=<declared charset>]`, with `base` one of `utf-8`, `utf-8-bom`,
//! `utf-16le`, `utf-16be` (both with a BOM) or an `encoding_rs` name such as `windows-1253` or `shift_jis`.

use crate::codepage::{label as cp_label, source_encoding};
use crate::garble::text_looks_garbled;
use crate::guard::{check_lines, Limits};
use crate::{Error, ExtractOptions, MergeOptions, OutputEncoding, Warning};
use encoding_rs::{DecoderResult, Encoding};
use regex::Regex;
use std::sync::OnceLock;

const BOM8: &[u8] = &[0xEF, 0xBB, 0xBF];
const RESAVE: &str = "Check the source text; if it is garbled, re-save the file as UTF-8 and upload it again.";

/// How one text file is read.
#[derive(Debug, Clone, Copy)]
pub struct DecodeOptions<'a> {
    /// The format declares its charset in the file (HTML `meta`, XML declaration, PO `Content-Type`): honour a
    /// declared legacy charset, and write the declaration back when the file is merged.
    pub declared: bool,
    /// The project's source language, which picks the legacy code page of an undeclared non-UTF-8 file.
    pub source_lang: Option<&'a str>,
    pub limits: Limits,
}

impl<'a> DecodeOptions<'a> {
    /// The options of one extraction: its source language and its limits.
    pub fn new(opts: &'a ExtractOptions) -> DecodeOptions<'a> {
        DecodeOptions { declared: false, source_lang: opts.source_lang.as_deref(), limits: opts.limits() }
    }
    /// The same, for a format that declares its charset in the file.
    pub fn declared(mut self) -> Self {
        self.declared = true;
        self
    }
}

/// A decoded text file: the text (without BOM), the envelope tag for the skeleton and any warnings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub text: String,
    /// Goes into `Skeleton::Text::encoding`; hand it back to [`encode_text`] on merge.
    pub encoding: String,
    pub warnings: Vec<Warning>,
}

fn enc_err(msg: impl Into<String>) -> Error {
    Error::Encoding(msg.into())
}

fn decl_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(
            r#"(?i)(<\?xml[^>]{0,200}?\bencoding\s*=\s*["']\s*|<meta\b[^>]{0,400}?\bcharset\s*=\s*["']?\s*|Content-Type:[^"\n]{0,100}?\bcharset\s*=\s*)([A-Za-z0-9_.:-]{1,40})"#,
        )
        .expect("charset declaration pattern")
    })
}

/// Charsets a file may declare and be read as: the common legacy text encodings (not UTF-8, UTF-16 or the
/// Windows-1252 family, which the default path reads).
fn declared_ok(e: &'static Encoding) -> bool {
    use encoding_rs::*;
    [
        WINDOWS_1250, WINDOWS_1251, WINDOWS_1253, WINDOWS_1254, WINDOWS_1255, WINDOWS_1256, WINDOWS_1257, WINDOWS_1258, ISO_8859_2,
        ISO_8859_5, ISO_8859_7, ISO_8859_15, KOI8_R, KOI8_U, SHIFT_JIS, EUC_JP, GBK, GB18030, BIG5, EUC_KR,
    ]
    .contains(&e)
}

fn ascii_head(data: &[u8], n: usize) -> String {
    data.iter().take(n).map(|b| if b.is_ascii() { *b as char } else { '?' }).collect()
}

/// True for the legacy charsets a file may declare and be read as (see [`decode_text`]).
pub fn is_declared_legacy(e: &'static Encoding) -> bool {
    declared_ok(e)
}

/// Decodes all of `data` as `enc`, or says where the first undecodable byte is.
pub fn decode_strict(enc: &'static Encoding, data: &[u8]) -> Result<String, usize> {
    strict(enc, data)
}

/// The encoding of a skeleton tag without its newline and label parts ("windows-1253;nl=crlf" is "windows-1253").
pub fn base(tag: &str) -> &str {
    tag.split(';').next().unwrap_or(tag)
}

/// The charset a file declares in its first 4 KB, as a decoder.
pub fn declared_encoding(data: &[u8]) -> Option<&'static Encoding> {
    let head = ascii_head(data, 4096);
    let m = decl_re().captures(&head)?;
    Encoding::for_label(m.get(2)?.as_str().as_bytes())
}

fn declared_label(data: &[u8]) -> Option<String> {
    let head = ascii_head(data, 4096);
    Some(decl_re().captures(&head)?.get(2)?.as_str().to_string())
}

/// Whether bytes that are not UTF-8 would be refused as binary by [`decode_text`]: a NUL, or more C0 controls than a
/// text file carries. A writer that puts text back in a legacy code page checks its output with this, so a merge
/// never writes a file that the same extract refuses.
pub fn looks_binary(data: &[u8]) -> bool {
    if data.contains(&0) {
        return true;
    }
    // C0 controls a text file never carries (TAB, LF, VT, FF, CR, SUB and ESC are allowed).
    let bad = data.iter().filter(|b| **b < 0x20 && !matches!(**b, 0x09 | 0x0A | 0x0B | 0x0C | 0x0D | 0x1A | 0x1B)).count();
    bad > 2 + data.len() / 100
}

/// Decodes all of `data` as `enc` or says where the first undecodable byte is. Windows-1252 is strict like
/// Python's `cp1252`: its five undefined bytes are errors, not C1 controls.
fn strict(enc: &'static Encoding, data: &[u8]) -> Result<String, usize> {
    if enc == encoding_rs::WINDOWS_1252 {
        if let Some(p) = data.iter().position(|b| matches!(b, 0x81 | 0x8D | 0x8F | 0x90 | 0x9D)) {
            return Err(p);
        }
    }
    let mut dec = enc.new_decoder_without_bom_handling();
    let mut out = String::with_capacity(dec.max_utf8_buffer_length_without_replacement(data.len()).unwrap_or(data.len() * 4));
    let (res, read) = dec.decode_to_string_without_replacement(data, &mut out, true);
    match res {
        DecoderResult::InputEmpty => Ok(out),
        DecoderResult::Malformed(bad, after) => Err(read.saturating_sub(bad as usize + after as usize)),
        DecoderResult::OutputFull => Err(read),
    }
}

/// The line-ending style of a text when it uses one throughout: `"\r\n"` (every `\n` follows `\r` and every `\r`
/// precedes `\n`), `"\r"` (no `\n` at all), else `""`.
pub fn newline_style(text: &str) -> &'static str {
    let b = text.as_bytes();
    let (mut crlf, mut lf, mut cr) = (0usize, 0usize, 0usize);
    for (i, c) in b.iter().enumerate() {
        match c {
            b'\n' => {
                lf += 1;
                if i > 0 && b[i - 1] == b'\r' {
                    crlf += 1;
                }
            }
            b'\r' => cr += 1,
            _ => {}
        }
    }
    if crlf > 0 && crlf == lf && lf == cr {
        "\r\n"
    } else if cr > 0 && lf == 0 {
        "\r"
    } else {
        ""
    }
}

fn tag(base: &str, text: &str, label: Option<&str>) -> String {
    let mut t = base.to_string();
    match newline_style(text) {
        "\r\n" => t.push_str(";nl=crlf"),
        "\r" => t.push_str(";nl=cr"),
        _ => {}
    }
    if let Some(l) = label {
        t.push_str(";label=");
        t.push_str(l);
    }
    t
}

/// The skeleton base for a file read as `enc` under a declared `label`. WHATWG (and so `encoding_rs`) reads
/// ISO-8859-1 as Windows-1252; a file that declares ISO-8859-1 and has none of the bytes 0x80..0x9F (where the two
/// differ) is tagged `iso-8859-1`, so a merge writes true ISO-8859-1 and a translation with a character outside it
/// (French oe, the euro sign) becomes UTF-8, never Windows-1252 bytes under an ISO-8859-1 label (LocOS
/// `text_envelope`, B-bytes gate). A file that does carry those bytes keeps the Windows-1252 tag, so an untouched
/// merge stays byte-identical.
fn declared_base(enc: &'static Encoding, label: Option<&str>, data: &[u8]) -> String {
    if enc == encoding_rs::WINDOWS_1252 && label.is_some_and(is_latin1_label) && !data.iter().any(|b| (0x80..=0x9F).contains(b)) {
        return LATIN1.to_string();
    }
    enc.name().to_ascii_lowercase()
}

const LATIN1: &str = "iso-8859-1";

fn is_latin1_label(l: &str) -> bool {
    matches!(
        l.to_ascii_lowercase().as_str(),
        "iso-8859-1" | "iso8859-1" | "iso88591" | "iso_8859-1" | "iso_8859-1:1987" | "latin1" | "l1" | "cp819" | "ibm819" | "csisolatin1" | "iso-ir-100"
    )
}

fn rewrite_declared(text: &str) -> String {
    decl_re().replace_all(text, |c: &regex::Captures| format!("{}UTF-8", &c[1])).into_owned()
}

/// `text` with every charset declaration that names a charset other than UTF-8 (`<?xml encoding>`, `<meta
/// charset>`, a PO `Content-Type`) rewritten to say UTF-8. For a writer that produces UTF-8 itself.
pub fn declare_utf8(text: &str) -> String {
    // declarations live at the top of a file; text further down that looks like one is content
    let mut cut = text.len().min(4096);
    while !text.is_char_boundary(cut) {
        cut -= 1;
    }
    let (head, tail) = text.split_at(cut);
    let mut out = decl_re()
        .replace_all(head, |c: &regex::Captures| {
            if c[2].eq_ignore_ascii_case("UTF-8") || c[2].eq_ignore_ascii_case("UTF8") {
                c[0].to_string()
            } else {
                format!("{}UTF-8", &c[1])
            }
        })
        .into_owned();
    out.push_str(tail);
    out
}

fn done(text: String, base: &str, label: Option<&str>, warnings: Vec<Warning>, l: &Limits) -> Result<Decoded, Error> {
    check_lines(&text, l)?;
    let encoding = tag(base, &text, label);
    Ok(Decoded { text, encoding, warnings })
}

/// Reads a text file (see the module docs for the order of attempts).
pub fn decode_text(data: &[u8], o: &DecodeOptions) -> Result<Decoded, Error> {
    let mut d = decode_text_inner(data, o)?;
    // GBK files from Windows write the euro sign as the single byte 0x80; the GB18030 spelling is A2 E3. A file
    // that uses A2 E3 keeps it on merge (an untouched merge is byte-identical); the tag says so.
    if base(&d.encoding) == "gbk" && d.text.contains('\u{20AC}') && gbk_has_a2e3(data) {
        d.encoding.push_str(";euro=a2e3");
    }
    Ok(d)
}

/// True when the GBK / GB18030 bytes hold the character A2 E3 (not the two bytes straddling two characters, such as
/// a trail byte A2 followed by a lead byte E3).
fn gbk_has_a2e3(data: &[u8]) -> bool {
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        if !(0x81..=0xFE).contains(&b) || i + 1 >= data.len() {
            i += 1;
        } else if (0x30..=0x39).contains(&data[i + 1]) {
            i += 4; // a GB18030 four-byte form
        } else {
            if b == 0xA2 && data[i + 1] == 0xE3 {
                return true;
            }
            i += 2;
        }
    }
    false
}

fn decode_text_inner(data: &[u8], o: &DecodeOptions) -> Result<Decoded, Error> {
    crate::guard::check_input(data, &o.limits)?;
    let locale = o.source_lang.unwrap_or("");
    let hint = source_encoding(locale);
    let (body, bom) = match data.strip_prefix(BOM8) {
        Some(r) => (r, true),
        None => (data, false),
    };
    if let Ok(s) = std::str::from_utf8(body) {
        let mut w = Vec::new();
        if let Some(h) = hint {
            if let Some(m) = legacy_as_utf8(body, s, h, locale) {
                w.push(m);
            }
        }
        if o.declared && !bom && s.is_ascii() {
            // a pure-ASCII file that declares another charset (an ISO-8859-2 PO file without accents yet): read
            // it as that charset, so text added to it later is written in it, not as UTF-8 under its declaration
            let named = declared_label(body).map(|l| l.to_ascii_lowercase());
            let skip = matches!(named.as_deref(), None | Some("utf-8" | "utf8" | "us-ascii" | "ascii" | "charset"));
            if let (false, Some(e)) = (skip, declared_encoding(body)) {
                if e != encoding_rs::UTF_8 && e != encoding_rs::UTF_16LE && e != encoding_rs::UTF_16BE {
                    let label = declared_label(body);
                    return done(rewrite_declared(s), &declared_base(e, label.as_deref(), body), label.as_deref(), w, &o.limits);
                }
            }
        }
        // a UTF-8 file that declares another charset keeps the label in its tag, so a merge asked for UTF-8
        // ([`MergeOptions`]) can rewrite the false declaration; the bytes of an untouched merge do not change
        let label = if o.declared && !s.is_ascii() {
            declared_label(body).filter(|l| !matches!(l.to_ascii_lowercase().as_str(), "utf-8" | "utf8" | "us-ascii" | "ascii" | "charset"))
        } else {
            None
        };
        return done(s.to_string(), if bom { "utf-8-bom" } else { "utf-8" }, label.as_deref(), w, &o.limits);
    }
    if data.starts_with(&[0xFF, 0xFE]) || data.starts_with(&[0xFE, 0xFF]) {
        let (e, name) = if data[0] == 0xFF { (encoding_rs::UTF_16LE, "utf-16le") } else { (encoding_rs::UTF_16BE, "utf-16be") };
        if let Some(s) = e.decode_without_bom_handling_and_without_replacement(&data[2..]) {
            let (text, label) = if o.declared {
                let head = e.decode_without_bom_handling(&data[2..data.len().min(8194)]).0.into_owned();
                let l = decl_re().captures(&head).and_then(|c| c.get(2)).map(|m| m.as_str().to_string());
                (rewrite_declared(&s), l)
            } else {
                (s.into_owned(), None)
            };
            return done(text, name, label.as_deref(), Vec::new(), &o.limits);
        }
    }
    if o.declared {
        if let Some(e) = declared_encoding(data).filter(|e| declared_ok(e)) {
            if let Ok(s) = strict(e, data) {
                let label = declared_label(data);
                return done(rewrite_declared(&s), &e.name().to_ascii_lowercase(), label.as_deref(), Vec::new(), &o.limits);
            }
        }
    }
    if looks_binary(data) {
        return Err(enc_err("This file is not valid UTF-8 and looks like a binary file, not text. Re-save the file as UTF-8."));
    }
    let mut warnings = Vec::new();
    let (text, enc) = decode_legacy(data, hint, locale, &mut warnings)?;
    if o.declared {
        // the label the file carries stays with it, so an untouched merge writes it back
        let label = declared_label(data);
        return done(rewrite_declared(&text), &declared_base(enc, label.as_deref(), data), label.as_deref(), warnings, &o.limits);
    }
    done(text, &enc.name().to_ascii_lowercase(), None, warnings, &o.limits)
}

fn where_(hint: Option<&'static Encoding>, locale: &str) -> String {
    if hint.is_some() && !locale.is_empty() {
        format!(", the code page of the project's source language {locale},")
    } else {
        String::new()
    }
}

fn bad_byte(label: &str, data: &[u8], pos: usize, also: &str) -> Error {
    enc_err(format!(
        "This file is not valid UTF-8 and could not be read as {label}{also} (byte 0x{:02x} at position {pos}). Re-save the file as UTF-8.",
        data.get(pos).copied().unwrap_or(0)
    ))
}

/// The last step: no UTF-8, no BOM, no usable declaration. The source language's code page, or Windows-1252.
fn decode_legacy(
    data: &[u8],
    hint: Option<&'static Encoding>,
    locale: &str,
    warnings: &mut Vec<Warning>,
) -> Result<(String, &'static Encoding), Error> {
    let primary = hint.unwrap_or(encoding_rs::WINDOWS_1252);
    let where_s = where_(hint, locale);
    let comma = if where_s.is_empty() { "," } else { where_s.as_str() };
    let (text, primary) = match strict(primary, data) {
        Ok(t) => (t, primary),
        Err(pos) => {
            let wider = (primary == encoding_rs::GBK).then_some(encoding_rs::GB18030);
            match wider.and_then(|w| strict(w, data).ok().map(|t| (t, w))) {
                Some(x) => x,
                None if primary == encoding_rs::WINDOWS_1252 => return Err(bad_byte(&cp_label(primary), data, pos, "")),
                None => {
                    // the source code page cannot read it: Windows-1252 with a warning, or refuse
                    let alt = strict(encoding_rs::WINDOWS_1252, data)
                        .map_err(|_| bad_byte(&cp_label(primary), data, pos, &format!("{where_s} or Windows-1252 either")))?;
                    add(
                        warnings,
                        format!(
                            "This file is not UTF-8 and declares no encoding. It could not be read as {}{where_s} so it was read as Western European (Windows-1252) instead. {RESAVE}",
                            cp_label(primary)
                        ),
                    );
                    return Ok((alt, encoding_rs::WINDOWS_1252));
                }
            }
        }
    };
    if let Some(reason) = text_looks_garbled(&text, primary, locale, data) {
        if primary != encoding_rs::WINDOWS_1252 {
            if let Ok(alt) = strict(encoding_rs::WINDOWS_1252, data) {
                if text_looks_garbled(&alt, encoding_rs::WINDOWS_1252, "", data).is_none() {
                    add(
                        warnings,
                        format!(
                            "This file is not UTF-8 and declares no encoding. Read as {}{comma} its letters looked wrong ({reason}), so it was read as Western European (Windows-1252) instead. {RESAVE}",
                            cp_label(primary)
                        ),
                    );
                    return Ok((alt, encoding_rs::WINDOWS_1252));
                }
            }
        }
        add(
            warnings,
            format!(
                "This file is not UTF-8 and declares no encoding, so it was read as {}{comma} but its letters look wrong ({reason}). {RESAVE}",
                cp_label(primary)
            ),
        );
    }
    Ok((text, primary))
}

fn add(w: &mut Vec<Warning>, message: String) {
    let x = Warning::new("text_encoding", "text encoding", message);
    if !w.contains(&x) {
        w.push(x);
    }
}

/// A few Chinese, Japanese or Korean characters are valid UTF-8 by accident surprisingly often. The file stays
/// UTF-8 (UTF-8 comes first), but in a project whose source language is CJK it warns when all of these hold: at
/// most 16 non-ASCII characters, every one in U+0080..U+07FF, at least one past Latin Extended-A, no CJK at all,
/// and the source code page reads the same bytes cleanly as CJK text.
fn legacy_as_utf8(data: &[u8], text: &str, hint: &'static Encoding, locale: &str) -> Option<Warning> {
    if !crate::garble::is_cjk_code_page(hint) {
        return None;
    }
    let mut odd = false;
    for (n, c) in text.chars().filter(|c| !c.is_ascii()).enumerate() {
        if n + 1 > 16 || (c as u32) > 0x7FF {
            return None;
        }
        odd = odd || (c as u32) >= 0x180;
    }
    if !odd {
        return None;
    }
    let alt = strict(hint, data).ok()?;
    if !alt.chars().any(crate::garble::is_cjk_char) || text_looks_garbled(&alt, hint, locale, &[]).is_some() {
        return None;
    }
    Some(Warning::new(
        "text_encoding",
        "text encoding",
        format!(
            "This file is valid UTF-8, so it was read as UTF-8, but its few non-English letters look wrong for the project's source language {locale} (read as {} the same bytes give Chinese, Japanese or Korean text). {RESAVE}",
            cp_label(hint)
        ),
    ))
}

/// For a reader with its own legacy decode (Java `.properties` are ISO-8859-1 by specification): the warning
/// to attach when `text`, read as `enc`, looks wrong for the project's source language `locale`.
pub fn warn_if_garbled(text: &str, enc: &'static Encoding, locale: &str, label: Option<&str>) -> Option<Warning> {
    if locale.is_empty() {
        return None;
    }
    let reason = text_looks_garbled(text, enc, locale, &[])?;
    Some(Warning::new(
        "text_encoding",
        "text encoding",
        format!(
            "This file is not UTF-8, so it was read as {}, but its letters look wrong ({reason}). {RESAVE}",
            label.map(str::to_string).unwrap_or_else(|| cp_label(enc))
        ),
    ))
}

struct Envelope<'a> {
    base: &'a str,
    nl: &'static str,
    label: Option<&'a str>,
    /// GBK file whose euro sign was the two bytes A2 E3 (GB18030's spelling)
    euro_a2e3: bool,
}

fn parse_tag(t: &str) -> Envelope<'_> {
    let mut parts = t.split(';');
    let base = parts.next().unwrap_or("utf-8");
    let (mut nl, mut label, mut euro_a2e3) = ("", None, false);
    for p in parts {
        match p {
            "euro=a2e3" => euro_a2e3 = true,
            "nl=crlf" => nl = "\r\n",
            "nl=cr" => nl = "\r",
            _ => {
                if let Some(l) = p.strip_prefix("label=") {
                    label = Some(l);
                }
            }
        }
    }
    Envelope { base, nl, label, euro_a2e3 }
}

fn with_newlines(text: &str, nl: &str) -> String {
    let mut out = String::with_capacity(text.len() + text.len() / 16);
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\r' => {
                if it.peek() == Some(&'\n') {
                    it.next();
                }
                out.push_str(nl);
            }
            '\n' => out.push_str(nl),
            c => out.push(c),
        }
    }
    out
}

/// [`encode_text`] with [`MergeOptions`]: with `output_encoding` UTF-8 the text is written as UTF-8 whatever the
/// original encoding was (a UTF-8 BOM is kept; a UTF-16 BOM or a legacy code page is not), the newline style of the
/// original is kept, and an XML, HTML or PO charset declaration that still names another charset is rewritten to
/// UTF-8, so the file says what it is.
pub fn encode_text_with(text: &str, encoding: &str, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
    if opts.output_encoding != OutputEncoding::Utf8 {
        return encode_text(text, encoding);
    }
    let env = parse_tag(encoding);
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let mut out = if env.nl.is_empty() { text.to_string() } else { with_newlines(text, env.nl) };
    if let Some(l) = env.label.filter(|l| !l.eq_ignore_ascii_case("utf-8") && !l.eq_ignore_ascii_case("utf8")) {
        out = decl_re()
            .replace_all(&out, |c: &regex::Captures| if c[2].eq_ignore_ascii_case(l) { format!("{}UTF-8", &c[1]) } else { c[0].to_string() })
            .into_owned();
    }
    let mut v = Vec::with_capacity(out.len() + 3);
    if env.base == "utf-8-bom" {
        v.extend_from_slice(BOM8);
    }
    v.extend_from_slice(out.as_bytes());
    Ok(v)
}

/// Writes the merged `text` in the envelope `encoding` (a skeleton tag from [`decode_text`], or plain `"utf-8"` /
/// `"utf-8-bom"`). Text that does not fit the original legacy encoding is written as UTF-8.
pub fn encode_text(text: &str, encoding: &str) -> Result<Vec<u8>, Error> {
    let env = parse_tag(encoding);
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let normalized;
    let text = if env.nl.is_empty() {
        text
    } else {
        normalized = with_newlines(text, env.nl);
        &normalized
    };
    let utf8 = |bom: bool| {
        let mut v = Vec::with_capacity(text.len() + 3);
        if bom {
            v.extend_from_slice(BOM8);
        }
        v.extend_from_slice(text.as_bytes());
        v
    };
    match env.base {
        "utf-8" => return Ok(utf8(false)),
        "utf-8-bom" => return Ok(utf8(true)),
        _ => {}
    }
    // a declared charset: each declaration parse rewrote to UTF-8 gets its own label back
    let restored = match env.label {
        Some(l) => decl_re()
            .replace_all(text, |c: &regex::Captures| {
                if c[2].eq_ignore_ascii_case("UTF-8") {
                    format!("{}{l}", &c[1])
                } else {
                    c[0].to_string()
                }
            })
            .into_owned(),
        None => text.to_string(),
    };
    let utf16 = match env.base {
        "utf-16le" => Some(false),
        "utf-16be" => Some(true),
        _ => None,
    };
    if let Some(be) = utf16 {
        let mut v: Vec<u8> = if be { vec![0xFE, 0xFF] } else { vec![0xFF, 0xFE] };
        for u in restored.encode_utf16() {
            v.extend_from_slice(&if be { u.to_be_bytes() } else { u.to_le_bytes() });
        }
        return Ok(v);
    }
    if env.base == LATIN1 {
        // true ISO-8859-1: every character must be U+0000..U+00FF, else UTF-8 under the UTF-8 labels
        let agree = decl_re().captures_iter(&restored).all(|c| {
            let name = c.get(2).map_or("", |m| m.as_str());
            is_latin1_label(name) || name.eq_ignore_ascii_case("us-ascii")
        });
        if !agree || restored.chars().any(|c| (c as u32) > 0xFF) {
            return Ok(utf8(false));
        }
        return Ok(restored.chars().map(|c| c as u32 as u8).collect());
    }
    let Some(enc) = Encoding::for_label(env.base.as_bytes()) else {
        return Err(Error::Write(format!("unknown encoding {}", env.base)));
    };
    // the bytes and every declaration in the file must agree; if one does not, write UTF-8 under the UTF-8 labels
    let agree = decl_re().captures_iter(&restored).all(|c| {
        let name = c.get(2).map_or("", |m| m.as_str());
        Encoding::for_label(name.as_bytes()) == Some(enc)
            || env.label.is_some_and(|l| l.eq_ignore_ascii_case(name))
            || name.eq_ignore_ascii_case("us-ascii")
    });
    if !agree {
        return Ok(utf8(false));
    }
    // GB18030 writes everything GBK does, with the same bytes, and the four-byte forms GBK's encoder lacks
    // (B6); only the euro sign differs: GBK and Windows code page 936 readers expect the single byte 0x80
    if enc == encoding_rs::GBK && !env.euro_a2e3 {
        return Ok(encode_gbk(&restored).0);
    }
    let writer = if enc == encoding_rs::GBK { encoding_rs::GB18030 } else { enc };
    let (bytes, _, unmappable) = writer.encode(&restored);
    if unmappable {
        return Ok(utf8(false));
    }
    Ok(bytes.into_owned())
}

/// Encodes `text` for a GBK / GB2312 / code page 936 file: GB18030's bytes (it holds everything GBK does, plus
/// four-byte forms for the rest), except that the euro sign U+20AC is the single byte 0x80, which is what GBK and
/// Windows readers expect (GB18030's A2 E3 is read as a different character by some of them). The flag is true
/// when a character has no mapping at all (never for GB18030; kept for the caller's fallback logic).
pub fn encode_gbk(text: &str) -> (Vec<u8>, bool) {
    let mut out = Vec::with_capacity(text.len() + text.len() / 2);
    let mut bad = false;
    for (i, piece) in text.split('\u{20AC}').enumerate() {
        if i > 0 {
            out.push(0x80);
        }
        let (b, _, unmappable) = encoding_rs::GB18030.encode(piece);
        bad |= unmappable;
        out.extend_from_slice(&b);
    }
    (out, bad)
}

/// True when `tag` names a legacy (neither UTF-8 nor UTF-16) encoding.
pub fn is_legacy(tag: &str) -> bool {
    !matches!(parse_tag(tag).base, "utf-8" | "utf-8-bom" | "utf-16le" | "utf-16be")
}

/// A plain name for the encoding of a tag, for messages ("Greek (Windows-1253)").
pub fn describe(tag: &str) -> String {
    let b = parse_tag(tag).base;
    match Encoding::for_label(b.as_bytes()) {
        Some(e) if is_legacy(tag) => cp_label(e),
        _ => b.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(bytes: &[u8], lang: Option<&str>, declared: bool) -> Result<Decoded, Error> {
        let o = DecodeOptions { declared, source_lang: lang, limits: Limits::default() };
        decode_text(bytes, &o)
    }

    fn legacy(enc: &'static Encoding, s: &str) -> Vec<u8> {
        let (b, _, bad) = enc.encode(s);
        assert!(!bad);
        b.into_owned()
    }

    #[test]
    fn utf8_and_bom_are_kept_and_written_back() {
        let d = dec("h\u{e9}llo\r\nw\u{f6}rld\r\n".as_bytes(), None, false).unwrap();
        assert_eq!(d.encoding, "utf-8;nl=crlf");
        assert!(d.warnings.is_empty());
        assert_eq!(encode_text(&d.text, &d.encoding).unwrap(), "h\u{e9}llo\r\nw\u{f6}rld\r\n".as_bytes());
        let mut b = BOM8.to_vec();
        b.extend_from_slice("x\n".as_bytes());
        let d = dec(&b, None, false).unwrap();
        assert_eq!(d.encoding, "utf-8-bom");
        assert_eq!(encode_text(&d.text, &d.encoding).unwrap(), b);
    }

    #[test]
    fn newline_style_is_restored_for_new_text() {
        let d = dec(b"a\r\nb\r\n", None, false).unwrap();
        assert_eq!(encode_text("x\ny\r\nz\rw", &d.encoding).unwrap(), b"x\r\ny\r\nz\r\nw");
        let d = dec(b"a\rb\r", None, false).unwrap();
        assert_eq!(encode_text("x\ny", &d.encoding).unwrap(), b"x\ry");
        // mixed endings are left alone
        let d = dec(b"a\r\nb\nc", None, false).unwrap();
        assert_eq!(d.encoding, "utf-8");
        assert_eq!(encode_text("x\ny\r\n", &d.encoding).unwrap(), b"x\ny\r\n");
    }

    #[test]
    fn utf16_with_bom_round_trips() {
        let mut b = vec![0xFF, 0xFE];
        for u in "Caf\u{e9} \u{4e2d}\r\nok\r\n".encode_utf16() {
            b.extend_from_slice(&u.to_le_bytes());
        }
        let d = dec(&b, None, false).unwrap();
        assert_eq!(d.encoding, "utf-16le;nl=crlf");
        assert_eq!(d.text, "Caf\u{e9} \u{4e2d}\r\nok\r\n");
        assert_eq!(encode_text(&d.text, &d.encoding).unwrap(), b);
        let mut be = vec![0xFE, 0xFF];
        for u in "ab".encode_utf16() {
            be.extend_from_slice(&u.to_be_bytes());
        }
        let d = dec(&be, None, false).unwrap();
        assert_eq!((d.text.as_str(), d.encoding.as_str()), ("ab", "utf-16be"));
        assert_eq!(encode_text("ab", "utf-16be").unwrap(), be);
    }

    #[test]
    fn western_default_is_windows_1252() {
        let b = legacy(encoding_rs::WINDOWS_1252, "Caf\u{e9} \u{201c}ok\u{201d}");
        let d = dec(&b, None, false).unwrap();
        assert_eq!(d.text, "Caf\u{e9} \u{201c}ok\u{201d}");
        assert_eq!(d.encoding, "windows-1252");
        assert!(d.warnings.is_empty());
        assert_eq!(encode_text(&d.text, &d.encoding).unwrap(), b);
        // a translation that does not fit is written as UTF-8
        assert_eq!(encode_text("\u{4e2d}", &d.encoding).unwrap(), "\u{4e2d}".as_bytes());
        // an undefined Windows-1252 byte is refused, with its position
        let e = dec(&[b'a', 0x81, b'b'], None, false).unwrap_err();
        assert!(matches!(&e, Error::Encoding(m) if m.contains("0x81") && m.contains("position 1")), "{e}");
    }

    #[test]
    fn source_language_picks_the_code_page() {
        let cases: [(&str, &'static Encoding, &str, &str); 7] = [
            ("el", encoding_rs::WINDOWS_1253, "\u{39a}\u{3b1}\u{3bb}\u{3b7}\u{3bc}\u{3ad}\u{3c1}\u{3b1} \u{3ba}\u{3cc}\u{3c3}\u{3bc}\u{3b5}", "windows-1253"),
            ("ru", encoding_rs::WINDOWS_1251, "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}, \u{43c}\u{438}\u{440}. \u{41a}\u{430}\u{43a} \u{434}\u{435}\u{43b}\u{430}?", "windows-1251"),
            ("ar", encoding_rs::WINDOWS_1256, "\u{645}\u{631}\u{62d}\u{628}\u{627} \u{628}\u{627}\u{644}\u{639}\u{627}\u{644}\u{645} \u{643}\u{64a}\u{641} \u{62d}\u{627}\u{644}\u{643}", "windows-1256"),
            ("ja", encoding_rs::SHIFT_JIS, "\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}\u{3001}\u{4e16}\u{754c}\u{3002}\u{30c6}\u{30b9}\u{30c8}\u{3067}\u{3059}\u{3002}", "shift_jis"),
            ("zh-CN", encoding_rs::GBK, "\u{4f60}\u{597d}\u{ff0c}\u{4e16}\u{754c}\u{3002}\u{8fd9}\u{662f}\u{4e00}\u{4e2a}\u{6d4b}\u{8bd5}\u{3002}", "gbk"),
            ("zh-TW", encoding_rs::BIG5, "\u{4f60}\u{597d}\u{ff0c}\u{4e16}\u{754c}\u{3002}\u{9019}\u{662f}\u{4e00}\u{500b}\u{6e2c}\u{8a66}\u{3002}", "big5"),
            ("ko", encoding_rs::EUC_KR, "\u{c548}\u{b155}\u{d558}\u{c138}\u{c694}, \u{c138}\u{acc4}. \u{d14c}\u{c2a4}\u{d2b8}\u{c785}\u{b2c8}\u{b2e4}.", "euc-kr"),
        ];
        for (lang, enc, text, name) in cases {
            let b = legacy(enc, text);
            let d = dec(&b, Some(lang), false).unwrap();
            assert_eq!((d.text.as_str(), d.encoding.as_str()), (text, name), "{lang}");
            assert!(d.warnings.is_empty(), "{lang}: {:?}", d.warnings);
            assert_eq!(encode_text(&d.text, &d.encoding).unwrap(), b, "{lang} round trip");
        }
    }

    #[test]
    fn four_byte_gb18030_is_read_in_a_chinese_project() {
        // a four-byte GB18030 sequence (U+20000): the GBK decoder of encoding_rs is the GB18030 one
        let b = legacy(encoding_rs::GB18030, "\u{4f60}\u{597d}\u{20000}\u{4e16}\u{754c}");
        let d = dec(&b, Some("zh-CN"), false).unwrap();
        assert_eq!(d.text, "\u{4f60}\u{597d}\u{20000}\u{4e16}\u{754c}");
        assert_eq!(encode_text(&d.text, &d.encoding).unwrap(), b);
    }

    #[test]
    fn declared_charset_is_read_rewritten_and_restored() {
        let src = "<html><head><meta charset=\"windows-1253\"></head><body>\u{39a}\u{3b1}\u{3bb}\u{3b7}\u{3bc}\u{3ad}\u{3c1}\u{3b1}</body></html>";
        let b = legacy(encoding_rs::WINDOWS_1253, src);
        let d = dec(&b, Some("en"), true).unwrap();
        assert!(d.text.contains("charset=\"UTF-8\"") && d.text.contains("\u{39a}\u{3b1}"));
        assert_eq!(d.encoding, "windows-1253;label=windows-1253");
        assert_eq!(encode_text(&d.text, &d.encoding).unwrap(), b);
        // text that does not fit is UTF-8 under the UTF-8 label
        let out = encode_text(&d.text.replace('\u{3b1}', "\u{4e2d}"), &d.encoding).unwrap();
        assert!(String::from_utf8(out).unwrap().contains("charset=\"UTF-8\""));
    }

    #[test]
    fn declared_iso_8859_1_is_written_as_iso_8859_1_or_utf8() {
        let po = b"msgid \"\"\nmsgstr \"\"\n\"Content-Type: text/plain; charset=ISO-8859-1\\n\"\n\nmsgid \"a\"\nmsgstr \"caf\xe9\"\n".to_vec();
        let d = dec(&po, Some("en"), true).unwrap();
        assert_eq!(d.encoding, "iso-8859-1;label=ISO-8859-1");
        assert_eq!(encode_text(&d.text, &d.encoding).unwrap(), po);
        // French oe and the euro sign are not ISO-8859-1: UTF-8 under the UTF-8 label, never Windows-1252 bytes
        let out = encode_text(&d.text.replace("caf\u{e9}", "c\u{153}ur \u{20ac}"), &d.encoding).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("charset=UTF-8") && s.contains("c\u{153}ur \u{20ac}"));
        // a pure-ASCII file under the same label
        let ascii = b"msgid \"\"\nmsgstr \"\"\n\"Content-Type: text/plain; charset=ISO-8859-1\\n\"\n".to_vec();
        let d = dec(&ascii, Some("en"), true).unwrap();
        assert!(d.encoding.starts_with("iso-8859-1"), "{}", d.encoding);
        assert_eq!(encode_text(&d.text, &d.encoding).unwrap(), ascii);
        // a mislabelled file with Windows-1252 bytes keeps its bytes on an untouched merge
        let mut bad = po.clone();
        bad.extend_from_slice(b"msgid \"b\"\nmsgstr \"\x80\"\n");
        let d = dec(&bad, Some("en"), true).unwrap();
        assert!(d.encoding.starts_with("windows-1252"), "{}", d.encoding);
        assert_eq!(encode_text(&d.text, &d.encoding).unwrap(), bad);
    }

    #[test]
    fn declared_unknown_label_comes_back_untouched() {
        let b = b"<?xml version=\"1.0\" encoding=\"x-nonsense\"?><a>caf\xe9</a>".to_vec();
        let d = dec(&b, None, true).unwrap();
        assert_eq!(encode_text(&d.text, &d.encoding).unwrap(), b);
    }

    #[test]
    fn wrong_code_page_gives_a_warning_not_an_error() {
        // Greek saved as Windows-1253 in a project with no source language: read as Windows-1252 it is accented
        // Latin soup, so the upload warns and never fails
        let b = legacy(encoding_rs::WINDOWS_1253, "\u{39a}\u{3b1}\u{3bb}\u{3b7}\u{3bc}\u{3ad}\u{3c1}\u{3b1} \u{3ba}\u{3cc}\u{3c3}\u{3bc}\u{3b5}, \u{3ba}\u{3b1}\u{3bb}\u{3b7}\u{3c3}\u{3c0}\u{3ad}\u{3c1}\u{3b1} \u{3c3}\u{3b1}\u{3c2}");
        let d = dec(&b, Some("en"), false).unwrap();
        assert_eq!(d.warnings.len(), 1);
        assert_eq!(d.warnings[0].kind, "text_encoding");
        assert!(d.warnings[0].message.contains("re-save the file as UTF-8"));
        // the same bytes in a Greek project are read correctly, without a warning
        assert!(dec(&b, Some("el"), false).unwrap().warnings.is_empty());
    }

    #[test]
    fn western_text_is_not_flagged_in_a_cjk_project() {
        assert!(dec("caf\u{e9}".as_bytes(), Some("ja"), false).unwrap().warnings.is_empty());
    }

    #[test]
    fn binary_and_unreadable_files_are_refused() {
        let e = dec(&[0x89, b'P', b'N', b'G', 0, 0, 0, 1], None, false).unwrap_err();
        assert!(matches!(&e, Error::Encoding(m) if m.contains("binary")), "{e}");
        let e = dec(&[b'a', 0x90, b'b'], Some("en"), false).unwrap_err();
        assert!(matches!(e, Error::Encoding(_)));
    }

    #[test]
    fn line_budget_applies_to_decoded_text() {
        let mut o = DecodeOptions { declared: false, source_lang: None, limits: Limits::default() };
        o.limits.max_lines = 3;
        assert!(decode_text(b"a\nb\n", &o).is_ok());
        let e = decode_text(b"\n\n\n\n\n", &o).unwrap_err();
        assert!(matches!(e, Error::Limit(l) if l.kind == crate::LimitKind::Lines));
    }
}

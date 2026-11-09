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
//! files. Grammar: `base[;nl=crlf|cr][;feff=<n>][;label=<declared charset>]`, with `base` one of `utf-8`,
//! `utf-8-bom`, `utf-16le`, `utf-16be` (both with a BOM) or an `encoding_rs` name such as `windows-1253` or
//! `shift_jis`. `feff=n` says the decoded text starts with n U+FEFF characters that are content (a file with two
//! byte order marks): the writer then keeps them ([`strip_own_mark`]).

use crate::codepage::{label as cp_label, source_encoding};
use crate::garble::text_looks_garbled;
use crate::guard::{check_lines, Limits};
use crate::{Error, ExtractOptions, MergeOptions, OutputEncoding, Warning};
use encoding_rs::{DecoderResult, Encoding};
use regex::Regex;
use std::sync::OnceLock;

const BOM8: &[u8] = &[0xEF, 0xBB, 0xBF];
const RESAVE: &str = "Check the source text; if it is garbled, re-save the file as UTF-8 and upload it again.";

/// How one text file is read. Build it with [`DecodeOptions::new`] (from the extraction options) or
/// [`DecodeOptions::with_limits`], then [`DecodeOptions::declared`] for a format that declares its charset.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct DecodeOptions<'a> {
    /// The format declares its charset in the file (HTML `meta`, XML declaration, PO `Content-Type`): honour a
    /// declared legacy charset, and write the declaration back when the file is merged.
    pub declared: bool,
    /// The project's source language, which picks the legacy code page of an undeclared non-UTF-8 file.
    pub source_lang: Option<&'a str>,
    /// The limits the decoded text is checked against.
    pub limits: Limits,
    /// A UTF-8 byte order mark followed by bytes that are not UTF-8 is read as UTF-8 with replacement characters
    /// (U+FFFD), as Localization OS does for plain text and subtitles. Off: such a file is refused.
    pub lossy_after_utf8_bom: bool,
    /// A UTF-8 byte order mark followed by bytes that are not UTF-8 is read as a mark, and the bytes after it as an
    /// undeclared legacy file (Localization OS BOM read version 2, #799.4 / #800.4: its subtitle readers). The
    /// envelope tag records the mark (`;bom8`), so a merge writes it back: before the code page's bytes, or before
    /// UTF-8 when the text is written as UTF-8. Off: the mark's three bytes are read as code-page text.
    pub mark_before_legacy: bool,
}

impl<'a> DecodeOptions<'a> {
    /// The options of one extraction: its source language and its limits.
    pub fn new(opts: &'a ExtractOptions) -> DecodeOptions<'a> {
        DecodeOptions { declared: false, source_lang: opts.source_lang.as_deref(), limits: opts.limits(), lossy_after_utf8_bom: false, mark_before_legacy: false }
    }
    /// No source language, not declared, these limits.
    pub fn with_limits(limits: Limits) -> DecodeOptions<'static> {
        DecodeOptions { declared: false, source_lang: None, limits, lossy_after_utf8_bom: false, mark_before_legacy: false }
    }
    /// The source language that picks the code page of an undeclared legacy file.
    pub fn with_source_lang(mut self, lang: Option<&'a str>) -> Self {
        self.source_lang = lang;
        self
    }
    /// See [`DecodeOptions::lossy_after_utf8_bom`].
    pub fn lossy_after_utf8_bom(mut self) -> Self {
        self.lossy_after_utf8_bom = true;
        self
    }
    /// See [`DecodeOptions::mark_before_legacy`].
    pub fn mark_before_legacy(mut self) -> Self {
        self.mark_before_legacy = true;
        self
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
    /// The decoded text.
    pub text: String,
    /// Goes into `Skeleton::Text::encoding`; hand it back to [`encode_text`] on merge.
    pub encoding: String,
    /// What the decoder noticed (text that looks garbled in the code page it was read with).
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
        || declared_if_strict(e)
}

/// The rarer charsets added on 11-07 (LocOS issue 810.3). A file that declares one is read in it when every byte
/// decodes; one that does not (often a page labelled ISO-8859-8 that holds Windows-1255 vowel points) is read the
/// way it was before, by the default path, not refused. ISO-8859-8 visual and -8-I logical decode to the same
/// characters in the order of the bytes: nothing is reordered.
fn declared_if_strict(e: &'static Encoding) -> bool {
    use encoding_rs::*;
    [
        ISO_8859_3, ISO_8859_4, ISO_8859_6, ISO_8859_8, ISO_8859_8_I, ISO_8859_10, ISO_8859_13, ISO_8859_14, ISO_8859_16, WINDOWS_874,
        IBM866, MACINTOSH, X_MAC_CYRILLIC,
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

/// Every charset declaration in `text` rewritten to say UTF-8, as LocOS's `read_text_upload(declared=True)` does
/// to a file it converted.
#[doc(hidden)]
pub fn rewrite_declarations(text: &str) -> String {
    rewrite_declared(text)
}

/// The skeleton tag without its newline part, so a merge leaves the line breaks of the text as they are (the LocOS
/// profile's XML resource writers: LocOS does not normalise the breaks a translation brings in).
#[doc(hidden)]
pub fn keep_newlines(tag: &str) -> String {
    tag.split(';').filter(|p| !p.starts_with("nl=")).collect::<Vec<_>>().join(";")
}

/// The skeleton of a LocOS-profile document whose writer works on the decoded file (po, srt and vtt): the file's
/// skeleton tag, a line break, then the decoded text without its BOM. Read it back with [`locos_skeleton_parts`].
#[doc(hidden)]
pub fn locos_skeleton(tag: &str, text: &str) -> Vec<u8> {
    let text = strip_own_mark(text, tag);
    let mut v = Vec::with_capacity(tag.len() + 1 + text.len());
    v.extend_from_slice(tag.as_bytes());
    v.push(b'\n');
    v.extend_from_slice(text.as_bytes());
    v
}

/// (skeleton tag, decoded text) of a [`locos_skeleton`]; `None` when `data` is not one.
#[doc(hidden)]
pub fn locos_skeleton_parts(data: &[u8]) -> Option<(&str, &str)> {
    let s = std::str::from_utf8(data).ok()?;
    let (tag, text) = s.split_once('\n')?;
    // a tag is a charset name with `;nl=` / `;label=` (markdown: `;fm=`) parts: nothing a declaration could carry
    // as markup
    let ok = !tag.is_empty() && tag.len() <= 4096 && tag.bytes().all(|b| b.is_ascii_alphanumeric() || b"_.:;=-".contains(&b));
    ok.then_some((tag, text))
}

/// The tag LocOS's UTF-8 export writes a file under: UTF-8, its UTF-8 BOM and its line breaks kept (a UTF-16 BOM,
/// a code page and its label are not).
#[doc(hidden)]
pub fn locos_utf8_tag(tag: &str) -> String {
    let mut t = String::from(if base(tag) == "utf-8-bom" { "utf-8-bom" } else { "utf-8" });
    for p in tag.split(';').skip(1).filter(|p| p.starts_with("nl=") || p.starts_with("feff=")) {
        t.push(';');
        t.push_str(p);
    }
    t
}

/// The charset a file declares in its first 4 KB, as a decoder.
pub fn declared_encoding(data: &[u8]) -> Option<&'static Encoding> {
    let head = ascii_head(data, 4096);
    let m = decl_re().captures(&head)?;
    label_encoding(m.get(2)?.as_str())
}

/// The encoding a charset label names, as Localization OS reads a declaration (`codecs.lookup`, then its
/// `_LABEL_ALIASES`): the WHATWG label first (what `encoding_rs` knows), then Python's codec names and aliases of
/// the legacy charsets a file may declare (`CP932`, `cp936`, `sjis`, `ujis`, `uhc`, `ms949`, `iso8859_2` ...),
/// normalised the way Python does (case, and runs of punctuation as one `_`). Python's ISO-8859-1, Windows-1252
/// and UTF-16 aliases are not in the table: LocOS reads those by its default path, not as a declaration.
pub fn label_encoding(label: &str) -> Option<&'static Encoding> {
    if let Some(e) = Encoding::for_label(label.trim().as_bytes()) {
        return Some(e);
    }
    let norm = python_normalize(&label.to_ascii_lowercase());
    let find = |k: &str| PY_ALIASES.binary_search_by(|(a, _)| (*a).cmp(k)).ok().map(|i| PY_ALIASES[i].1);
    let whatwg = find(&norm).or_else(|| find(&norm.replace('.', "_")))?;
    Encoding::for_label(whatwg.as_bytes())
}

/// Python's `encodings.normalize_encoding`: letters, digits and dots kept, every other run of characters one `_`
/// (none at the ends).
fn python_normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut punct = false;
    for c in s.chars() {
        if c.is_alphanumeric() || c == '.' {
            if punct && !out.is_empty() {
                out.push('_');
            }
            if c.is_ascii() {
                out.push(c);
            }
            punct = false;
        } else {
            punct = true;
        }
    }
    out
}

/// Python 3 codec aliases (normalised) of the charsets LocOS reads from a declaration, to the WHATWG label of the
/// same encoding. Sorted (binary search). Generated from `encodings.aliases` and the codec module names.
const PY_ALIASES: &[(&str, &str)] = &[
    ("1250", "windows-1250"),
    ("1251", "windows-1251"),
    ("1253", "windows-1253"),
    ("1254", "windows-1254"),
    ("1255", "windows-1255"),
    ("1256", "windows-1256"),
    ("1257", "windows-1257"),
    ("1258", "windows-1258"),
    ("866", "ibm866"),
    ("932", "shift_jis"),
    ("936", "gbk"),
    ("949", "euc-kr"),
    ("arabic", "iso-8859-6"),
    ("asmo_708", "iso-8859-6"),
    ("big5", "big5"),
    ("big5_tw", "big5"),
    ("chinese", "gbk"),
    ("cp1250", "windows-1250"),
    ("cp1251", "windows-1251"),
    ("cp1253", "windows-1253"),
    ("cp1254", "windows-1254"),
    ("cp1255", "windows-1255"),
    ("cp1256", "windows-1256"),
    ("cp1257", "windows-1257"),
    ("cp1258", "windows-1258"),
    ("cp866", "ibm866"),
    ("cp874", "windows-874"),
    ("cp932", "shift_jis"),
    ("cp936", "gbk"),
    ("cp949", "euc-kr"),
    ("csbig5", "big5"),
    ("csibm866", "ibm866"),
    ("csiso58gb231280", "gbk"),
    ("csisolatin2", "iso-8859-2"),
    ("csisolatin3", "iso-8859-3"),
    ("csisolatin4", "iso-8859-4"),
    ("csisolatin5", "iso-8859-9"),
    ("csisolatin6", "iso-8859-10"),
    ("csisolatinarabic", "iso-8859-6"),
    ("csisolatincyrillic", "iso-8859-5"),
    ("csisolatingreek", "iso-8859-7"),
    ("csisolatinhebrew", "iso-8859-8"),
    ("cskoi8r", "koi8-r"),
    ("csshiftjis", "shift_jis"),
    ("cyrillic", "iso-8859-5"),
    ("ecma_114", "iso-8859-6"),
    ("ecma_118", "iso-8859-7"),
    ("elot_928", "iso-8859-7"),
    ("euc_cn", "gbk"),
    ("euc_jp", "euc-jp"),
    ("euc_kr", "euc-kr"),
    ("euccn", "gbk"),
    ("eucgb2312_cn", "gbk"),
    ("eucjp", "euc-jp"),
    ("euckr", "euc-kr"),
    ("gb18030", "gb18030"),
    ("gb18030_2000", "gb18030"),
    ("gb2312", "gbk"),
    ("gb2312_1980", "gbk"),
    ("gb2312_80", "gbk"),
    ("gbk", "gbk"),
    ("greek", "iso-8859-7"),
    ("greek8", "iso-8859-7"),
    ("hebrew", "iso-8859-8"),
    ("ibm866", "ibm866"),
    ("iso8859_10", "iso-8859-10"),
    ("iso8859_13", "iso-8859-13"),
    ("iso8859_14", "iso-8859-14"),
    ("iso8859_15", "iso-8859-15"),
    ("iso8859_16", "iso-8859-16"),
    ("iso8859_2", "iso-8859-2"),
    ("iso8859_3", "iso-8859-3"),
    ("iso8859_4", "iso-8859-4"),
    ("iso8859_5", "iso-8859-5"),
    ("iso8859_6", "iso-8859-6"),
    ("iso8859_7", "iso-8859-7"),
    ("iso8859_8", "iso-8859-8"),
    ("iso8859_9", "iso-8859-9"),
    ("iso_8859_10", "iso-8859-10"),
    ("iso_8859_10_1992", "iso-8859-10"),
    ("iso_8859_13", "iso-8859-13"),
    ("iso_8859_14", "iso-8859-14"),
    ("iso_8859_14_1998", "iso-8859-14"),
    ("iso_8859_15", "iso-8859-15"),
    ("iso_8859_16", "iso-8859-16"),
    ("iso_8859_16_2001", "iso-8859-16"),
    ("iso_8859_2", "iso-8859-2"),
    ("iso_8859_2_1987", "iso-8859-2"),
    ("iso_8859_3", "iso-8859-3"),
    ("iso_8859_3_1988", "iso-8859-3"),
    ("iso_8859_4", "iso-8859-4"),
    ("iso_8859_4_1988", "iso-8859-4"),
    ("iso_8859_5", "iso-8859-5"),
    ("iso_8859_5_1988", "iso-8859-5"),
    ("iso_8859_6", "iso-8859-6"),
    ("iso_8859_6_1987", "iso-8859-6"),
    ("iso_8859_7", "iso-8859-7"),
    ("iso_8859_7_1987", "iso-8859-7"),
    ("iso_8859_8", "iso-8859-8"),
    ("iso_8859_8_1988", "iso-8859-8"),
    ("iso_8859_9", "iso-8859-9"),
    ("iso_8859_9_1989", "iso-8859-9"),
    ("iso_celtic", "iso-8859-14"),
    ("iso_ir_101", "iso-8859-2"),
    ("iso_ir_109", "iso-8859-3"),
    ("iso_ir_110", "iso-8859-4"),
    ("iso_ir_126", "iso-8859-7"),
    ("iso_ir_127", "iso-8859-6"),
    ("iso_ir_138", "iso-8859-8"),
    ("iso_ir_144", "iso-8859-5"),
    ("iso_ir_148", "iso-8859-9"),
    ("iso_ir_157", "iso-8859-10"),
    ("iso_ir_199", "iso-8859-14"),
    ("iso_ir_226", "iso-8859-16"),
    ("iso_ir_58", "gbk"),
    ("koi8_r", "koi8-r"),
    ("koi8_u", "koi8-u"),
    ("korean", "euc-kr"),
    ("ks_c_5601", "euc-kr"),
    ("ks_c_5601_1987", "euc-kr"),
    ("ks_x_1001", "euc-kr"),
    ("ksc5601", "euc-kr"),
    ("ksx1001", "euc-kr"),
    ("l10", "iso-8859-16"),
    ("l2", "iso-8859-2"),
    ("l3", "iso-8859-3"),
    ("l4", "iso-8859-4"),
    ("l5", "iso-8859-9"),
    ("l6", "iso-8859-10"),
    ("l7", "iso-8859-13"),
    ("l8", "iso-8859-14"),
    ("l9", "iso-8859-15"),
    ("latin10", "iso-8859-16"),
    ("latin2", "iso-8859-2"),
    ("latin3", "iso-8859-3"),
    ("latin4", "iso-8859-4"),
    ("latin5", "iso-8859-9"),
    ("latin6", "iso-8859-10"),
    ("latin7", "iso-8859-13"),
    ("latin8", "iso-8859-14"),
    ("latin9", "iso-8859-15"),
    ("mac_cyrillic", "x-mac-cyrillic"),
    ("mac_roman", "macintosh"),
    ("maccyrillic", "x-mac-cyrillic"),
    ("macintosh", "macintosh"),
    ("macroman", "macintosh"),
    ("ms932", "shift_jis"),
    ("ms936", "gbk"),
    ("ms949", "euc-kr"),
    ("ms_kanji", "shift_jis"),
    ("mskanji", "shift_jis"),
    ("s_jis", "shift_jis"),
    ("shift_jis", "shift_jis"),
    ("shiftjis", "shift_jis"),
    ("sjis", "shift_jis"),
    ("u_jis", "euc-jp"),
    ("uhc", "euc-kr"),
    ("ujis", "euc-jp"),
    ("windows_1250", "windows-1250"),
    ("windows_1251", "windows-1251"),
    ("windows_1253", "windows-1253"),
    ("windows_1254", "windows-1254"),
    ("windows_1255", "windows-1255"),
    ("windows_1256", "windows-1256"),
    ("windows_1257", "windows-1257"),
    ("windows_1258", "windows-1258"),
    ("windows_31j", "shift_jis"),
    ("x_mac_japanese", "shift_jis"),
    ("x_mac_korean", "euc-kr"),
    ("x_mac_simp_chinese", "gbk"),
    ("x_mac_trad_chinese", "big5"),
];

/// The charset name a file declares in its first 4 KB, as written.
#[doc(hidden)]
pub fn declared_label(data: &[u8]) -> Option<String> {
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
    let marks = leading_marks(text);
    if marks > 0 {
        t.push_str(&format!(";feff={marks}"));
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

/// Declarations live at the top of a file: only those in the first 4096 characters are the file's own. A page that
/// embeds another page (a forum export with a second `<meta charset>` deep in its body) has text there that looks
/// like a declaration but is content, and must come back as it was.
fn split_head(text: &str) -> (&str, &str) {
    let cut = text.char_indices().nth(4096).map_or(text.len(), |(i, _)| i);
    text.split_at(cut)
}

fn rewrite_declared(text: &str) -> String {
    let (head, tail) = split_head(text);
    let mut out = decl_re().replace_all(head, |c: &regex::Captures| format!("{}UTF-8", &c[1])).into_owned();
    out.push_str(tail);
    out
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
    // In a format that declares its charset (XML, HTML, PO, ...) a byte order mark is a declaration too: bytes
    // that are not valid in it are refused, never read with a fallback code page (lxml in LocOS refuses them). An
    // undeclared format falls back to a code page, as LocOS's `read_text_upload` does; LocOS's plain text and
    // subtitle readers instead read UTF-8 with replacement characters (`lossy_after_utf8_bom`).
    if bom {
        let bad = std::str::from_utf8(body).err().map_or(0, |e| e.valid_up_to());
        if o.lossy_after_utf8_bom {
            return done(String::from_utf8_lossy(body).into_owned(), "utf-8-bom", None, Vec::new(), &o.limits);
        }
        if o.declared {
            return Err(enc_err(format!(
            "This file starts with a UTF-8 byte order mark but is not valid UTF-8 (byte 0x{:02x} at position {}). Re-save the file as UTF-8.",
            body.get(bad).copied().unwrap_or(0),
            bad + 3
            )));
        }
        // BOM read version 2 (LocOS subtitle readers): the mark is a mark, the rest an undeclared legacy file
        if o.mark_before_legacy && !o.declared {
            let mut d = decode_text_inner(body, &DecodeOptions { mark_before_legacy: false, ..*o })?;
            if !matches!(base(&d.encoding), "utf-8" | "utf-8-bom" | "utf-16le" | "utf-16be") {
                d.encoding.push_str(";bom8");
                return Ok(d);
            }
        }
        // an undeclared format reads it as LocOS's `read_text_upload` does: the code page fallback below
    }
    if data.starts_with(&[0xFF, 0xFE]) || data.starts_with(&[0xFE, 0xFF]) {
        let (e, name) = if data[0] == 0xFF { (encoding_rs::UTF_16LE, "utf-16le") } else { (encoding_rs::UTF_16BE, "utf-16be") };
        let decoded = e.decode_without_bom_handling_and_without_replacement(&data[2..]);
        if decoded.is_none() && o.declared {
            return Err(enc_err(format!("This file starts with a {} byte order mark but is not valid {}. Re-save the file as UTF-8.", name.to_ascii_uppercase(), name.to_ascii_uppercase())));
        }
        if let Some(s) = decoded {
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
        if let Some(e) = declared_encoding(data) {
            let label = declared_label(data);
            let name = label.clone().unwrap_or_else(|| e.name().to_string());
            // the file says UTF-8 (or a code page it does not fit): refused, as an XML parser and LocOS do
            if e == encoding_rs::UTF_8 {
                let bad = std::str::from_utf8(data).err().map_or(0, |e| e.valid_up_to());
                return Err(bad_declared(&name, data, bad));
            }
            // a declared Windows-1252 / ISO-8859-1 file is read in it whatever the source language (B-SIDE W6 ts-1:
            // a Japanese project read it as Shift_JIS); bytes it cannot read fall through to the default path
            let latin = e == encoding_rs::WINDOWS_1252;
            if declared_ok(e) || latin {
                match strict(e, data) {
                    Ok(s) => {
                        // the same tag the default path gives a Latin-1 file (`iso-8859-1` when no C1 byte is used)
                        let b = if latin { declared_base(e, label.as_deref(), data) } else { e.name().to_ascii_lowercase() };
                        return done(rewrite_declared(&s), &b, label.as_deref(), Vec::new(), &o.limits);
                    }
                    Err(pos) => {
                        // Windows-1255 holds all of ISO-8859-8 at the same bytes, plus the vowel points pages use;
                        // the declared label stays, so a merge writes the same bytes back
                        let hebrew = e == encoding_rs::ISO_8859_8 || e == encoding_rs::ISO_8859_8_I;
                        if let Some(s) = hebrew.then(|| strict(encoding_rs::WINDOWS_1255, data).ok()).flatten() {
                            return done(rewrite_declared(&s), "windows-1255", label.as_deref(), Vec::new(), &o.limits);
                        }
                        if !declared_if_strict(e) && !latin {
                            return Err(bad_declared(&name, data, pos));
                        }
                    }
                }
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
        if label.is_none() {
            if let Some(m) = late_declaration(&text, enc) {
                add(&mut warnings, m);
            }
        }
        return done(rewrite_declared(&text), &declared_base(enc, label.as_deref(), data), label.as_deref(), warnings, &o.limits);
    }
    done(text, &enc.name().to_ascii_lowercase(), None, warnings, &o.limits)
}

/// What [`decode_source_legacy`] returns: the text, the code page it was read in and the warnings.
pub type SourceLegacy = Result<(String, &'static Encoding, Vec<Warning>), Error>;

/// For a reader with its own BOM and UTF-8 handling (ini, yaml, js modules): the last step of [`decode_text`] for
/// bytes that are not UTF-8 and carry no byte order mark. Returns `None` when `source_lang` has no code page of its
/// own (the caller keeps its Windows-1252 path); else the text, the code page it was read in and the warnings
/// (the letters look wrong in it, or it did not read and Windows-1252 was used). The caller has already refused
/// binary files.
pub fn decode_source_legacy(data: &[u8], source_lang: Option<&str>) -> Option<SourceLegacy> {
    let locale = source_lang.unwrap_or("");
    let hint = source_encoding(locale)?;
    let mut warnings = Vec::new();
    Some(decode_legacy(data, Some(hint), locale, &mut warnings).map(|(t, e)| (t, e, warnings)))
}

fn where_(hint: Option<&'static Encoding>, locale: &str) -> String {
    if hint.is_some() && !locale.is_empty() {
        format!(", the code page of the project's source language {locale},")
    } else {
        String::new()
    }
}

fn bad_declared(name: &str, data: &[u8], pos: usize) -> Error {
    enc_err(format!(
        "This file declares the encoding {name} but its bytes are not valid in it (byte 0x{:02x} at position {pos}). Re-save the file as UTF-8.",
        data.get(pos).copied().unwrap_or(0)
    ))
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

/// A file with no declaration in its first 4 KB that names another legacy charset further down (a page whose
/// `<meta charset>` comes after a long script): that later one is not the file's declaration (it may be content),
/// so the file was read in `enc`; the warning says so, so a page that really is in the later charset is not
/// silently read as the wrong one.
fn late_declaration(text: &str, enc: &'static Encoding) -> Option<String> {
    decl_re().captures_iter(text).find_map(|c| {
        let name = c.get(2)?.as_str();
        let e = label_encoding(name)?;
        (e != enc && e != encoding_rs::UTF_8 && e != encoding_rs::UTF_16LE && e != encoding_rs::UTF_16BE).then(|| {
            format!(
                "This file is not UTF-8 and declares no encoding in its first 4096 characters, so it was read as {}. A declaration of {name} further down was not used. {RESAVE}",
                cp_label(enc)
            )
        })
    })
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
    /// leading U+FEFF characters of the decoded text that are content (`feff=n`)
    feff: usize,
    /// a UTF-8 byte order mark stood before the legacy bytes (`bom8`, [`DecodeOptions::mark_before_legacy`])
    bom8: bool,
}

fn leading_marks(text: &str) -> usize {
    text.chars().take_while(|c| *c == '\u{FEFF}').count()
}

/// `text` without the one leading U+FEFF a writer may have put in front as its byte order mark, which the encoder
/// writes itself. The decoder records how many leading U+FEFF of the file's text are content (`feff=n` in the
/// skeleton tag, a file that starts with two marks); only a U+FEFF beyond those is taken off, so an untouched merge
/// of such a file is exact, and a writer that adds one more U+FEFF in front (php, subtext, csv) still gets one.
pub fn strip_own_mark<'t>(text: &'t str, encoding: &str) -> &'t str {
    if leading_marks(text) > parse_tag(encoding).feff {
        &text['\u{FEFF}'.len_utf8()..]
    } else {
        text
    }
}

fn parse_tag(t: &str) -> Envelope<'_> {
    let mut parts = t.split(';');
    let base = parts.next().unwrap_or("utf-8");
    let (mut nl, mut label, mut euro_a2e3, mut feff, mut bom8) = ("", None, false, 0, false);
    for p in parts {
        match p {
            "euro=a2e3" => euro_a2e3 = true,
            "bom8" => bom8 = true,
            "nl=crlf" => nl = "\r\n",
            "nl=cr" => nl = "\r",
            _ => {
                if let Some(l) = p.strip_prefix("label=") {
                    label = Some(l);
                } else if let Some(n) = p.strip_prefix("feff=") {
                    feff = n.parse().unwrap_or(0);
                }
            }
        }
    }
    Envelope { base, nl, label, euro_a2e3, feff, bom8 }
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
    // LocOS rewrites every declaration of a converted file to UTF-8 when it reads it (not only those near the top)
    // and gives each the file's own label back when it writes it. Under the LocOS profile a file that carries a
    // label does the same: a declaration further down becomes the file's label (or UTF-8, when the file is written
    // as UTF-8). Without the profile it is content and comes back as it was.
    let every;
    let text = if opts.is_profile(crate::profile::LOCOS) && parse_tag(encoding).label.is_some() {
        every = decl_re().replace_all(text, |c: &regex::Captures| format!("{}UTF-8", &c[1])).into_owned();
        every.as_str()
    } else {
        text
    };
    if opts.output_encoding != OutputEncoding::Utf8 {
        return encode_text(text, encoding);
    }
    Ok(utf8_text(text, encoding, false))
}

/// [`encode_text_with`] for an HTML page (a format whose LocOS filter reads with `declared=True`). LocOS rewrites
/// every charset declaration of a converted page to UTF-8 when it reads it and, when the page has no label of its own
/// to give back (its only declaration sits past the first 4096 characters, so it never named the page's charset),
/// writes the page as UTF-8: the gate in `encode_like` finds a declaration that does not name the code page. Under the
/// LocOS profile a legacy page with no label and such a late declaration does the same. Native mode keeps the
/// declaration as content and the code page, as before.
pub fn encode_declared_with(text: &str, encoding: &str, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
    if opts.is_profile(crate::profile::LOCOS) {
        let env = parse_tag(encoding);
        let legacy = !(env.base.starts_with("utf-8") || env.base.starts_with("utf-16"));
        if legacy && env.label.is_none() && decl_re().is_match(text) {
            let every = decl_re().replace_all(text, |c: &regex::Captures| format!("{}UTF-8", &c[1])).into_owned();
            return Ok(utf8_text(&every, encoding, false));
        }
    }
    encode_text_with(text, encoding, opts)
}

/// For a format crate with its own codec and its own encoding names: the bytes to write when `opts` asks for UTF-8
/// out ([`OutputEncoding::Utf8`]), or `None` when the crate writes its own encoding (`Keep`). The text is written as
/// UTF-8 (a UTF-8 BOM is kept when `encoding` is `utf-8-bom`; a UTF-16 BOM is not), and an XML, HTML or PO charset
/// declaration that names `encoding` (or UTF-16, for a UTF-16 file) is rewritten to say UTF-8. Newlines stay as
/// they are in `text`, except that a model envelope's `nl=` is honoured.
pub fn utf8_output(text: &str, encoding: &str, opts: &MergeOptions) -> Option<Vec<u8>> {
    (opts.output_encoding == OutputEncoding::Utf8).then(|| utf8_text(text, encoding, true))
}

fn utf8_text(text: &str, encoding: &str, base_is_label: bool) -> Vec<u8> {
    let env = parse_tag(encoding);
    let text = strip_own_mark(text, encoding);
    let mut out = if env.nl.is_empty() { text.to_string() } else { with_newlines(text, env.nl) };
    let is_utf8 = |l: &str| l.eq_ignore_ascii_case("utf-8") || l.eq_ignore_ascii_case("utf8") || l.eq_ignore_ascii_case("utf-8-bom");
    let mut labels: Vec<&str> = env.label.into_iter().collect();
    if base_is_label {
        labels.push(env.base);
        if env.base.to_ascii_lowercase().starts_with("utf-16") {
            labels.push("utf-16");
        }
    }
    labels.retain(|l| !is_utf8(l));
    if !labels.is_empty() {
        out = decl_re()
            .replace_all(&out, |c: &regex::Captures| {
                if labels.iter().any(|l| c[2].eq_ignore_ascii_case(l)) {
                    format!("{}UTF-8", &c[1])
                } else {
                    c[0].to_string()
                }
            })
            .into_owned();
    }
    let mut v = Vec::with_capacity(out.len() + 3);
    if env.base == "utf-8-bom" || env.bom8 {
        v.extend_from_slice(BOM8);
    }
    v.extend_from_slice(out.as_bytes());
    v
}

/// Writes the merged `text` in the envelope `encoding` (a skeleton tag from [`decode_text`], or plain `"utf-8"` /
/// `"utf-8-bom"`). Text that does not fit the original legacy encoding is written as UTF-8.
///
/// A legacy code page result that the same extract would refuse as binary ([`looks_binary`]: a merge that shortened
/// a file below its control-byte allowance) is written as UTF-8 instead, which LocOS also writes and which always
/// reads back ([`legacy_output_or_utf8`]).
pub fn encode_text(text: &str, encoding: &str) -> Result<Vec<u8>, Error> {
    let out = encode_text_as_is(text, encoding)?;
    legacy_output_or_utf8(out, text, encoding)
}

/// For a writer with its own legacy codec: `out` (the bytes it wrote for `text` under the skeleton tag `encoding`)
/// unless they are a legacy code page that [`decode_text`] would refuse as binary; then `text` written as UTF-8
/// (newline style kept). Every filter that writes a legacy code page goes through this, so a merge never writes a
/// file its own extract refuses.
pub fn legacy_output_or_utf8(out: Vec<u8>, text: &str, encoding: &str) -> Result<Vec<u8>, Error> {
    let b = base(encoding);
    let legacy = !matches!(b, "utf-8" | "utf-8-bom" | "utf-16le" | "utf-16be");
    if legacy && std::str::from_utf8(&out).is_err() && looks_binary(&out) {
        return encode_text_as_is(text, &format!("utf-8{}", &encoding[b.len()..]));
    }
    Ok(out)
}

fn encode_text_as_is(text: &str, encoding: &str) -> Result<Vec<u8>, Error> {
    let mut out = encode_text_unmarked(text, encoding)?;
    // a mark read before the legacy bytes goes back in front of whatever was written (UTF-8 or the code page)
    if parse_tag(encoding).bom8 {
        out.splice(0..0, BOM8.iter().copied());
    }
    Ok(out)
}

fn encode_text_unmarked(text: &str, encoding: &str) -> Result<Vec<u8>, Error> {
    let env = parse_tag(encoding);
    let text = strip_own_mark(text, encoding);
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
    // a declared charset: each declaration parse rewrote to UTF-8 gets its own label back. The whole text is
    // looked at, not only its head: a longer translation before a declaration near the 4096th character moves it
    // out of the head, and it must still say the charset the bytes are written in
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
        let agree = decl_re().captures_iter(split_head(&restored).0).all(|c| {
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
    let agree = decl_re().captures_iter(split_head(&restored).0).all(|c| {
        let name = c.get(2).map_or("", |m| m.as_str());
        label_encoding(name) == Some(enc)
            || env.label.is_some_and(|l| l.eq_ignore_ascii_case(name))
            || name.eq_ignore_ascii_case("us-ascii")
    });
    if !agree {
        return Ok(utf8(false));
    }
    // A page labelled ISO-8859-8 that was read as Windows-1255 (it holds vowel points) keeps its label only while
    // no character is one that Windows-1255 writes at a byte meaning another sign under that label (curly quotes,
    // dashes, the ellipsis, the euro and shekel signs): a browser would show a C1 control or the currency sign.
    // Such a page is written as UTF-8 (LocOS #810 gate, G7).
    if enc == encoding_rs::WINDOWS_1255 {
        let says_8859_8 = decl_re().captures_iter(split_head(&restored).0).any(|c| {
            let name = c.get(2).map_or("", |m| m.as_str());
            matches!(label_encoding(name), Some(e) if e == encoding_rs::ISO_8859_8 || e == encoding_rs::ISO_8859_8_I)
        });
        if says_8859_8 && restored.chars().any(|c| cp1255_only_signs().contains(&c)) {
            return Ok(utf8(false));
        }
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

/// The characters Windows-1255 writes at a byte that ISO-8859-8 reads as another character (both define the byte):
/// the C1 range signs Windows-1255 uses for curly quotes, dashes, the ellipsis, the euro and the new shekel sign.
fn cp1255_only_signs() -> &'static [char] {
    static S: OnceLock<Vec<char>> = OnceLock::new();
    S.get_or_init(|| {
        (0x80u8..=0xFF)
            .filter_map(|b| {
                let one = [b];
                let a = encoding_rs::WINDOWS_1255.decode_without_bom_handling_and_without_replacement(&one)?;
                let c = encoding_rs::ISO_8859_8.decode_without_bom_handling_and_without_replacement(&one)?;
                (a != c).then(|| a.chars().next()).flatten()
            })
            .collect()
    })
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
        let o = DecodeOptions { declared, source_lang: lang, limits: Limits::default(), lossy_after_utf8_bom: false, mark_before_legacy: false };
        decode_text(bytes, &o)
    }

    fn legacy(enc: &'static Encoding, s: &str) -> Vec<u8> {
        let (b, _, bad) = enc.encode(s);
        assert!(!bad);
        b.into_owned()
    }

    /// LocOS BOM read version 2 (#799.4, #800.4; B-SIDE W6 st-1, ass-1): with `mark_before_legacy` a UTF-8 BOM
    /// before Windows-1252 bytes is a mark, the rest is read in the code page; the mark comes back on merge.
    #[test]
    fn mark_before_legacy_bytes() {
        let data = b"\xef\xbb\xbfCaf\xe9\nSecond\n";
        let o = DecodeOptions::with_limits(Limits::default()).mark_before_legacy();
        let d = decode_text(data, &o).unwrap();
        assert_eq!(d.text, "Caf\u{e9}\nSecond\n");
        assert_eq!(d.encoding, "windows-1252;bom8");
        assert_eq!(encode_text(&d.text, &d.encoding).unwrap(), data.to_vec());
        // UTF-8 out (the LocOS profile): the mark, then UTF-8
        let opts = MergeOptions::new().with_output_encoding(OutputEncoding::Utf8);
        assert_eq!(encode_text_with(&d.text, &d.encoding, &opts).unwrap(), "\u{feff}Caf\u{e9}\nSecond\n".as_bytes());
        assert_eq!(utf8_output(&d.text, &d.encoding, &opts).unwrap(), "\u{feff}Caf\u{e9}\nSecond\n".as_bytes());
        // without the option the mark's bytes are code-page text, as before
        let d = decode_text(data, &DecodeOptions::with_limits(Limits::default())).unwrap();
        assert!(d.text.starts_with("\u{ef}\u{bb}\u{bf}Caf"), "{:?}", d.text);
        // a valid UTF-8 file after the mark is unchanged
        let d = decode_text("\u{feff}Caf\u{e9}".as_bytes(), &o).unwrap();
        assert_eq!((d.text.as_str(), d.encoding.as_str()), ("Caf\u{e9}", "utf-8-bom"));
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

    /// A file with two (or three) byte order marks: the marks after the first are text, and every envelope writes
    /// them back; a writer that puts one more U+FEFF in front (its own mark) still gets exactly one taken off.
    #[test]
    fn a_second_byte_order_mark_is_content_and_kept() {
        let utf16 = |be: bool, s: &str| {
            let mut v = if be { vec![0xFE, 0xFF] } else { vec![0xFF, 0xFE] };
            for u in s.encode_utf16() {
                v.extend_from_slice(&if be { u.to_be_bytes() } else { u.to_le_bytes() });
            }
            v
        };
        for marks in 1..=2 {
            let text = format!("{}a\r\nb\r\n", "\u{FEFF}".repeat(marks));
            for b in [[BOM8, text.as_bytes()].concat(), utf16(false, &text), utf16(true, &text)] {
                let d = dec(&b, None, false).unwrap();
                assert_eq!(d.text, text);
                assert!(d.encoding.contains(&format!(";feff={marks}")), "{}", d.encoding);
                assert_eq!(encode_text(&d.text, &d.encoding).unwrap(), b);
                assert_eq!(encode_text(&format!("\u{FEFF}{}", d.text), &d.encoding).unwrap(), b, "own mark taken off");
                assert_eq!(locos_skeleton_parts(&locos_skeleton(&d.encoding, &d.text)).unwrap().1, text);
                let utf8 = encode_text_with(&d.text, &d.encoding, &MergeOptions { output_encoding: OutputEncoding::Utf8, ..Default::default() }).unwrap();
                assert!(utf8.ends_with(text.as_bytes()), "UTF-8 output keeps the content marks");
            }
        }
        // one mark only: the tag and the writer are as before
        let d = dec(b"\xEF\xBB\xBFa", None, false).unwrap();
        assert_eq!(d.encoding, "utf-8-bom");
        assert_eq!(encode_text("\u{FEFF}a", &d.encoding).unwrap(), b"\xEF\xBB\xBFa");
        assert_eq!(strip_own_mark("\u{FEFF}a", "utf-8-bom"), "a");
        assert_eq!(strip_own_mark("\u{FEFF}a", "utf-8-bom;feff=1"), "\u{FEFF}a");
        assert_eq!(strip_own_mark("\u{FEFF}\u{FEFF}a", "utf-16le;nl=crlf;feff=1;label=utf-16"), "\u{FEFF}a");
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
    fn a_declaration_deep_in_the_body_is_content_and_comes_back_as_written() {
        // a page that embeds another page: the second charset name is text, not the file's declaration
        let pad = "<p>x</p>".repeat(700);
        let src = format!(
            "<html><head><meta http-equiv=\"Content-Type\" content=\"text/html; charset=windows-1251\"></head><body>{pad}<p>\u{41f}\u{440}\u{438}</p><!-- <meta charset=windows-1252> --></body></html>"
        );
        let b = legacy(encoding_rs::WINDOWS_1251, &src);
        let d = dec(&b, Some("en"), true).unwrap();
        assert_eq!(d.encoding, "windows-1251;label=windows-1251");
        assert!(d.text.contains("charset=UTF-8\"") && d.text.contains("<meta charset=windows-1252>"));
        assert_eq!(encode_text(&d.text, &d.encoding).unwrap(), b);
    }

    #[test]
    fn a_declaration_moved_past_the_head_by_a_longer_translation_still_gets_its_label_back() {
        // the page's own declaration sits just inside the first 4096 characters, after a title
        let pad = "x".repeat(3900);
        let src = format!("<html><head><title>Hi</title><style>{pad}</style><meta charset=\"windows-1251\"></head><body>\u{41f}</body></html>");
        let b = legacy(encoding_rs::WINDOWS_1251, &src);
        let d = dec(&b, Some("en"), true).unwrap();
        assert!(d.text.contains("charset=\"UTF-8\""));
        let longer = d.text.replace("<title>Hi</title>", &format!("<title>{}</title>", "\u{41f}".repeat(400)));
        let out = encode_text(&longer, &d.encoding).unwrap();
        let back = encoding_rs::WINDOWS_1251.decode(&out).0.into_owned();
        assert!(back.contains("charset=\"windows-1251\"") && !back.contains("UTF-8"), "the bytes are Windows-1251 and must say so");
    }

    #[test]
    fn a_file_whose_only_declaration_is_past_the_head_is_read_with_a_warning() {
        let pad = "<p>x</p>".repeat(600);
        let src = format!("<html><head><script>{pad}</script><meta charset=\"windows-1251\"></head><body>\u{41f}\u{440}\u{438}</body></html>");
        let b = legacy(encoding_rs::WINDOWS_1251, &src);
        let d = dec(&b, Some("en"), true).unwrap();
        // not the file's declaration (it may be content): read by the default path, never rewritten, and said so
        assert!(d.text.contains("charset=\"windows-1251\""));
        assert!(d.warnings.iter().any(|w| w.message.contains("windows-1251 further down")), "{:?}", d.warnings);
        assert_eq!(encode_text(&d.text, &d.encoding).unwrap(), b);
        // the same page in the code page it is read in: no warning
        let d = dec(&b, Some("ru"), true).unwrap();
        assert!(d.text.contains("\u{41f}\u{440}\u{438}") && d.warnings.is_empty(), "{:?}", d.warnings);
    }

    #[test]
    fn declared_hebrew_and_other_rare_charsets_are_read_not_mojibake() {
        // ISO-8859-8 was read as Windows-1252 soup before
        let src = "<html><head><meta charset=\"iso-8859-8\"></head><body>\u{5e9}\u{5dc}\u{5d5}\u{5dd}</body></html>";
        let b = legacy(encoding_rs::ISO_8859_8, src);
        let d = dec(&b, None, true).unwrap();
        assert!(d.text.contains("\u{5e9}\u{5dc}\u{5d5}\u{5dd}"), "{}", d.text);
        assert_eq!(encode_text(&d.text, &d.encoding).unwrap(), b);
        let src = "<meta charset=\"windows-874\"><p>\u{e2a}\u{e27}\u{e31}\u{e2a}\u{e14}\u{e35}</p>";
        let b = legacy(encoding_rs::WINDOWS_874, src);
        let d = dec(&b, None, true).unwrap();
        assert!(d.text.contains("\u{e2a}\u{e27}"), "{}", d.text);
        assert_eq!(encode_text(&d.text, &d.encoding).unwrap(), b);
    }

    #[test]
    fn rare_declared_charsets_never_refuse_a_file_they_used_to_open_and_never_lose_a_character() {
        // a page labelled ISO-8859-8 with a Windows-1255 vowel point (qamats, byte 0xC8, undefined in ISO-8859-8)
        let mut b = b"<meta charset=\"iso-8859-8\"><p>".to_vec();
        b.extend_from_slice(&[0xF9, 0xC8, 0xEC, 0xE5, 0xED]);
        b.extend_from_slice(b"</p>");
        let d = dec(&b, None, true).unwrap();
        assert!(d.text.contains("\u{5e9}\u{5b8}\u{5dc}\u{5d5}\u{5dd}"), "{}", d.text);
        assert_eq!(encode_text(&d.text, &d.encoding).unwrap(), b);
        // ISO-8859-6 with a byte it leaves undefined (0xA1): read by the default path as before, not refused
        let b = b"<meta charset=\"iso-8859-6\"><p>a\xA1b</p>".to_vec();
        let d = dec(&b, None, true).unwrap();
        assert_eq!(encode_text(&d.text, &d.encoding).unwrap(), b);
        // Mac Roman and Mac Cyrillic: the unchanged text comes back byte for byte
        for (e, s) in [(encoding_rs::MACINTOSH, "<meta charset=\"macintosh\"><p>caf\u{e9} \u{2022}</p>"), (encoding_rs::X_MAC_CYRILLIC, "<meta charset=\"x-mac-cyrillic\"><p>\u{43f}\u{440}\u{438}\u{432}\u{435}\u{442}</p>")] {
            let b = legacy(e, s);
            let d = dec(&b, None, true).unwrap();
            assert!(d.text.contains(&s[s.find("<p>").unwrap()..]), "{}", d.text);
            assert_eq!(encode_text(&d.text, &d.encoding).unwrap(), b);
        }
        // a translation the code page cannot hold: the whole file is written as UTF-8 and says so, nothing is replaced
        let b = legacy(encoding_rs::ISO_8859_8, "<meta charset=\"iso-8859-8\"><p>\u{5e9}\u{5dc}\u{5d5}\u{5dd}</p>");
        let d = dec(&b, None, true).unwrap();
        let out = encode_text(&d.text.replace("\u{5e9}\u{5dc}\u{5d5}\u{5dd}", "\u{65e5}\u{672c}"), &d.encoding).unwrap();
        let out = String::from_utf8(out).unwrap();
        assert!(out.contains("charset=\"UTF-8\"") && out.contains("\u{65e5}\u{672c}"), "{out}");
    }

    #[test]
    fn iso_8859_8_label_over_windows_1255_bytes_becomes_utf8_for_signs_it_lacks() {
        let mut b = b"<meta charset=\"iso-8859-8\"><p>".to_vec();
        b.extend_from_slice(&[0xF9, 0xC8, 0xEC, 0xE5, 0xED]);
        b.extend_from_slice(b"</p>");
        let d = dec(&b, None, true).unwrap();
        assert!(d.encoding.starts_with("windows-1255"), "{}", d.encoding);
        // a curly quote, a dash, an ellipsis, the euro sign, the new shekel sign: not in ISO-8859-8
        for sign in ["\u{201c}", "\u{2013}", "\u{2026}", "\u{20ac}", "\u{20aa}"] {
            let out = encode_text(&format!("{}{sign}", d.text), &d.encoding).unwrap();
            let out = String::from_utf8(out).unwrap_or_else(|_| panic!("{sign:?} was written as code page bytes"));
            assert!(out.contains("charset=\"UTF-8\"") && out.contains(sign), "{out}");
        }
        // a letter and a vowel point both exist in the page's code page: the label stays
        let out = encode_text(&format!("{}\u{5d0}\u{5b0}", d.text), &d.encoding).unwrap();
        assert!(std::str::from_utf8(&out).is_err() && out.starts_with(b"<meta charset=\"iso-8859-8\">"));
    }

    #[test]
    fn locos_profile_gives_every_declaration_the_files_label_or_utf8() {
        // a Windows-1256 page whose first declaration says so and whose second one (past the head) says 1252
        let page = format!("<meta charset=\"windows-1256\"><p>{}</p><meta charset=\"windows-1252\">", "x".repeat(5000));
        let b = legacy(encoding_rs::WINDOWS_1256, &page);
        let d = dec(&b, None, true).unwrap();
        assert_eq!(d.encoding, "windows-1256;label=windows-1256");
        let locos = MergeOptions::new().with_profile(crate::profile::LOCOS);
        // native: the late declaration is content and stays; LocOS: it takes the file's label
        assert_eq!(encode_text_with(&d.text, &d.encoding, &MergeOptions::new()).unwrap(), b);
        let out = String::from_utf8(encode_text_with(&d.text, &d.encoding, &locos).unwrap()).unwrap();
        assert!(out.ends_with("<meta charset=\"windows-1256\">") && !out.contains("1252"), "{}", &out[out.len() - 60..]);
        // a translation the code page cannot hold: UTF-8, and no declaration keeps the old label (LocOS)
        let text = d.text.replacen("xx", "\u{65e5}\u{672c}", 1);
        let out = String::from_utf8(encode_text_with(&text, &d.encoding, &locos).unwrap()).unwrap();
        assert!(!out.contains("1256") && !out.contains("1252") && out.matches("charset=\"UTF-8\"").count() == 2, "{}", &out[out.len() - 60..]);
    }

    #[test]
    fn locos_profile_writes_a_legacy_page_with_only_a_late_declaration_as_utf8() {
        // the only declaration is past the first 4096 characters: the page is read as Windows-1252 with no label
        let page = format!("<html><head><script>/*{}*/</script><meta charset=\"windows-1252\"></head><body><p>Caf\u{e9}</p></body></html>", "x".repeat(5000));
        let b = legacy(encoding_rs::WINDOWS_1252, &page);
        let d = dec(&b, None, true).unwrap();
        assert_eq!(d.encoding, "windows-1252");
        let locos = MergeOptions::new().with_profile(crate::profile::LOCOS);
        // native: the code page and the declaration as they were
        assert_eq!(encode_declared_with(&d.text, &d.encoding, &MergeOptions::new()).unwrap(), b);
        // LocOS: UTF-8 under a UTF-8 label, whether the text fits the code page or not
        for text in [d.text.clone(), d.text.replace("Caf\u{e9}", "\u{436}")] {
            let out = String::from_utf8(encode_declared_with(&text, &d.encoding, &locos).unwrap()).unwrap();
            assert!(out.contains("<meta charset=\"UTF-8\">") && !out.contains("1252"), "{}", &out[out.len() - 80..]);
        }
        // a page with no declaration at all keeps its code page
        let plain = legacy(encoding_rs::WINDOWS_1252, "<p>Caf\u{e9}</p>");
        let d = dec(&plain, None, true).unwrap();
        assert_eq!(encode_declared_with(&d.text, &d.encoding, &locos).unwrap(), plain);
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
    fn python_charset_names_are_read_and_written_back() {
        use encoding_rs::*;
        // LocOS reads a declared charset with Python's codec lookup: CP932, cp936, sjis, uhc are not WHATWG labels
        for (l, e) in [
            ("CP932", SHIFT_JIS),
            ("ms_kanji", SHIFT_JIS),
            ("x.mac.japanese", SHIFT_JIS),
            ("cp936", GBK),
            ("UHC", EUC_KR),
            ("cp949", EUC_KR),
            ("ujis", EUC_JP),
            ("iso8859_2", ISO_8859_2),
            ("KOI8_R", KOI8_R),
        ] {
            assert_eq!(label_encoding(l), Some(e), "{l}");
        }
        // WHATWG first: its meaning wins where both know a label; unknown names stay unknown
        assert_eq!(label_encoding("latin1"), Some(WINDOWS_1252));
        assert_eq!(label_encoding("x-made-up"), None);
        assert_eq!(label_encoding("cp437"), None);
        assert_eq!(label_encoding("MS-932"), None); // Python does not know it either
        assert!(PY_ALIASES.windows(2).all(|w| w[0].0 < w[1].0), "PY_ALIASES must stay sorted");
        // a PO header declaring CP932: read as Shift_JIS, written back in it under its own label
        let mut b = b"msgid \"\"\nmsgstr \"Content-Type: text/plain; charset=CP932\\n\"\n\nmsgid \"a\"\nmsgstr \"".to_vec();
        b.extend_from_slice(&[0x93, 0xfa, 0x96, 0x7b]);
        b.extend_from_slice(b"\"\n");
        let d = dec(&b, Some("en"), true).unwrap();
        assert!(d.text.contains("\u{65e5}\u{672c}") && d.text.contains("charset=UTF-8"), "{}", d.text);
        assert!(d.encoding.starts_with("shift_jis") && d.encoding.contains("label=CP932"), "{}", d.encoding);
        assert_eq!(encode_text(&d.text, &d.encoding).unwrap(), b);
    }

    #[test]
    fn line_budget_applies_to_decoded_text() {
        let mut o = DecodeOptions { declared: false, source_lang: None, limits: Limits::default(), lossy_after_utf8_bom: false, mark_before_legacy: false };
        o.limits.max_lines = 3;
        assert!(decode_text(b"a\nb\n", &o).is_ok());
        let e = decode_text(b"\n\n\n\n\n", &o).unwrap_err();
        assert!(matches!(e, Error::Limit(l) if l.kind == crate::LimitKind::Lines));
    }
}

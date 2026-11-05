//! Charsets of text parts and encoded words: the labels a mail client writes, mapped onto decoders and encoders
//! that fail loudly (strict decode, no unmappable characters on encode), as Python's codecs do for LocOS.

use bilingua_model::encoding_rs::{self, Encoding};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Codec {
    Ascii,
    /// True ISO-8859-1 (every byte is the code point of the same number), unlike WHATWG's Windows-1252 alias.
    Latin1,
    Enc(&'static Encoding),
    /// ISO-2022-JP: decoded by `encoding_rs`, encoded here (the crate cannot encode it).
    Iso2022Jp,
}

pub const UTF8: Codec = Codec::Enc(encoding_rs::UTF_8);
pub const CP1252: Codec = Codec::Enc(encoding_rs::WINDOWS_1252);

const ESC_ASCII: &[u8] = b"\x1b(B";
const ESC_JIS: &[u8] = b"\x1b$B";

impl Codec {
    /// The codec for a charset label, or `None` when it is unknown or cannot be used for text (UTF-16 and the
    /// WHATWG "replacement" family are refused, as a label no text part should carry).
    pub fn lookup(label: &str) -> Option<Codec> {
        let l = label.trim().to_ascii_lowercase();
        match l.as_str() {
            "ascii" | "us-ascii" | "us" | "iso646-us" | "ansi_x3.4-1968" | "ansi_x3.4_1968" | "646" | "iso-ir-6" | "cp367" | "ibm367" | "csascii" => Some(Codec::Ascii),
            "iso-8859-1" | "iso8859-1" | "iso_8859-1" | "latin1" | "latin-1" | "latin_1" | "l1" | "cp819" | "ibm819" | "csisolatin1" | "iso-ir-100" | "8859" | "iso88591" | "iso_8859-1:1987" => Some(Codec::Latin1),
            "iso-2022-jp" | "iso2022-jp" | "iso2022_jp" | "csiso2022jp" => Some(Codec::Iso2022Jp),
            _ => {
                let e = Encoding::for_label(l.as_bytes()).or_else(|| python_alias(&l))?;
                if e == encoding_rs::REPLACEMENT || e == encoding_rs::UTF_16LE || e == encoding_rs::UTF_16BE || e == encoding_rs::X_USER_DEFINED {
                    return None;
                }
                Some(Codec::Enc(e))
            }
        }
    }

    /// Strict decode: `Err(())` when a byte does not belong to the charset.
    pub fn decode(self, b: &[u8]) -> Result<String, ()> {
        match self {
            Codec::Ascii => {
                if b.is_ascii() {
                    Ok(String::from_utf8_lossy(b).into_owned())
                } else {
                    Err(())
                }
            }
            Codec::Latin1 => Ok(b.iter().map(|c| *c as char).collect()),
            Codec::Enc(e) if e == encoding_rs::UTF_8 => String::from_utf8(b.to_vec()).map_err(|_| ()),
            Codec::Enc(e) => strict(e, b),
            Codec::Iso2022Jp => strict(encoding_rs::ISO_2022_JP, &merge_escapes(b)),
        }
    }

    /// Encode, or `None` when a character does not fit the charset.
    pub fn encode(self, s: &str) -> Option<Vec<u8>> {
        match self {
            Codec::Ascii => s.is_ascii().then(|| s.as_bytes().to_vec()),
            Codec::Latin1 => s.chars().map(|c| u8::try_from(c as u32).ok()).collect(),
            Codec::Enc(e) if e == encoding_rs::UTF_8 => Some(s.as_bytes().to_vec()),
            Codec::Enc(e) => {
                if s.is_ascii() && e.is_ascii_compatible() {
                    return Some(s.as_bytes().to_vec());
                }
                let (out, used, bad) = e.encode(s);
                (!bad && used == e).then(|| out.into_owned())
            }
            Codec::Iso2022Jp => encode_iso2022jp(s),
        }
    }
}

/// Decodes all of `b` or fails (Windows-1252 is strict like Python's `cp1252`: its five undefined bytes are errors).
fn strict(e: &'static Encoding, b: &[u8]) -> Result<String, ()> {
    if e == encoding_rs::WINDOWS_1252 && b.iter().any(|c| matches!(c, 0x81 | 0x8D | 0x8F | 0x90 | 0x9D)) {
        return Err(());
    }
    e.decode_without_bom_handling_and_without_replacement(b).map(|c| c.into_owned()).ok_or(())
}

/// Drops an escape sequence that is directly followed by another one (what joining two encoded words leaves:
/// `ESC ( B` then `ESC $ B`). Python reads that; the WHATWG decoder treats it as an error.
fn merge_escapes(b: &[u8]) -> Vec<u8> {
    let is_esc = |i: usize| b.len() >= i + 3 && b[i] == 0x1b && matches!(b[i + 1], b'(' | b'$');
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if is_esc(i) && is_esc(i + 3) {
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    out
}

fn encode_iso2022jp(s: &str) -> Option<Vec<u8>> {
    let mut out: Vec<u8> = Vec::with_capacity(s.len() + 8);
    let mut jis = false;
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut Vec<u8>, jis: &mut bool| -> Option<()> {
        if run.is_empty() {
            return Some(());
        }
        let (bytes, _, bad) = encoding_rs::EUC_JP.encode(run);
        if bad || bytes.len() % 2 != 0 {
            return None;
        }
        if !*jis {
            out.extend_from_slice(ESC_JIS);
            *jis = true;
        }
        for pair in bytes.chunks(2) {
            if pair[0] < 0xA1 || pair[0] > 0xFE || pair[1] < 0xA1 || pair[1] > 0xFE {
                return None;
            }
            out.push(pair[0] - 0x80);
            out.push(pair[1] - 0x80);
        }
        run.clear();
        Some(())
    };
    for c in s.chars() {
        if c.is_ascii() {
            flush(&mut run, &mut out, &mut jis)?;
            if jis {
                out.extend_from_slice(ESC_ASCII);
                jis = false;
            }
            out.push(c as u8);
        } else {
            run.push(c);
        }
    }
    flush(&mut run, &mut out, &mut jis)?;
    if jis {
        out.extend_from_slice(ESC_ASCII);
    }
    Some(out)
}

/// The MIME charset name written into a header for a codec found by decoding an undeclared body.
/// Names Python's codecs accept that the WHATWG label table lacks (Localization OS writes two of them itself:
/// `eucgb2312_cn` and `big5-tw` in encoded words).
fn python_alias(l: &str) -> Option<&'static Encoding> {
    let n = l.replace('_', "-");
    match n.as_str() {
        "eucgb2312-cn" | "cp936" | "gb-2312" => return Some(encoding_rs::GBK),
        "big5-tw" | "cp950" => return Some(encoding_rs::BIG5),
        "cp949" | "uhc" | "euc-kr" => return Some(encoding_rs::EUC_KR),
        "cp932" | "sjis" => return Some(encoding_rs::SHIFT_JIS),
        "euc-jp" => return Some(encoding_rs::EUC_JP),
        "cp874" => return Some(encoding_rs::WINDOWS_874),
        _ => {}
    }
    if let Some(num) = n.strip_prefix("cp").filter(|r| r.len() == 4 && r.starts_with("125")) {
        return Encoding::for_label(format!("windows-{num}").as_bytes());
    }
    if let Some(rest) = n.strip_prefix("iso8859-").or_else(|| n.strip_prefix("iso-8859")) {
        return Encoding::for_label(format!("iso-8859-{}", rest.trim_start_matches('-')).as_bytes());
    }
    None
}

pub fn mime_name(c: Codec) -> &'static str {
    match c {
        Codec::Ascii => "us-ascii",
        Codec::Latin1 => "iso-8859-1",
        Codec::Enc(e) if e == encoding_rs::UTF_8 => "utf-8",
        Codec::Enc(e) if e == encoding_rs::WINDOWS_1252 => "windows-1252",
        Codec::Enc(_) | Codec::Iso2022Jp => "utf-8",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_and_strictness() {
        assert_eq!(Codec::lookup("US-ASCII"), Some(Codec::Ascii));
        assert_eq!(Codec::lookup("ISO-8859-1"), Some(Codec::Latin1));
        assert_eq!(Codec::lookup("utf-8"), Some(UTF8));
        assert!(Codec::lookup("x-made-up").is_none());
        assert!(Codec::lookup("base64").is_none());
        assert!(Codec::lookup("utf-16").is_none());
        assert!(Codec::lookup("hz-gb-2312").is_none());
        assert!(Codec::Ascii.decode(b"\xe9").is_err());
        assert_eq!(Codec::Latin1.decode(b"\x80\xe9").unwrap(), "\u{80}\u{e9}");
        assert!(UTF8.decode(b"\xff").is_err());
        assert!(CP1252.decode(b"\x81").is_err());
    }

    #[test]
    fn iso2022jp_round_trip() {
        let s = "abc \u{65e5}\u{672c}\u{8a9e} xyz";
        let b = Codec::Iso2022Jp.encode(s).unwrap();
        assert!(b.starts_with(b"abc \x1b$B"));
        assert!(b.ends_with(b"\x1b(B xyz"));
        assert_eq!(Codec::Iso2022Jp.decode(&b).unwrap(), s);
        assert!(Codec::Iso2022Jp.encode("\u{0e01}").is_none());
    }

    #[test]
    fn cjk_encoders() {
        for (label, text) in [("shift_jis", "\u{65e5}\u{672c}"), ("euc-kr", "\u{d55c}\u{ae00}"), ("gbk", "\u{4e2d}\u{6587}"), ("big5", "\u{4e2d}\u{6587}")] {
            let c = Codec::lookup(label).unwrap();
            let b = c.encode(text).unwrap();
            assert_eq!(c.decode(&b).unwrap(), text);
        }
        assert!(Codec::Latin1.encode("\u{20ac}").is_none());
        assert!(CP1252.encode("\u{4e2d}").is_none());
    }
}


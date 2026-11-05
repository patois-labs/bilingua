//! "The decoded letters look wrong": the script-mix checks Localization OS runs on a text it had to read with a
//! guessed code page (`text_looks_garbled`, decision 22). Heuristics, deliberately conservative: a warning is
//! never an error.

use crate::codepage::{code_page_for_encoding, native_letters, target_code_page};
use encoding_rs::Encoding;

const SAMPLE: usize = 200_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Script {
    Latin,
    Greek,
    Cyrillic,
    Hebrew,
    Arabic,
    Thai,
    HwKana,
    Cjk,
    Other,
}

fn script(c: char) -> Script {
    let o = c as u32;
    if o < 0x250 || (0x1E00..=0x1EFF).contains(&o) {
        Script::Latin
    } else if (0x370..=0x3FF).contains(&o) || (0x1F00..=0x1FFF).contains(&o) {
        Script::Greek
    } else if (0x400..=0x52F).contains(&o) {
        Script::Cyrillic
    } else if (0x590..=0x5FF).contains(&o) || (0xFB1D..=0xFB4F).contains(&o) {
        Script::Hebrew
    } else if (0x600..=0x77F).contains(&o) || (0xFB50..=0xFDFF).contains(&o) || (0xFE70..=0xFEFF).contains(&o) {
        Script::Arabic
    } else if (0xE00..=0xE7F).contains(&o) {
        Script::Thai
    } else if (0xFF61..=0xFF9F).contains(&o) {
        Script::HwKana
    } else if (0x3040..=0x30FF).contains(&o)
        || (0x3400..=0x9FFF).contains(&o)
        || (0xF900..=0xFAFF).contains(&o)
        || (0xAC00..=0xD7AF).contains(&o)
        || (0x1100..=0x11FF).contains(&o)
        || (0x3130..=0x318F).contains(&o)
        || (0xFF00..=0xFF60).contains(&o)
        || (0x20000..=0x2FA1F).contains(&o)
    {
        Script::Cjk
    } else {
        Script::Other
    }
}

/// True for CJK text (Japanese, Chinese, Korean), the family whose files mix ASCII letters with ideographs.
pub fn is_cjk_char(c: char) -> bool {
    script(c) == Script::Cjk
}

/// Letters plus the Hebrew, Arabic and Thai combining marks: a Western letter read as Hebrew can come out as a
/// vowel point glued to a Latin letter.
fn is_word_char(c: char) -> bool {
    let o = c as u32;
    (c.is_alphabetic() && !c.is_numeric() && c != '_')
        || (0x591..=0x5C7).contains(&o)
        || (0x610..=0x61A).contains(&o)
        || (0x64B..=0x65F).contains(&o)
        || o == 0x670
        || o == 0xE31
        || (0xE34..=0xE3A).contains(&o)
        || (0xE47..=0xE4E).contains(&o)
}

fn is_multibyte(e: &'static Encoding) -> bool {
    [encoding_rs::SHIFT_JIS, encoding_rs::EUC_JP, encoding_rs::GBK, encoding_rs::GB18030, encoding_rs::BIG5, encoding_rs::EUC_KR]
        .contains(&e)
}

fn cjk_family(cp: u16) -> bool {
    matches!(cp, 932 | 936 | 949 | 950)
}

fn utf8_check(e: &'static Encoding) -> bool {
    [
        encoding_rs::WINDOWS_1250,
        encoding_rs::WINDOWS_1251,
        encoding_rs::WINDOWS_1252,
        encoding_rs::WINDOWS_1253,
        encoding_rs::WINDOWS_1254,
        encoding_rs::WINDOWS_1257,
        encoding_rs::WINDOWS_1258,
    ]
    .contains(&e)
}

/// Counts valid UTF-8 sequences of two to four bytes, left to right without overlap, and their total length.
fn utf8_sequences(raw: &[u8]) -> (usize, usize) {
    let (mut i, mut count, mut len) = (0, 0, 0);
    let cont = |b: Option<&u8>| b.is_some_and(|b| (0x80..=0xBF).contains(b));
    while i < raw.len() {
        let b = raw[i];
        let need = match b {
            0xC2..=0xDF => 1,
            0xE0..=0xEF => 2,
            0xF0..=0xF4 => 3,
            _ => 0,
        };
        if need > 0 && (1..=need).all(|k| cont(raw.get(i + k))) {
            count += 1;
            len += need + 1;
            i += need + 1;
        } else {
            i += 1;
        }
    }
    (count, len)
}

/// A short plain reason when `text` (an undeclared legacy decode of `data` with `enc`) looks like the wrong code
/// page, else `None`:
/// * letters of two scripts inside one word (Greek read as Western: "Cafι"); in a CJK file ASCII letters next to
///   ideographs are normal and ignored;
/// * many words made mostly of accented Latin letters (Russian or Greek read as Western); when `enc` is the
///   locale's own code page, a word made only of that language's own accented letters does not count;
/// * a single-byte decode of bytes that are mostly valid UTF-8 sequences (a mixed file).
pub fn text_looks_garbled(text: &str, enc: &'static Encoding, locale: &str, data: &[u8]) -> Option<&'static str> {
    let sample: String = text.chars().take(SAMPLE).collect();
    let mut cjk = is_multibyte(enc);
    let mut native: Vec<char> = Vec::new();
    if !locale.is_empty() {
        cjk = cjk || target_code_page(locale).is_some_and(cjk_family);
        native = native_letters(locale, enc);
    }
    let (mut words, mut mixed, mut heavy) = (0usize, 0usize, 0usize);
    let mut word = String::new();
    let mut check = |word: &str| {
        let nonascii: Vec<char> = word.chars().filter(|c| (*c as u32) > 127).collect();
        if nonascii.is_empty() {
            return;
        }
        words += 1;
        let mut fams: std::collections::HashSet<Script> = if cjk {
            nonascii.iter().map(|c| script(*c)).collect()
        } else {
            word.chars().map(script).collect()
        };
        if cjk && fams.contains(&Script::HwKana) && nonascii.len() < word.chars().count() {
            fams.insert(Script::Latin);
        }
        if fams.len() > 1 {
            mixed += 1;
        } else if fams.len() == 1
            && fams.contains(&Script::Latin)
            && nonascii.len() >= 3
            && nonascii.len() * 10 >= word.chars().count() * 6
            && !nonascii.iter().all(|c| native.contains(c))
        {
            heavy += 1;
        }
    };
    for c in sample.chars() {
        if is_word_char(c) {
            word.push(c);
        } else if !word.is_empty() {
            check(&word);
            word.clear();
        }
    }
    if !word.is_empty() {
        check(&word);
    }
    if words > 0 && mixed > 0 && (mixed * 5 >= words || mixed >= 10) {
        return Some("letters of different scripts mixed inside words");
    }
    if heavy >= 2 && heavy * 10 >= words * 3 {
        return Some("many words made mostly of accented letters");
    }
    if !data.is_empty() && utf8_check(enc) {
        let raw = &data[..data.len().min(SAMPLE * 2)];
        let high = raw.iter().filter(|b| **b > 0x7F).count();
        let (n, len) = utf8_sequences(raw);
        if n >= 3 && len * 2 >= high {
            return Some("parts of the file look like UTF-8");
        }
    }
    None
}

/// `true` when `enc` is a code page of a CJK language.
pub fn is_cjk_code_page(e: &'static Encoding) -> bool {
    code_page_for_encoding(e).is_some_and(cjk_family)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greek_read_as_western_is_garbled() {
        // "Καλημέρα κόσμε" in Windows-1253 read as Windows-1252 gives accented Latin soup
        let (bytes, _, _) = encoding_rs::WINDOWS_1253.encode("Καλημέρα κόσμε, καλησπέρα σας");
        let (text, _) = encoding_rs::WINDOWS_1252.decode_without_bom_handling(&bytes);
        let r = text_looks_garbled(&text, encoding_rs::WINDOWS_1252, "", &bytes);
        assert!(r.is_some());
    }

    #[test]
    fn polish_in_its_own_code_page_is_fine() {
        let (bytes, _, _) = encoding_rs::WINDOWS_1250.encode("Zażółć gęślą jaźń. Łódź i ŁĄKA są w Polsce.");
        let (text, _) = encoding_rs::WINDOWS_1250.decode_without_bom_handling(&bytes);
        assert_eq!(text_looks_garbled(&text, encoding_rs::WINDOWS_1250, "pl", &bytes), None);
    }

    #[test]
    fn plain_japanese_is_fine() {
        let (bytes, _, _) = encoding_rs::SHIFT_JIS.encode("これはテストです。Hello world。");
        let (text, _) = encoding_rs::SHIFT_JIS.decode_without_bom_handling(&bytes);
        assert_eq!(text_looks_garbled(&text, encoding_rs::SHIFT_JIS, "ja", &bytes), None);
    }

    #[test]
    fn utf8_bytes_read_as_a_single_byte_page_are_flagged() {
        let bytes = "café résumé naïve façade".as_bytes();
        let (text, _) = encoding_rs::WINDOWS_1252.decode_without_bom_handling(bytes);
        assert_eq!(
            text_looks_garbled(&text, encoding_rs::WINDOWS_1252, "", bytes),
            Some("parts of the file look like UTF-8")
        );
    }
}

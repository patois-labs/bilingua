//! Windows code pages per language: which legacy encoding an undeclared, non-UTF-8 text file most likely uses
//! when its project's source language is known (Localization OS decision 22, `code_pages.py`).

use encoding_rs::Encoding;

/// Splits a BCP 47 or POSIX tag into (language, script, region): lower, title and upper case.
pub fn split_locale(tag: &str) -> (String, String, String) {
    let mut it = tag.trim().split(['-', '_']);
    let lang = it.next().unwrap_or("").to_ascii_lowercase();
    let (mut script, mut region) = (String::new(), String::new());
    for p in it {
        if p.len() == 4 && p.chars().all(|c| c.is_ascii_alphabetic()) {
            let mut cs = p.chars();
            script = cs.next().map(|c| c.to_ascii_uppercase().to_string()).unwrap_or_default() + &cs.as_str().to_ascii_lowercase();
        } else if (p.len() == 2 && p.chars().all(|c| c.is_ascii_alphabetic())) || (p.len() == 3 && p.chars().all(|c| c.is_ascii_digit())) {
            region = p.to_ascii_uppercase();
        }
    }
    (lang, script, region)
}

fn lang_cp(lang: &str) -> Option<u16> {
    const TABLE: &[(u16, &str)] = &[
        (1250, "cs pl hu hr sk sl ro sq bs hsb dsb"),
        (1251, "ru uk bg be mk sr kk ky tt mn ba sah tg"),
        (
            1252,
            "en fr de es it pt nl da sv nb nn no fi is ca eu gl ga id ms sw af fo fy lb rm oc br co gd cy tl fil zu xh tn st nso rw quz",
        ),
        (1253, "el"),
        (1254, "tr az uz"),
        (1255, "he yi"),
        (1256, "ar fa ur ps ug sd"),
        (1257, "et lv lt"),
        (1258, "vi"),
        (874, "th"),
        (932, "ja"),
        (936, "zh"),
        (949, "ko"),
    ];
    TABLE.iter().find(|(_, langs)| langs.split(' ').any(|l| l == lang)).map(|(cp, _)| *cp)
}

/// The Windows ANSI code page of a language tag, or `None` when it has none we know.
pub fn target_code_page(locale: &str) -> Option<u16> {
    let (lang, script, region) = split_locale(locale);
    if lang.is_empty() {
        return None;
    }
    if lang == "zh" {
        return Some(if script == "Hant" || matches!(region.as_str(), "TW" | "HK" | "MO") { 950 } else { 936 });
    }
    if script == "Cyrl" && matches!(lang.as_str(), "sr" | "bs" | "az" | "uz" | "mn" | "tg") {
        return Some(1251);
    }
    if script == "Latn" && lang == "sr" {
        return Some(1250);
    }
    lang_cp(&lang)
}

/// The decoder for a code page number.
pub fn encoding_for_code_page(cp: u16) -> Option<&'static Encoding> {
    Some(match cp {
        874 => encoding_rs::WINDOWS_874,
        932 => encoding_rs::SHIFT_JIS,
        936 => encoding_rs::GBK,
        949 => encoding_rs::EUC_KR,
        950 => encoding_rs::BIG5,
        1250 => encoding_rs::WINDOWS_1250,
        1251 => encoding_rs::WINDOWS_1251,
        1252 => encoding_rs::WINDOWS_1252,
        1253 => encoding_rs::WINDOWS_1253,
        1254 => encoding_rs::WINDOWS_1254,
        1255 => encoding_rs::WINDOWS_1255,
        1256 => encoding_rs::WINDOWS_1256,
        1257 => encoding_rs::WINDOWS_1257,
        1258 => encoding_rs::WINDOWS_1258,
        _ => return None,
    })
}

/// The code page number of a decoder (GBK is 936; GB18030 has none).
pub fn code_page_for_encoding(e: &'static Encoding) -> Option<u16> {
    [874u16, 932, 936, 949, 950, 1250, 1251, 1252, 1253, 1254, 1255, 1256, 1257, 1258]
        .into_iter()
        .find(|cp| encoding_for_code_page(*cp) == Some(e))
}

/// The legacy decoder for a source language, `None` for Western ones (Windows-1252 is the default anyway).
pub fn source_encoding(locale: &str) -> Option<&'static Encoding> {
    let e = encoding_for_code_page(target_code_page(locale)?)?;
    (e != encoding_rs::WINDOWS_1252).then_some(e)
}

/// A plain name for messages: "Greek (Windows-1253)".
pub fn label(e: &'static Encoding) -> String {
    match code_page_for_encoding(e) {
        Some(874) => "Thai (Windows-874)".into(),
        Some(932) => "Japanese (Shift_JIS)".into(),
        Some(936) => "Chinese Simplified (GBK)".into(),
        Some(949) => "Korean (Windows-949)".into(),
        Some(950) => "Chinese Traditional (Big5)".into(),
        Some(1250) => "Central European (Windows-1250)".into(),
        Some(1251) => "Cyrillic (Windows-1251)".into(),
        Some(1252) => "Western European (Windows-1252)".into(),
        Some(1253) => "Greek (Windows-1253)".into(),
        Some(1254) => "Turkish (Windows-1254)".into(),
        Some(1255) => "Hebrew (Windows-1255)".into(),
        Some(1256) => "Arabic (Windows-1256)".into(),
        Some(1257) => "Baltic (Windows-1257)".into(),
        Some(1258) => "Vietnamese (Windows-1258)".into(),
        _ => e.name().to_string(),
    }
}

/// The accented letters of a Latin-script language's own alphabet (lower and upper case), used so that ordinary
/// Polish or Czech text read with its own code page is not called garbled.
fn native_letters_of(lang: &str) -> &'static str {
    match lang {
        "pl" => "ąćęłńóśźż",
        "cs" => "áčďéěíňóřšťúůýž",
        "sk" => "áäčďéíĺľňóôŕšťúýž",
        "hu" => "áéíóöőúüű",
        "sl" | "hr" | "bs" | "sr" => "čšžćđ",
        "ro" => "ăâîșțşţ",
        "tr" => "çğıöşüâîûİ",
        "lt" => "ąčęėįšųūž",
        "lv" => "āčēģīķļņšūž",
        "et" => "äöõüšž",
        "vi" => "àáâãèéêìíòóôõùúýăđĩũơưạảấầẩẫậắằẳẵặẹẻẽếềểễệỉịọỏốồổỗộớờởỡợụủứừửữựỳỵỷỹ\u{300}\u{301}\u{303}\u{309}\u{323}",
        "de" => "äöüß",
        "fr" => "àâæçéèêëîïôœùûüÿ",
        "es" => "áéíñóúü",
        "pt" => "áâãàçéêíóôõú",
        "it" => "àèéìíîòóù",
        "nl" => "áéëïóöüè",
        "sv" => "åäöé",
        "da" | "no" | "nb" | "nn" => "æøåé",
        "fi" => "äöåšž",
        "is" => "áðéíóúýþæö",
        "ca" => "àçéèíïòóúü·",
        _ => "",
    }
}

/// The native accented letters when `enc` is the locale's own Windows code page, else an empty list.
pub fn native_letters(locale: &str, enc: &'static Encoding) -> Vec<char> {
    let (lang, _, _) = split_locale(locale);
    let letters = native_letters_of(&lang);
    if letters.is_empty() || code_page_for_encoding(enc) != target_code_page(locale) {
        return Vec::new();
    }
    let mut v: Vec<char> = letters.chars().collect();
    let upper: Vec<char> = letters
        .chars()
        .filter_map(|c| {
            let mut u = c.to_uppercase();
            let first = u.next()?;
            u.next().is_none().then_some(first)
        })
        .collect();
    v.extend(upper);
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_map_to_their_code_pages() {
        assert_eq!(target_code_page("ja-JP"), Some(932));
        assert_eq!(target_code_page("ko"), Some(949));
        assert_eq!(target_code_page("zh-CN"), Some(936));
        assert_eq!(target_code_page("zh-Hant"), Some(950));
        assert_eq!(target_code_page("zh_TW"), Some(950));
        assert_eq!(target_code_page("ar-SA"), Some(1256));
        assert_eq!(target_code_page("sr-Latn"), Some(1250));
        assert_eq!(target_code_page("sr-Cyrl"), Some(1251));
        assert_eq!(target_code_page("hi"), None);
        assert!(source_encoding("en").is_none());
        assert_eq!(source_encoding("el").unwrap().name(), "windows-1253");
    }
}

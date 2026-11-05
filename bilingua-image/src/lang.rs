//! The BCP 47 tag and text direction LocOS declares for a locale (`html_lang`, `text_direction`).

use regex::Regex;
use std::sync::OnceLock;

const RTL_SCRIPTS: &[&str] = &["arab", "hebr", "syrc", "thaa", "nkoo", "adlm", "rohg", "samr", "mand", "mend", "yezi"];
const RTL_LANGUAGES: &[&str] = &[
    "ar", "arc", "azb", "ckb", "dv", "fa", "glk", "he", "iw", "ji", "ks", "lrc", "mzn", "nqo", "pnb", "prs", "ps", "sd", "syr", "ug", "ur", "yi",
];

/// The tag to declare, or `None` when the locale is not a usable tag.
pub fn canonical_tag(locale: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"^[A-Za-z]{2,8}(?:-[A-Za-z0-9]{1,8})*$").expect("static pattern"));
    let tag = locale.trim().replace('_', "-");
    if tag.len() > 64 || !re.is_match(&tag) {
        return None;
    }
    let mut parts = tag.split('-');
    let mut out = vec![parts.next()?.to_ascii_lowercase()];
    let mut extension = false;
    for sub in parts {
        let alpha = sub.chars().all(|c| c.is_ascii_alphabetic());
        let digit = sub.chars().all(|c| c.is_ascii_digit());
        if sub.len() == 1 {
            extension = true;
            out.push(sub.to_ascii_lowercase());
        } else if extension {
            out.push(sub.to_ascii_lowercase());
        } else if sub.len() == 4 && alpha {
            let mut c = sub.to_ascii_lowercase();
            c[..1].make_ascii_uppercase();
            out.push(c);
        } else if (sub.len() == 2 && alpha) || (sub.len() == 3 && digit) {
            out.push(sub.to_ascii_uppercase());
        } else {
            out.push(sub.to_ascii_lowercase());
        }
    }
    Some(out.join("-"))
}

/// True for a right-to-left tag: a script subtag decides, else the base language.
pub fn is_rtl(tag: &str) -> bool {
    let parts: Vec<&str> = tag.split('-').collect();
    for sub in &parts[1..] {
        if sub.len() == 1 {
            break;
        }
        if sub.len() == 4 && sub.chars().all(|c| c.is_ascii_alphabetic()) {
            return RTL_SCRIPTS.contains(&sub.to_ascii_lowercase().as_str());
        }
    }
    RTL_LANGUAGES.contains(&parts[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_and_direction() {
        assert_eq!(canonical_tag("es_es").as_deref(), Some("es-ES"));
        assert_eq!(canonical_tag("zh-hant-tw").as_deref(), Some("zh-Hant-TW"));
        assert_eq!(canonical_tag("-"), None);
        assert_eq!(canonical_tag("a<b"), None);
        assert!(is_rtl("ar") && is_rtl("fa-IR") && is_rtl("ku-Arab") && !is_rtl("ug-Latn") && !is_rtl("ja") && !is_rtl("sd-Deva"));
    }
}

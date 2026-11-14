//! Target-language tags as Localization OS writes them, and which OpenDocument language slot a target belongs in.

/// The canonical BCP 47 tag of `code` (`es_es` gives `es-ES`), or `None` when it is not a usable tag (blank, a
/// dash placeholder, quotes, spaces). LocOS then leaves the file's own language alone.
pub fn html_lang(code: &str) -> Option<String> {
    let tag = code.trim().replace('_', "-");
    if tag.is_empty() || tag.len() > 64 {
        return None;
    }
    let mut parts = tag.split('-');
    let first = parts.next()?;
    if !(2..=8).contains(&first.len()) || !first.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let mut out = vec![first.to_ascii_lowercase()];
    let mut extension = false;
    for sub in parts {
        if sub.is_empty() || sub.len() > 8 || !sub.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return None;
        }
        if sub.len() == 1 {
            extension = true;
            out.push(sub.to_ascii_lowercase());
        } else if extension {
            out.push(sub.to_ascii_lowercase());
        } else if sub.len() == 4 && sub.bytes().all(|b| b.is_ascii_alphabetic()) {
            let mut s = sub.to_ascii_lowercase();
            s[..1].make_ascii_uppercase();
            out.push(s);
        } else if (sub.len() == 2 && sub.bytes().all(|b| b.is_ascii_alphabetic())) || (sub.len() == 3 && sub.bytes().all(|b| b.is_ascii_digit())) {
            out.push(sub.to_ascii_uppercase());
        } else {
            out.push(sub.to_ascii_lowercase());
        }
    }
    Some(out.join("-"))
}

const RTL_SCRIPTS: [&str; 11] = ["arab", "hebr", "syrc", "thaa", "nkoo", "adlm", "rohg", "samr", "mand", "mend", "yezi"];
const RTL_LANGUAGES: [&str; 22] = [
    "ar", "arc", "azb", "ckb", "dv", "fa", "glk", "he", "iw", "ji", "ks", "lrc", "mzn", "nqo", "pnb", "prs", "ps", "sd", "syr", "ug", "ur", "yi",
];
const EAST_ASIAN: [&str; 8] = ["zh", "ja", "ko", "yue", "wuu", "hak", "nan", "cmn"];
const COMPLEX: [&str; 25] = [
    "th", "lo", "km", "my", "hi", "mr", "ne", "sa", "bn", "as", "pa", "gu", "or", "ta", "te", "kn", "ml", "si", "bo", "dz", "kok", "mai", "bho", "doi", "sd",
];

/// True for a right-to-left target (a script subtag decides, else the base language).
pub fn is_rtl(tag: &str) -> bool {
    let parts: Vec<&str> = tag.split('-').collect();
    for sub in &parts[1..] {
        if sub.len() == 1 {
            break;
        }
        if sub.len() == 4 && sub.bytes().all(|b| b.is_ascii_alphabetic()) {
            return RTL_SCRIPTS.contains(&sub.to_ascii_lowercase().as_str());
        }
    }
    RTL_LANGUAGES.contains(&parts[0])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Latin,
    EastAsia,
    Complex,
}

/// Which language attribute a target is written in: East Asian targets in the `-asian` slot; right-to-left and
/// other complex-script targets (Thai, Hindi, ...) in the `-complex` slot, unless romanised (`hi-Latn`).
pub fn slot(tag: &str) -> Slot {
    if tag.is_empty() {
        return Slot::Latin;
    }
    if is_rtl(tag) {
        return Slot::Complex;
    }
    let subs: Vec<String> = tag.split('-').map(str::to_ascii_lowercase).collect();
    if COMPLEX.contains(&subs[0].as_str()) && !subs[1..].iter().any(|s| s == "latn") {
        return Slot::Complex;
    }
    if EAST_ASIAN.contains(&subs[0].as_str()) {
        return Slot::EastAsia;
    }
    Slot::Latin
}

/// Language, country and script values for a tag (country upper case, script title case).
pub fn parts(tag: &str) -> (String, Option<String>, Option<String>) {
    let subs: Vec<&str> = tag.split('-').collect();
    let lang = subs[0].to_ascii_lowercase();
    let script = subs[1..].iter().find(|s| s.len() == 4 && s.bytes().all(|b| b.is_ascii_alphabetic())).map(|s| {
        let mut x = s.to_ascii_lowercase();
        x[..1].make_ascii_uppercase();
        x
    });
    let country = subs[1..]
        .iter()
        .find(|s| (s.len() == 2 && s.bytes().all(|b| b.is_ascii_alphabetic())) || (s.len() == 3 && s.bytes().all(|b| b.is_ascii_digit())))
        .map(|s| s.to_ascii_uppercase());
    (lang, country, script)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_and_slots() {
        assert_eq!(html_lang("de_de").as_deref(), Some("de-DE"));
        assert_eq!(html_lang("zh-hant-tw").as_deref(), Some("zh-Hant-TW"));
        assert_eq!(html_lang("-"), None);
        assert_eq!(html_lang("a b"), None);
        assert_eq!(html_lang(""), None);
        assert_eq!(slot("ja"), Slot::EastAsia);
        assert_eq!(slot("ko-KR"), Slot::EastAsia);
        assert_eq!(slot("th-TH"), Slot::Complex);
        assert_eq!(slot("hi-IN"), Slot::Complex);
        assert_eq!(slot("hi-Latn"), Slot::Latin);
        assert_eq!(slot("ar"), Slot::Complex);
        assert_eq!(slot("he-IL"), Slot::Complex);
        assert_eq!(slot("de"), Slot::Latin);
        assert!(is_rtl("fa-IR") && is_rtl("ku-Arab") && !is_rtl("ku") && !is_rtl("sd-Deva") && !is_rtl("ug-Latn"));
        assert_eq!(parts("zh-Hant-TW"), ("zh".into(), Some("TW".into()), Some("Hant".into())));
    }
}

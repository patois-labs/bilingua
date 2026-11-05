//! Locale knowledge: is a root key a locale code (Rails files), and which CLDR plural categories does a target
//! language use. The tables in `locale_data` come from Babel's CLDR data, like LocOS.

use crate::locale_data::{LANGUAGES, PLURALS, SCRIPTS, TERRITORIES};

const RAILS_3_LETTER: &[&str] = &["ast", "fil", "gsw", "haw", "ckb", "dsb", "hsb", "nds", "sah", "yue"];
pub const PLURAL_ORDER: [&str; 6] = ["zero", "one", "two", "few", "many", "other"];

/// `^[A-Za-z]{2,3}(?:[-_][A-Za-z0-9]{2,8})*$`
fn locale_shape(k: &str) -> bool {
    let mut parts = k.split(['-', '_']);
    let Some(first) = parts.next() else { return false };
    if !(2..=3).contains(&first.len()) || !first.bytes().all(|b| b.is_ascii_alphabetic()) {
        return false;
    }
    parts.all(|p| (2..=8).contains(&p.len()) && p.bytes().all(|b| b.is_ascii_alphanumeric()))
}

fn has(list: &[&str], x: &str) -> bool {
    list.binary_search(&x).is_ok()
}

/// Rails-style root key: `en`, `pt-BR`, `zh_CN`. A bare 3-letter key counts only when Rails ships it.
pub fn looks_locale(key: &str) -> bool {
    if !locale_shape(key) || key.eq_ignore_ascii_case("root") || key.eq_ignore_ascii_case("und") {
        return false;
    }
    let mut parts = key.split(['-', '_']);
    let primary = parts.next().unwrap_or("");
    if primary.len() == 3 && !RAILS_3_LETTER.contains(&primary.to_ascii_lowercase().as_str()) {
        return false;
    }
    if !has(LANGUAGES, &primary.to_ascii_lowercase()) {
        return false;
    }
    for p in parts {
        let script = {
            let mut c = p.chars();
            c.next().map(|f| f.to_ascii_uppercase().to_string() + &c.as_str().to_ascii_lowercase()).unwrap_or_default()
        };
        let ok = (p.len() == 4 && has(SCRIPTS, &script)) || ((p.len() == 2 || p.len() == 3) && has(TERRITORIES, &p.to_ascii_uppercase())) || (5..=8).contains(&p.len());
        if !ok {
            return false;
        }
    }
    true
}

fn letter_cats(code: &str) -> &'static str {
    let key = code.trim().replace('-', "_");
    let find = |k: &str| PLURALS.binary_search_by(|(a, _)| (*a).cmp(k)).ok().map(|i| PLURALS[i].1);
    if let Some(c) = find(&key) {
        return c;
    }
    let base = key.split('_').next().unwrap_or("");
    find(base).unwrap_or("")
}

/// The CLDR plural categories of a language in CLDR order, always with `other`. None when unknown.
pub fn target_categories(locale: &str) -> Option<Vec<&'static str>> {
    let letters = letter_cats(locale);
    if letters.is_empty() {
        return None;
    }
    Some(
        PLURAL_ORDER
            .iter()
            .zip(["z", "o", "t", "f", "m", "x"])
            .filter(|(_, l)| letters.contains(l))
            .map(|(n, _)| *n)
            .collect(),
    )
}

/// The target's categories absent from `have` (CLDR order). Empty when `have` has no `other`.
pub fn missing_categories(have: &[String], locale: &str) -> Vec<&'static str> {
    let have: Vec<String> = have.iter().map(|h| h.to_ascii_lowercase()).collect();
    if !have.iter().any(|h| h == "other") {
        return vec![];
    }
    target_categories(locale).unwrap_or_default().into_iter().filter(|c| !have.iter().any(|h| h == c)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locales() {
        for k in ["en", "pt-BR", "zh_CN", "fil", "fil-PH", "ast", "de", "ja", "ar", "zh-Hant-TW"] {
            assert!(looks_locale(k), "{k}");
        }
        for k in ["nav", "spa", "app", "dev", "root", "und", "x", "hello_world", "en-XYZZY12345"] {
            assert!(!looks_locale(k), "{k}");
        }
    }

    #[test]
    fn plurals() {
        assert_eq!(target_categories("pl-PL").unwrap(), vec!["one", "few", "many", "other"]);
        assert_eq!(target_categories("ar").unwrap(), vec!["zero", "one", "two", "few", "many", "other"]);
        assert_eq!(target_categories("ja").unwrap(), vec!["other"]);
        assert!(target_categories("xx").is_none());
        assert_eq!(missing_categories(&["one".into(), "other".into()], "pl"), vec!["few", "many"]);
    }
}

//! Language helpers the localized export needs: canonical tag, direction, Windows LCID (as LocOS).

use crate::lcid_table::LCIDS;

/// The canonical BCP 47 tag, or None for something that is not a well-formed tag.
pub fn canon(code: &str) -> Option<String> {
    let t = code.trim().replace('_', "-");
    if t.is_empty() || t.len() > 64 {
        return None;
    }
    let parts: Vec<&str> = t.split('-').collect();
    let l = parts[0];
    if !(2..=8).contains(&l.len()) || !l.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let mut out = vec![l.to_ascii_lowercase()];
    let mut ext = false;
    for s in &parts[1..] {
        if s.is_empty() || s.len() > 8 || !s.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return None;
        }
        if s.len() == 1 {
            ext = true;
            out.push(s.to_ascii_lowercase());
        } else if ext {
            out.push(s.to_ascii_lowercase());
        } else if s.len() == 4 && s.bytes().all(|b| b.is_ascii_alphabetic()) {
            let mut c = s.to_ascii_lowercase();
            c[..1].make_ascii_uppercase();
            out.push(c);
        } else if (s.len() == 2 && s.bytes().all(|b| b.is_ascii_alphabetic())) || (s.len() == 3 && s.bytes().all(|b| b.is_ascii_digit())) {
            out.push(s.to_ascii_uppercase());
        } else {
            out.push(s.to_ascii_lowercase());
        }
    }
    Some(out.join("-"))
}

const RTL_SCRIPTS: &[&str] = &["arab", "hebr", "syrc", "thaa", "nkoo", "adlm", "rohg", "samr", "mand", "mend", "yezi"];
const RTL_LANGS: &[&str] = &[
    "ar", "arc", "azb", "ckb", "dv", "fa", "glk", "he", "iw", "ji", "ks", "lrc", "mzn", "nqo", "pnb", "prs", "ps", "sd", "syr", "ug", "ur", "yi",
];

/// True for a right-to-left target (script subtag first, else the base language).
pub fn is_rtl(code: &str) -> bool {
    let Some(tag) = canon(code) else { return false };
    let parts: Vec<&str> = tag.split('-').collect();
    for s in &parts[1..] {
        if s.len() == 1 {
            break;
        }
        if s.len() == 4 && s.bytes().all(|b| b.is_ascii_alphabetic()) {
            return RTL_SCRIPTS.contains(&s.to_ascii_lowercase().as_str());
        }
    }
    RTL_LANGS.contains(&parts[0])
}

/// The Windows LCID of a target, as LocOS `_lcid`.
pub fn lcid(code: &str) -> Option<u32> {
    let tag = canon(code)?;
    let parts: Vec<&str> = tag.split('-').collect();
    let lang = parts[0].to_ascii_lowercase();
    let mut region = parts[1..].iter().find(|p| p.len() == 2 && p.bytes().all(|b| b.is_ascii_alphabetic())).map(|p| p.to_ascii_uppercase()).unwrap_or_default();
    let script = parts[1..].iter().find(|p| p.len() == 4 && p.bytes().all(|b| b.is_ascii_alphabetic())).map(|p| p.to_string());
    if lang == "zh" && region.is_empty() {
        region = if script.as_deref() == Some("Hant") { "TW".into() } else { "CN".into() };
    }
    let mut table: Vec<(u32, &str)> = LCIDS.to_vec();
    table.sort();
    if !region.is_empty() {
        let want = format!("{lang}_{region}");
        if let Some((k, _)) = table.iter().find(|(_, v)| *v == want) {
            return Some(*k);
        }
    }
    let same: Vec<u32> = table.iter().filter(|(_, v)| v.split('_').next() == Some(lang.as_str())).map(|(k, _)| *k).collect();
    same.iter().copied().find(|k| (k >> 10) == 1).or_else(|| same.first().copied())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lcids() {
        assert_eq!(lcid("de-DE"), Some(1031));
        assert_eq!(lcid("de"), Some(1031));
        assert_eq!(lcid("ja"), Some(1041));
        assert_eq!(lcid("zh-Hant"), Some(1028));
        assert_eq!(lcid("ar"), Some(1025));
        assert_eq!(lcid("xx"), None);
        assert!(is_rtl("he-IL") && is_rtl("ar-SA") && is_rtl("fa") && !is_rtl("de-DE") && !is_rtl("sd-Deva"));
    }
}

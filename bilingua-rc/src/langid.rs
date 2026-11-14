//! A locale tag to a Windows LANGID, as LocOS does it (`html_lang` canonical tag, then Python's
//! `locale.windows_locale` table).

use crate::winlocale::WINDOWS_LOCALE;

/// The canonical BCP 47 tag LocOS declares for a locale code, or `None` when it is not a usable tag.
pub fn target_tag(code: &str) -> Option<String> {
    let text = code.trim();
    if text.is_empty() {
        return None;
    }
    let tag = text.replace('_', "-");
    if tag.len() > 64 {
        return None;
    }
    let mut parts = tag.split('-');
    let first = parts.next()?;
    if !(2..=8).contains(&first.len()) || !first.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let rest: Vec<&str> = parts.collect();
    if rest.iter().any(|p| p.is_empty() || p.len() > 8 || !p.bytes().all(|b| b.is_ascii_alphanumeric())) {
        return None;
    }
    let mut out = vec![first.to_ascii_lowercase()];
    let mut extension = false;
    for sub in rest {
        let alpha = sub.bytes().all(|b| b.is_ascii_alphabetic());
        if sub.len() == 1 {
            extension = true;
            out.push(sub.to_ascii_lowercase());
        } else if extension {
            out.push(sub.to_ascii_lowercase());
        } else if sub.len() == 4 && alpha {
            let mut s = sub.to_ascii_lowercase();
            s[..1].make_ascii_uppercase();
            out.push(s);
        } else if (sub.len() == 2 && alpha) || (sub.len() == 3 && sub.bytes().all(|b| b.is_ascii_digit())) {
            out.push(sub.to_ascii_uppercase());
        } else {
            out.push(sub.to_ascii_lowercase());
        }
    }
    Some(out.join("-"))
}

/// The Windows LANGID of a locale, or `None` when the code is unusable or the language is not in the table.
pub fn langid(locale: &str) -> Option<u32> {
    let tag = target_tag(locale)?;
    let parts: Vec<&str> = tag.split('-').collect();
    let lang = parts[0].to_ascii_lowercase();
    let mut region = parts[1..].iter().find(|p| p.len() == 2 && p.bytes().all(|b| b.is_ascii_alphabetic())).map(|p| p.to_ascii_uppercase());
    let script = parts[1..].iter().find(|p| p.len() == 4 && p.bytes().all(|b| b.is_ascii_alphabetic())).map(|p| {
        let mut s = p.to_ascii_lowercase();
        s[..1].make_ascii_uppercase();
        s
    });
    if lang == "zh" && region.is_none() {
        region = Some(if script.as_deref() == Some("Hant") { "TW" } else { "CN" }.to_string());
    }
    if let Some(r) = region {
        let want = format!("{lang}_{r}");
        if let Some((k, _)) = WINDOWS_LOCALE.iter().find(|(_, v)| *v == want) {
            return Some(*k);
        }
    }
    let same: Vec<u32> = WINDOWS_LOCALE.iter().filter(|(_, v)| v.split('_').next() == Some(lang.as_str())).map(|(k, _)| *k).collect();
    same.iter().copied().find(|k| k >> 10 == 1).or_else(|| same.first().copied())
}

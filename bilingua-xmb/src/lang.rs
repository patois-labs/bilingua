//! The BCP 47 tag written to `lang`: underscores become hyphens and case is canonical (language lower, Script
//! title, REGION upper), as Localization OS writes it. Anything that is not a well-formed tag gives `None`.

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
    let rest: Vec<&str> = parts.collect();
    if rest.iter().any(|p| p.is_empty() || p.len() > 8 || !p.bytes().all(|b| b.is_ascii_alphanumeric())) {
        return None;
    }
    let mut out = vec![first.to_ascii_lowercase()];
    let mut extension = false;
    for sub in rest {
        let alpha = sub.bytes().all(|b| b.is_ascii_alphabetic());
        let digit = sub.bytes().all(|b| b.is_ascii_digit());
        if sub.len() == 1 {
            extension = true;
            out.push(sub.to_ascii_lowercase());
        } else if extension {
            out.push(sub.to_ascii_lowercase());
        } else if sub.len() == 4 && alpha {
            let mut s = sub.to_ascii_lowercase();
            s[..1].make_ascii_uppercase();
            out.push(s);
        } else if (sub.len() == 2 && alpha) || (sub.len() == 3 && digit) {
            out.push(sub.to_ascii_uppercase());
        } else {
            out.push(sub.to_ascii_lowercase());
        }
    }
    Some(out.join("-"))
}

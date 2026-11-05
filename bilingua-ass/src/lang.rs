//! The BCP 47 tag LocOS declares for a locale (`html_lang`), used for the `Language:` header.

/// The canonical BCP 47 tag for a stored locale code, or None when it is not usable.
pub fn html_lang(code: &str) -> Option<String> {
    let t = code.trim().replace('_', "-");
    if t.is_empty() || t.len() > 64 {
        return None;
    }
    let parts: Vec<&str> = t.split('-').collect();
    let first = parts[0];
    if !(2..=8).contains(&first.len()) || !first.bytes().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    for sub in &parts[1..] {
        if sub.is_empty() || sub.len() > 8 || !sub.bytes().all(|c| c.is_ascii_alphanumeric()) {
            return None;
        }
    }
    let mut out = vec![first.to_ascii_lowercase()];
    let mut extension = false;
    for sub in &parts[1..] {
        let alpha = sub.bytes().all(|c| c.is_ascii_alphabetic());
        if sub.len() == 1 {
            extension = true;
            out.push(sub.to_ascii_lowercase());
        } else if extension {
            out.push(sub.to_ascii_lowercase());
        } else if sub.len() == 4 && alpha {
            let l = sub.to_ascii_lowercase();
            out.push(format!("{}{}", l[..1].to_ascii_uppercase(), &l[1..]));
        } else if (sub.len() == 2 && alpha) || (sub.len() == 3 && sub.bytes().all(|c| c.is_ascii_digit())) {
            out.push(sub.to_ascii_uppercase());
        } else {
            out.push(sub.to_ascii_lowercase());
        }
    }
    Some(out.join("-"))
}

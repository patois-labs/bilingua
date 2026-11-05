//! Language tags as LocOS writes them into `<TS language="..">`: `html_lang` (BCP 47 casing, `-` for `_`) and then
//! `_` between the parts, which is how Qt spells a locale (`de_DE`, `zh_Hans`).

/// The canonical tag of `code` with `-` separators, or `None` when it is not a well-formed BCP 47 tag (blank,
/// placeholder, quotes, spaces): LocOS then leaves the file's own language alone.
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

/// The Qt spelling of a target language: `de-DE` becomes `de_DE`.
pub fn qt_language(code: &str) -> Option<String> {
    html_lang(code).map(|t| t.replace('-', "_"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags() {
        assert_eq!(qt_language("de-de").as_deref(), Some("de_DE"));
        assert_eq!(qt_language("zh_hans").as_deref(), Some("zh_Hans"));
        assert_eq!(qt_language("es-419").as_deref(), Some("es_419"));
        assert_eq!(qt_language("pl").as_deref(), Some("pl"));
        assert_eq!(qt_language("x"), None);
        assert_eq!(qt_language("a b"), None);
        assert_eq!(qt_language("*"), None);
        assert_eq!(qt_language(""), None);
    }
}

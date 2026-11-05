//! Language code helpers.

/// Primary language codes the multilingual element layout accepts as element names (`<en>`, `<fr>`): the codes of
/// the Localization OS language catalogue.
pub(crate) const KNOWN: &str = "af am ar az be bg bn bs ca ceb ckb cs cy da de dv el en eo es et eu fa fi fil fr ga gl gu ha haw he hi hr hu hy id ig is it ja jv ka kk km kn ko ku ky la lo lt lv mi mk ml mn mr ms mt my nb ne nl nn no or pa pl ps pt ro ru sd si sk sl sm so sq sr su sv sw ta te tg th tl tr tt ug uk ur uz vi xh yi yo zh zu";

pub(crate) fn is_known_primary(p: &str) -> bool {
    KNOWN.split(' ').any(|k| k == p)
}

/// `^[A-Za-z]{2,3}(?:[-_][A-Za-z0-9]{1,8}){0,3}$`
pub(crate) fn lang_code_ok(s: &str) -> bool {
    let mut parts = s.split(['-', '_']);
    let Some(first) = parts.next() else { return false };
    if !(2..=3).contains(&first.len()) || !first.bytes().all(|b| b.is_ascii_alphabetic()) {
        return false;
    }
    // the separators must be exactly one character each: re-walk
    let rest = &s[first.len()..];
    let mut n = 0;
    let mut r = rest;
    while !r.is_empty() {
        let b = r.as_bytes()[0];
        if b != b'-' && b != b'_' {
            return false;
        }
        let body = &r[1..];
        let end = body.find(['-', '_']).unwrap_or(body.len());
        let seg = &body[..end];
        if seg.is_empty() || seg.len() > 8 || !seg.bytes().all(|c| c.is_ascii_alphanumeric()) {
            return false;
        }
        n += 1;
        if n > 3 {
            return false;
        }
        r = &body[end..];
    }
    true
}

pub(crate) fn norm(code: &str) -> String {
    code.trim().to_lowercase().replace('_', "-")
}

/// The BCP 47 form of a locale (language lower case, script title case, region upper case), or `None` when it is
/// not a well-formed tag. Localization OS `html_lang`.
pub(crate) fn html_lang(code: &str) -> Option<String> {
    let tag = code.trim().replace('_', "-");
    if tag.is_empty() || tag.len() > 64 {
        return None;
    }
    let parts: Vec<&str> = tag.split('-').collect();
    let first = parts[0];
    if !(2..=8).contains(&first.len()) || !first.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    if parts[1..].iter().any(|p| p.is_empty() || p.len() > 8 || !p.bytes().all(|b| b.is_ascii_alphanumeric())) {
        return None;
    }
    let mut out = vec![first.to_lowercase()];
    let mut extension = false;
    for sub in &parts[1..] {
        if sub.len() == 1 {
            extension = true;
            out.push(sub.to_lowercase());
        } else if extension {
            out.push(sub.to_lowercase());
        } else if sub.len() == 4 && sub.bytes().all(|b| b.is_ascii_alphabetic()) {
            let mut c = sub.chars();
            let f = c.next().map(|x| x.to_ascii_uppercase()).unwrap_or(' ');
            out.push(format!("{f}{}", c.as_str().to_lowercase()));
        } else if (sub.len() == 2 && sub.bytes().all(|b| b.is_ascii_alphabetic())) || (sub.len() == 3 && sub.bytes().all(|b| b.is_ascii_digit())) {
            out.push(sub.to_uppercase());
        } else {
            out.push(sub.to_lowercase());
        }
    }
    Some(out.join("-"))
}

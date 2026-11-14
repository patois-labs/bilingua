//! The target language in an HTML part: `lang` (and `xml:lang`, `dir`) on the `<html>` element and a
//! `<meta http-equiv="content-language">`, as LocOS declares it. Language tags are canonicalised
//! (`de_de` becomes `de-DE`).

use regex::Regex;

/// A canonical BCP 47 tag (language lower case, script title case, region upper case), or `None` when unusable.
pub fn canonical_tag(lang: &str) -> Option<String> {
    let t = lang.trim().replace('_', "-");
    if t.is_empty() || t.len() > 64 {
        return None;
    }
    let mut parts = t.split('-');
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
        let alpha = sub.bytes().all(|b| b.is_ascii_alphabetic());
        if sub.len() == 1 {
            extension = true;
            out.push(sub.to_ascii_lowercase());
        } else if extension {
            out.push(sub.to_ascii_lowercase());
        } else if sub.len() == 4 && alpha {
            let mut c = sub.to_ascii_lowercase();
            c[..1].make_ascii_uppercase();
            out.push(c);
        } else if (sub.len() == 2 && alpha) || (sub.len() == 3 && sub.bytes().all(|b| b.is_ascii_digit())) {
            out.push(sub.to_ascii_uppercase());
        } else {
            out.push(sub.to_ascii_lowercase());
        }
    }
    Some(out.join("-"))
}

/// "rtl" or "ltr" for a canonical tag.
pub fn direction(tag: &str) -> &'static str {
    const RTL_SCRIPTS: [&str; 11] = ["arab", "hebr", "syrc", "thaa", "nkoo", "adlm", "rohg", "samr", "mand", "mend", "yezi"];
    const RTL_LANGS: [&str; 22] = [
        "ar", "arc", "azb", "ckb", "dv", "fa", "glk", "he", "iw", "ji", "ks", "lrc", "mzn", "nqo", "pnb", "prs", "ps", "sd", "syr", "ug", "ur", "yi",
    ];
    let parts: Vec<&str> = tag.split('-').collect();
    for sub in &parts[1..] {
        if sub.len() == 1 {
            break;
        }
        if sub.len() == 4 && sub.bytes().all(|b| b.is_ascii_alphabetic()) {
            return if RTL_SCRIPTS.contains(&sub.to_ascii_lowercase().as_str()) { "rtl" } else { "ltr" };
        }
    }
    if RTL_LANGS.contains(&parts[0]) {
        "rtl"
    } else {
        "ltr"
    }
}

fn set_attr(tag: &str, attr: &str, value: &str, add_if_missing: bool) -> String {
    let pat = Regex::new(&format!(r#"(?i)(\s{}\s*=\s*)("[^"]*"|'[^']*'|[^\s>"']+)"#, regex::escape(attr))).unwrap();
    if let Some(m) = pat.captures(tag) {
        let whole = m.get(0).unwrap();
        return format!("{}{}\"{}\"{}", &tag[..whole.start()], &m[1], value, &tag[whole.end()..]);
    }
    if !add_if_missing {
        return tag.to_string();
    }
    let name_end = tag
        .char_indices()
        .skip(1)
        .find(|(_, c)| matches!(c, '\t' | '\n' | '\u{0c}' | '\r' | ' ' | '/' | '>'))
        .map_or(tag.len(), |(i, _)| i);
    format!("{} {attr}=\"{value}\"{}", &tag[..name_end], &tag[name_end..])
}

/// End of the start tag that begins at `i` (after the closing `>`), honouring quoted attribute values.
fn tag_end(s: &str, i: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut j = i + 1;
    let mut quote: Option<u8> = None;
    while j < b.len() {
        match (quote, b[j]) {
            (Some(q), c) if c == q => quote = None,
            (None, b'"' | b'\'') => quote = Some(b[j]),
            (None, b'>') => return Some(j + 1),
            _ => {}
        }
        j += 1;
    }
    None
}

/// Position of `needle` (lower case) in `hay` from `from`, ignoring ASCII case.
fn find_ci(hay: &str, from: usize, needle: &str) -> Option<usize> {
    let (h, n) = (hay.as_bytes(), needle.as_bytes());
    if h.len() < n.len() || from > h.len() - n.len() {
        return None;
    }
    (from..=h.len() - n.len()).find(|&i| h[i..i + n.len()].eq_ignore_ascii_case(n))
}

/// Points the page's language declaration at `tag`. A fragment with no `<html>` element gets none.
pub fn declare_language(html: &str, tag: &str) -> String {
    let dir = direction(tag);
    let xml_lang = Regex::new(r"(?i)\sxml:lang\s*=").unwrap();
    let http_equiv = Regex::new(r#"(?i)\shttp-equiv\s*=\s*["']?\s*content-language\s*["']?[\s/>]"#).unwrap();
    let mut out = String::with_capacity(html.len() + 32);
    let mut i = 0;
    let mut last = 0;
    while let Some(p) = html[i..].find('<').map(|p| p + i) {
        i = p + 1;
        let rest = &html[p..];
        if rest.starts_with("<!--") {
            match html[p + 4..].find("-->") {
                Some(e) => i = p + 4 + e + 3,
                None => break,
            }
            continue;
        }
        let Some(c) = rest.as_bytes().get(1) else { break };
        if !c.is_ascii_alphabetic() {
            continue;
        }
        let Some(end) = tag_end(html, p) else { break };
        let head = &rest[..end - p];
        let name_end = head.find(['\t', '\n', '\u{0c}', '\r', ' ', '/', '>']).unwrap_or(head.len());
        let name = head[1..name_end].to_ascii_lowercase();
        let tok = &html[p..end];
        match name.as_str() {
            "html" => {
                let mut t = set_attr(tok, "lang", tag, true);
                if xml_lang.is_match(&t) {
                    t = set_attr(&t, "xml:lang", tag, true);
                }
                if dir == "rtl" {
                    t = set_attr(&t, "dir", "rtl", true);
                } else {
                    t = set_attr(&t, "dir", "ltr", false);
                }
                out.push_str(&html[last..p]);
                out.push_str(&t);
                last = end;
            }
            "meta" if http_equiv.is_match(tok) => {
                out.push_str(&html[last..p]);
                out.push_str(&set_attr(tok, "content", tag, false));
                last = end;
            }
            "script" | "style" => {
                match find_ci(html, end, &format!("</{name}")) {
                    Some(e) => i = e,
                    None => break,
                }
                continue;
            }
            _ => {}
        }
        i = end.max(i);
    }
    out.push_str(&html[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_and_direction() {
        assert_eq!(canonical_tag("de_de").unwrap(), "de-DE");
        assert_eq!(canonical_tag("zh-hant-tw").unwrap(), "zh-Hant-TW");
        assert!(canonical_tag("a b").is_none());
        assert_eq!(direction("ar-SA"), "rtl");
        assert_eq!(direction("ku-Arab"), "rtl");
        assert_eq!(direction("de-DE"), "ltr");
    }

    #[test]
    fn declares_language() {
        let h = "<!-- <html> --><html><head><meta http-equiv=\"content-language\" content=\"en\"></head><body>x</body></html>";
        let o = declare_language(h, "de-DE");
        assert!(o.contains("<html lang=\"de-DE\">"), "{o}");
        assert!(o.contains("content=\"de-DE\""));
        assert!(o.starts_with("<!-- <html> -->"));
        let r = declare_language("<html lang=en dir=ltr>", "ar");
        assert_eq!(r, "<html lang=\"ar\" dir=\"rtl\">");
        assert_eq!(declare_language("<p>x</p>", "de"), "<p>x</p>");
    }
}

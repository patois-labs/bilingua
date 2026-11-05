//! The dialect lexers: where the code islands of a server page are. Port of the LocOS `server_page_filter`
//! lexers. Each returns `(start, end, inline)` in document order, or a refusal naming the line.

use bilingua_htmlcore::pyx::*;

/// A refusal: what is wrong and where (byte offset in the text the lexer read).
#[derive(Debug)]
pub(crate) struct Refusal {
    pub pos: usize,
    pub what: String,
}

pub(crate) type Island = (usize, usize, bool);

fn refuse<T>(pos: usize, what: &str) -> Result<T, Refusal> {
    Err(Refusal { pos, what: what.to_string() })
}

/// `pos` at an opening quote: the index after the closing one, honouring backslashes; `None` when never closed.
pub(crate) fn quote_end(b: &[u8], pos: usize) -> Option<usize> {
    let q = b[pos];
    let mut i = pos + 1;
    loop {
        let j = b[i.min(b.len())..].iter().position(|&c| c == q)? + i;
        let mut k = j - 1;
        while k > pos && b[k] == b'\\' {
            k -= 1;
        }
        if (j - 1 - k).is_multiple_of(2) {
            return Some(j + 1);
        }
        i = j + 1;
    }
}

// --- PHP ----------------------------------------------------------------------------------------------------

/// `<\?(?:(=)|php(?=\s|$)|(?=\s))` (case-insensitive): (start, end of the opener, is `<?=`).
fn php_open(text: &str, from: usize) -> Option<(usize, usize, bool)> {
    let b = text.as_bytes();
    let mut i = from;
    while let Some(p) = find_bytes(b, i, b"<?") {
        let a = p + 2;
        if b.get(a) == Some(&b'=') {
            return Some((p, a + 1, true));
        }
        if b.len() >= a + 3 && b[a..a + 3].eq_ignore_ascii_case(b"php") {
            let after = &text[a + 3..];
            if after.is_empty() || after.chars().next().is_some_and(is_space) {
                return Some((p, a + 3, false));
            }
        }
        if text[a..].chars().next().is_some_and(is_space) {
            return Some((p, a, false));
        }
        i = p + 1;
    }
    None
}

/// The next piece of PHP code syntax at or after `pos`: `?>`, a quote, `/*`, `//`, `#` (not `#[`) or `<<<`.
fn php_code(b: &[u8], mut pos: usize) -> Option<(usize, usize, u8)> {
    while pos < b.len() {
        match b[pos] {
            b'?' if b.get(pos + 1) == Some(&b'>') => return Some((pos, pos + 2, b'?')),
            c @ (b'\'' | b'"' | b'`') => return Some((pos, pos + 1, c)),
            b'/' if b.get(pos + 1) == Some(&b'*') => return Some((pos, pos + 2, b'*')),
            b'/' if b.get(pos + 1) == Some(&b'/') => return Some((pos, pos + 2, b'/')),
            b'#' if b.get(pos + 1) != Some(&b'[') => return Some((pos, pos + 1, b'#')),
            b'<' if b[pos..].starts_with(b"<<<") => return Some((pos, pos + 3, b'<')),
            _ => pos += 1,
        }
    }
    None
}

/// `<<<[ \t]*(["']?)(LABEL)\1\r?\n` at `pos`: (label, end of the opener line).
fn heredoc_open(b: &[u8], pos: usize) -> Option<(&[u8], usize)> {
    let mut i = pos + 3;
    while matches!(b.get(i), Some(b' ' | b'\t')) {
        i += 1;
    }
    let q = match b.get(i) {
        Some(&c @ (b'"' | b'\'')) => {
            i += 1;
            Some(c)
        }
        _ => None,
    };
    let ls = i;
    if !b.get(i).is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_') {
        return None;
    }
    while b.get(i).is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_') {
        i += 1;
    }
    let label = &b[ls..i];
    if let Some(q) = q {
        if b.get(i) != Some(&q) {
            return None;
        }
        i += 1;
    }
    if b.get(i) == Some(&b'\r') {
        i += 1;
    }
    if b.get(i) != Some(&b'\n') {
        return None;
    }
    Some((label, i + 1))
}

/// The end of a heredoc: `(?m)^[ \t]*LABEL(?![A-Za-z0-9_])` searched from `from` (a line start).
fn heredoc_close(b: &[u8], from: usize, label: &[u8]) -> Option<usize> {
    let mut ls = from;
    loop {
        let mut i = ls;
        while matches!(b.get(i), Some(b' ' | b'\t')) {
            i += 1;
        }
        if b[i.min(b.len())..].starts_with(label) {
            let e = i + label.len();
            if !b.get(e).is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_') {
                return Some(e);
            }
        }
        ls = b[ls..].iter().position(|&c| c == b'\n')? + ls + 1;
    }
}

/// `\s*(?:echo|print)\b[^{};]*;?\s*` (case-insensitive) matching the whole body.
fn php_echo_only(body: &str) -> bool {
    let t = trim_start(body);
    let tb = t.as_bytes();
    let k = if tb.len() >= 4 && tb[..4].eq_ignore_ascii_case(b"echo") {
        4
    } else if tb.len() >= 5 && tb[..5].eq_ignore_ascii_case(b"print") {
        5
    } else {
        return false;
    };
    let rest = &t[k..];
    // \b: the next character must not be a word character
    if rest.chars().next().is_some_and(bilingua_model::py_is_word_char) {
        return false;
    }
    match rest.find(['{', '}', ';']) {
        None => true,
        Some(p) => rest.as_bytes()[p] == b';' && trim(&rest[p + 1..]).is_empty(),
    }
}

pub(crate) fn php_islands(text: &str) -> Result<Vec<Island>, Refusal> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some((start, open_end, eq)) = php_open(text, i) {
        let mut pos = open_end;
        let mut end = b.len(); // a file may end in PHP mode with no closing ?>
        while let Some((cs, ce, kind)) = php_code(b, pos) {
            match kind {
                b'?' => {
                    end = ce;
                    break;
                }
                b'\'' | b'"' | b'`' => match quote_end(b, cs) {
                    Some(e) => pos = e,
                    None => return refuse(cs, "a PHP string is never closed"),
                },
                b'*' => match find_bytes(b, ce, b"*/") {
                    Some(c) => pos = c + 2,
                    None => return refuse(cs, "a PHP comment is never closed"),
                },
                b'/' | b'#' => {
                    let nl = b[ce..].iter().position(|&c| c == b'\n').map(|p| p + ce);
                    let lim = nl.unwrap_or(b.len());
                    // ?> ends PHP mode even inside a one-line comment
                    if let Some(c) = find_bytes(&b[..lim], ce, b"?>") {
                        end = c + 2;
                        break;
                    }
                    pos = lim;
                }
                _ => match heredoc_open(b, cs) {
                    None => pos = ce,
                    Some((label, body)) => match heredoc_close(b, body, label) {
                        Some(e) => pos = e,
                        None => return refuse(cs, "a PHP heredoc is never closed"),
                    },
                },
            }
        }
        let mut inline = eq;
        if !inline {
            let body_end = if text[..end].ends_with("?>") { end - 2 } else { end };
            inline = php_echo_only(&text[open_end..body_end.max(open_end)]);
        }
        out.push((start, end, inline));
        i = end;
    }
    Ok(out)
}

// --- <% %> family -------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Percent {
    Jsp,
    Aspx,
    Erb,
}

fn percent_end(b: &[u8], s: usize, d: Percent) -> Option<usize> {
    if matches!(d, Percent::Jsp | Percent::Aspx) && b[s..].starts_with(b"<%--") {
        return find_bytes(b, s + 4, b"--%>").map(|e| e + 4);
    }
    let mut pos = s + 2;
    loop {
        let e = find_bytes(b, pos, b"%>")?;
        if d == Percent::Erb && b[e - 1] == b'%' && e > s + 2 {
            pos = e + 2; // ERB "%%>" is a literal %> inside the tag
            continue;
        }
        return Some(e + 2);
    }
}

/// `${...}` or `#{...}` at `s`: the index after the matching brace (quotes honoured).
fn el_end(b: &[u8], s: usize) -> Option<usize> {
    let mut depth = 0i64;
    let mut i = s + 1;
    while i < b.len() {
        match b[i] {
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth -= 1;
                i += 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            b'\'' | b'"' => i = quote_end(b, i)?,
            _ => i += 1,
        }
    }
    None
}

/// `<jsp:(scriptlet|declaration|expression)(?=[\s>/])` (case-insensitive) at `s`: the element name as written.
fn jsp_xml_open(text: &str, s: usize) -> Option<(&str, usize)> {
    let b = text.as_bytes();
    if b.len() < s + 5 || !b[s..s + 5].eq_ignore_ascii_case(b"<jsp:") {
        return None;
    }
    for name in ["scriptlet", "declaration", "expression"] {
        let e = s + 5 + name.len();
        if b.len() >= e && b[s + 5..e].eq_ignore_ascii_case(name.as_bytes()) {
            let ok = text[e..].chars().next().is_some_and(|c| is_space(c) || c == '>' || c == '/');
            if ok {
                return Some((&text[s + 5..e], e));
            }
        }
    }
    None
}

/// `</jsp:NAME\s*>` (case-insensitive) searched from `from`: the end of the match.
fn jsp_xml_close(text: &str, from: usize, name: &str) -> Option<usize> {
    let b = text.as_bytes();
    let mut pat = b"</jsp:".to_vec();
    pat.extend_from_slice(name.as_bytes());
    let mut f = from;
    while let Some(p) = find_ci(b, f, &pat) {
        let rest = &text[p + pat.len()..];
        let t = trim_start(rest);
        if t.starts_with('>') {
            return Some(p + pat.len() + (rest.len() - t.len()) + 1);
        }
        f = p + 1;
    }
    None
}

pub(crate) fn percent_islands(text: &str, d: Percent) -> Result<Vec<Island>, Refusal> {
    let b = text.as_bytes();
    let inline_marks: &str = match d {
        Percent::Jsp | Percent::Erb => "=",
        Percent::Aspx => "=:#$",
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        // leftmost opener
        let c = b[i];
        let mut found: Option<(usize, u8)> = None; // (start, kind) kind: 0 "<%", 1 jsp xml, 2 EL
        if c == b'<' {
            if b.get(i + 1) == Some(&b'%') {
                found = Some((i, 0));
            } else if d == Percent::Jsp && jsp_xml_open(text, i).is_some() {
                found = Some((i, 1));
            }
        } else if d == Percent::Jsp && (c == b'$' || c == b'#') && b.get(i + 1) == Some(&b'{') && (i == 0 || b[i - 1] != b'\\') {
            found = Some((i, 2));
        }
        let Some((s, kind)) = found else {
            i += 1;
            continue;
        };
        let (e, inline);
        match kind {
            2 => {
                let Some(x) = el_end(b, s) else { return refuse(s, "a JSP expression ${...} is never closed") };
                e = x;
                inline = true;
            }
            1 => {
                let (name, after) = jsp_xml_open(text, s).expect("checked");
                let Some(x) = jsp_xml_close(text, after, name) else {
                    return refuse(s, &format!("<jsp:{name}> is never closed"));
                };
                e = x;
                inline = name.eq_ignore_ascii_case("expression");
            }
            _ => {
                if d == Percent::Erb && b[s..].starts_with(b"<%%") {
                    e = s + 3;
                    inline = true;
                } else {
                    let Some(x) = percent_end(b, s, d) else { return refuse(s, "a <% code block is never closed with %>") };
                    e = x;
                    let mark = text[s + 2..].chars().next();
                    let mut il = mark.is_some_and(|m| inline_marks.contains(m)) && !b[s..].starts_with(b"<%--");
                    if d == Percent::Erb && b[s..].starts_with(b"<%==") {
                        il = true;
                    }
                    inline = il;
                }
            }
        }
        out.push((s, e, inline));
        i = e;
    }
    Ok(out)
}

// --- SSI ----------------------------------------------------------------------------------------------------

pub(crate) fn ssi_islands(text: &str) -> Result<Vec<Island>, Refusal> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(p) = find_bytes(b, i, b"<!--#") {
        let mut e = p + 5;
        while b.get(e).is_some_and(u8::is_ascii_alphabetic) {
            e += 1;
        }
        if e == p + 5 {
            i = p + 1;
            continue;
        }
        let name = text[p + 5..e].to_ascii_lowercase();
        let Some(close) = find_bytes(b, e, b"-->") else {
            return refuse(p, "a server-side include <!--# is never closed with -->");
        };
        out.push((p, close + 3, matches!(name.as_str(), "echo" | "fsize" | "flastmod")));
        i = close + 3;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn php() {
        let t = "<p>Hi <?= $n ?> <?php if ($a): ?>x<?php echo $b; ?> <?php\n$s = <<<EOT\n?> no\nEOT;\n// ?> yes\n";
        let v = php_islands(t).unwrap();
        let s: Vec<&str> = v.iter().map(|(a, e, _)| &t[*a..*e]).collect();
        assert_eq!(s[0], "<?= $n ?>");
        assert_eq!(s[1], "<?php if ($a): ?>");
        assert_eq!(s[2], "<?php echo $b; ?>");
        assert!(s[3].ends_with("// ?>"));
        assert_eq!(v.iter().map(|x| x.2).collect::<Vec<_>>(), [true, false, true, false]);
    }

    #[test]
    fn percent_and_ssi() {
        let t = "a <%= x %> <%-- c --%> ${a.b} <%%> <jsp:scriptlet>int x;</jsp:scriptlet>";
        let v = percent_islands(t, Percent::Jsp).unwrap();
        assert_eq!(v.len(), 5);
        assert!(percent_islands("<p>Hi <% int x = 1; </p>", Percent::Jsp).is_err());
        let v = ssi_islands("<!--#echo var=\"X\" --> <!--#include file=\"y\" -->").unwrap();
        assert_eq!(v.iter().map(|x| x.2).collect::<Vec<_>>(), [true, false]);
    }
}

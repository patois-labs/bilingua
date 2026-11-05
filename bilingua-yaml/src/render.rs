//! Writing a translated value back into the span of the original scalar, keeping its style when the new text
//! still fits, and quoting only when YAML needs it.

use crate::resolve::resolves_str;
use crate::scan::Style;
use crate::walk::py_space;

fn strip(s: &str) -> &str {
    s.trim_matches(py_space)
}

/// Characters YAML cannot carry in a plain or single-quoted scalar: non-printable ones and line-break lookalikes.
fn unsafe_unquoted(s: &str) -> bool {
    s.chars().any(|c| {
        let o = c as u32;
        let printable = matches!(o, 0x09 | 0x0A | 0x0D | 0x20..=0x7E | 0x85 | 0xA0..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF);
        !printable || matches!(c, '\u{85}' | '\u{2028}' | '\u{2029}' | '\u{feff}' | '\r')
    })
}

pub fn dq(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        let o = c as u32;
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\u{85}' => out.push_str("\\N"),
            '\u{2028}' => out.push_str("\\L"),
            '\u{2029}' => out.push_str("\\P"),
            _ => {
                let printable = matches!(o, 0x20..=0x7E | 0xA0..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF);
                if c == '\u{feff}' || !printable {
                    if o <= 0xFF {
                        out.push_str(&format!("\\x{o:02X}"));
                    } else if o <= 0xFFFF {
                        out.push_str(&format!("\\u{o:04X}"));
                    } else {
                        out.push_str(&format!("\\U{o:08X}"));
                    }
                } else {
                    out.push(c);
                }
            }
        }
    }
    out.push('"');
    out
}

pub fn plain_ok(s: &str, flow: bool) -> bool {
    if s.is_empty() || s != strip(s) || s.contains('\n') || s.contains('\t') || unsafe_unquoted(s) {
        return false;
    }
    let first = s.chars().next().unwrap_or(' ');
    if "-?:,[]{}#&*!|>'\"%@`".contains(first) || s.starts_with("...") {
        return false;
    }
    if s.contains(": ") || s.contains(" #") || s.ends_with(':') {
        return false;
    }
    if flow && s.chars().any(|c| ",[]{}".contains(c)) {
        return false;
    }
    resolves_str(s)
}

fn pieces(rest: &str) -> Vec<&str> {
    rest.split_inclusive('\n').collect()
}

/// A literal/folded block with the source header, indentation and trailing blank lines. None when a block cannot
/// carry `new` (the caller double-quotes).
fn render_block(new: &str, style: Style, region: &str) -> Option<String> {
    let nl_at = region.find('\n')?;
    let header = &region[..=nl_at];
    let head = header.trim_end_matches(['\r', '\n']);
    if head.split('#').next().unwrap_or("").chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    let ps = pieces(&region[nl_at + 1..]);
    let last = ps.iter().rposition(|p| !strip(p).is_empty())?;
    let content_part: String = ps[..=last].concat();
    let tail: String = ps[last + 1..].concat();
    let first = ps.iter().find(|p| !strip(p).is_empty())?;
    let indent = &first[..first.len() - first.trim_start_matches(' ').len()];
    if indent.is_empty() {
        return None;
    }
    let eol = if header.ends_with("\r\n") { "\r\n" } else { "\n" };
    let body = new.trim_end_matches('\n');
    if unsafe_unquoted(body) {
        return None;
    }
    let lines: Vec<&str> = body.split('\n').collect();
    let first_at = lines.iter().position(|l| !strip(l).is_empty());
    let first_text = first_at.map_or("", |i| lines[i]);
    if first_text.is_empty() || first_text.starts_with(' ') {
        return None;
    }
    // A white-space-only line before the first text would set the block's indentation (YAML auto-detects it from
    // the leading blank lines) and break the file; LocOS writes it anyway, Bilingua double-quotes instead.
    if lines[..first_at.unwrap_or(0)].iter().any(|l| !l.is_empty()) {
        return None;
    }
    let mut header = header.to_string();
    if body.contains('\n') && style == Style::Folded {
        header = header.replacen('>', "|", 1);
    } else if !body.contains('\n') && style == Style::Folded && body != strip(body) {
        return None;
    }
    let mut out = String::new();
    for l in &lines {
        if !l.is_empty() {
            out.push_str(indent);
            out.push_str(l);
        }
        out.push_str(eol);
    }
    if !content_part.ends_with('\n') {
        out.truncate(out.len() - eol.len());
    }
    Some(format!("{header}{out}{tail}"))
}

/// The text that replaces the scalar's span.
pub fn render(new: &str, style: Style, flow: bool, region: &str) -> String {
    match style {
        Style::Literal | Style::Folded => {
            if let Some(b) = render_block(new, style, region) {
                return b;
            }
            let tail = match region.find('\n') {
                Some(nl_at) => {
                    let ps = pieces(&region[nl_at + 1..]);
                    let last = ps.iter().rposition(|p| !strip(p).is_empty());
                    ps[last.map_or(0, |l| l + 1)..].concat()
                }
                None => String::new(),
            };
            let eol = if region.contains("\r\n") { "\r\n" } else { "\n" };
            format!("{}{}{}", dq(new), if region.ends_with('\n') { eol } else { "" }, tail)
        }
        Style::Single if !new.contains('\n') && !unsafe_unquoted(new) => format!("'{}'", new.replace('\'', "''")),
        Style::Plain if plain_ok(new, flow) => new.to_string(),
        _ => dq(new),
    }
}

/// A root locale key in the style of the original.
pub fn render_key(locale: &str, style: Style) -> String {
    match style {
        Style::Single => format!("'{}'", locale.replace('\'', "''")),
        Style::Double => dq(locale),
        _ => {
            if plain_ok(locale, false) {
                locale.to_string()
            } else {
                dq(locale)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_style() {
        assert_eq!(render("Klar", Style::Plain, false, "plain"), "Klar");
        assert_eq!(render("Ja", Style::Single, false, "'x'"), "'Ja'");
        assert_eq!(render("yes", Style::Plain, false, "x"), "\"yes\"");
        assert_eq!(render("x\ny", Style::Folded, false, ">-\n  a\n  b\n"), "|-\n  x\n  y\n");
        // a white-space-only first line would change the block's indentation: double-quoted instead
        assert_eq!(render(" \n%\n", Style::Literal, false, "|\n    blk\n"), "\" \\n%\\n\"\n");
    }
}

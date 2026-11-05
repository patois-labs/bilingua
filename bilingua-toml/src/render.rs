//! Writing a translated value as a TOML string token in the quoting style the original had.

use crate::scan::Kind;
use std::fmt::Write;

fn is_ctrl(c: char) -> bool {
    (c as u32) < 0x20 || c == '\u{7f}'
}

fn push_escape(out: &mut String, ch: char) {
    match ch {
        '\u{8}' => out.push_str("\\b"),
        '\t' => out.push_str("\\t"),
        '\n' => out.push_str("\\n"),
        '\u{c}' => out.push_str("\\f"),
        '\r' => out.push_str("\\r"),
        '"' => out.push_str("\\\""),
        '\\' => out.push_str("\\\\"),
        c if is_ctrl(c) => {
            let _ = write!(out, "\\u{:04X}", c as u32);
        }
        c => out.push(c),
    }
}

/// The body of a basic string: every character that needs it is escaped.
pub fn escape_basic(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for ch in s.chars() {
        push_escape(&mut out, ch);
    }
    out
}

/// The body of a multi-line basic string. Real line breaks are kept (written as `nl`); a run of three quotes
/// or quotes touching the closing delimiter are escaped, ordinary quotes stay readable.
fn escape_ml_basic(s: &str, lead: &str, nl: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    let mut out = String::with_capacity(s.len() + 8);
    let mut i = 0;
    while i < n {
        let ch = chars[i];
        match ch {
            '"' => {
                let mut j = i;
                while j < n && chars[j] == '"' {
                    j += 1;
                }
                let run = j - i;
                for _ in 0..run {
                    out.push_str(if run >= 3 || j == n { "\\\"" } else { "\"" });
                }
                i = j;
                continue;
            }
            '\n' => {
                if i == 0 && lead.is_empty() {
                    out.push_str("\\n");
                } else {
                    out.push_str(nl);
                }
            }
            '\t' => out.push('\t'),
            c => push_escape(&mut out, c),
        }
        i += 1;
    }
    out
}

fn bad_control(s: &str, allow: &str) -> bool {
    s.chars().any(|c| is_ctrl(c) && !allow.contains(c))
}

/// The complete string token (delimiters included) that holds `value`. `lead` is the line break that followed
/// the opening delimiter of a multi-line original (kept), `file_nl` the file's line ending.
pub fn render(kind: Kind, lead: &str, value: &str, file_nl: &str) -> String {
    let value = value.replace("\r\n", "\n");
    let nl = if lead.is_empty() { file_nl } else { lead };
    let mut kind = kind;
    if kind == Kind::Literal {
        if !value.contains('\'') && !bad_control(&value, "\t") {
            return format!("'{value}'");
        }
        kind = Kind::Basic;
    }
    if kind == Kind::Basic {
        return format!("\"{}\"", escape_basic(&value));
    }
    if kind == Kind::MlLiteral {
        let ok = !value.contains("'''")
            && !value.ends_with('\'')
            && !bad_control(&value, "\t\n")
            && !(value.starts_with('\n') && lead.is_empty());
        if ok {
            return format!("'''{lead}{}'''", value.replace('\n', nl));
        }
    }
    format!("\"\"\"{lead}{}\"\"\"", escape_ml_basic(&value, lead, nl))
}

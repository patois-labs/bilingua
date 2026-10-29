//! Cell references, sheet name quoting and the header and footer code tokenizer.

use crate::rich::Atom;

/// "AB12" or "$AB$12" to (column, row), both 1-based.
pub fn parse_ref(s: &str) -> Option<(u32, u32)> {
    let s = s.replace('$', "");
    let split = s.find(|c: char| c.is_ascii_digit())?;
    let (letters, digits) = s.split_at(split);
    if letters.is_empty() || !letters.chars().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    let mut col: u32 = 0;
    for c in letters.chars() {
        col = col
            .checked_mul(26)?
            .checked_add(c.to_ascii_uppercase() as u32 - 'A' as u32 + 1)?;
    }
    let row: u32 = digits.parse().ok()?;
    Some((col, row))
}

/// (column, row) to "AB12", both 1-based.
pub fn cell_name(col: u32, row: u32) -> String {
    let mut letters = Vec::new();
    let mut c = col.max(1);
    while c > 0 {
        let rem = ((c - 1) % 26) as u8;
        letters.push((b'A' + rem) as char);
        c = (c - 1) / 26;
    }
    letters.reverse();
    format!("{}{}", letters.into_iter().collect::<String>(), row)
}

/// A range such as "A1:C5" or a single cell, as (c1, r1, c2, r2).
pub fn parse_range(s: &str) -> Option<(u32, u32, u32, u32)> {
    match s.split_once(':') {
        Some((a, b)) => {
            let (c1, r1) = parse_ref(a)?;
            let (c2, r2) = parse_ref(b)?;
            Some((c1.min(c2), r1.min(r2), c1.max(c2), r1.max(r2)))
        }
        None => {
            let (c, r) = parse_ref(s)?;
            Some((c, r, c, r))
        }
    }
}

/// A space separated list of ranges (`sqref`).
pub fn in_sqref(sqref: &str, col: u32, row: u32) -> bool {
    sqref
        .split_whitespace()
        .filter_map(parse_range)
        .any(|(c1, r1, c2, r2)| col >= c1 && col <= c2 && row >= r1 && row <= r2)
}

/// True when a sheet name must be written in single quotes inside a formula.
pub fn needs_quote(name: &str) -> bool {
    let mut it = name.chars();
    let first_ok = it.next().is_some_and(|c| c.is_alphabetic() || c == '_');
    if !first_ok || !name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.') {
        return true;
    }
    // Looks like a cell reference (A1, R1C1) or a keyword.
    parse_ref(name).is_some_and(|(c, r)| c <= 16384 && r <= 1_048_576) || name.eq_ignore_ascii_case("true") || name.eq_ignore_ascii_case("false")
}

pub fn quote_sheet(name: &str) -> String {
    if needs_quote(name) {
        format!("'{}'", name.replace('\'', "''"))
    } else {
        name.to_string()
    }
}

/// Sheet name as used in "Sheet1!B4" locations.
pub fn location(sheet: &str, cell: &str) -> String {
    format!("{}!{}", quote_sheet(sheet), cell)
}

/// Splits header/footer text into text and code atoms. `&&` is a literal ampersand.
pub fn hf_atoms(s: &str) -> Vec<Atom> {
    let mut out: Vec<Atom> = Vec::new();
    let cs: Vec<char> = s.chars().collect();
    let mut text = String::new();
    let flush = |text: &mut String, out: &mut Vec<Atom>| {
        if !text.is_empty() {
            out.push(Atom::Text { fmt: String::new(), key: String::new(), text: std::mem::take(text) });
        }
    };
    let mut i = 0;
    while i < cs.len() {
        let c = cs[i];
        if c != '&' {
            text.push(c);
            i += 1;
            continue;
        }
        let Some(&n) = cs.get(i + 1) else {
            flush(&mut text, &mut out);
            out.push(code("&".into()));
            i += 1;
            continue;
        };
        match n {
            '&' => {
                text.push('&');
                i += 2;
            }
            '"' => {
                flush(&mut text, &mut out);
                let mut j = i + 2;
                while j < cs.len() && cs[j] != '"' {
                    j += 1;
                }
                let end = (j + 1).min(cs.len());
                out.push(code(cs[i..end].iter().collect()));
                i = end;
            }
            d if d.is_ascii_digit() => {
                flush(&mut text, &mut out);
                let mut j = i + 1;
                while j < cs.len() && cs[j].is_ascii_digit() {
                    j += 1;
                }
                out.push(code(cs[i..j].iter().collect()));
                i = j;
            }
            'K' => {
                flush(&mut text, &mut out);
                let end = (i + 8).min(cs.len());
                out.push(code(cs[i..end].iter().collect()));
                i = end;
            }
            _ => {
                flush(&mut text, &mut out);
                out.push(code(cs[i..i + 2].iter().collect()));
                i += 2;
            }
        }
    }
    flush(&mut text, &mut out);
    out
}

fn code(xml: String) -> Atom {
    let label = match xml.as_str() {
        "&L" => "left section",
        "&C" => "centre section",
        "&R" => "right section",
        "&P" => "page number",
        "&N" => "page count",
        "&D" => "date",
        "&T" => "time",
        "&F" => "file name",
        "&A" => "sheet name",
        "&Z" => "file path",
        "&G" => "picture",
        _ => "header code",
    }
    .to_string();
    Atom::Code { display: xml.clone(), xml, label }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refs() {
        assert_eq!(parse_ref("B4"), Some((2, 4)));
        assert_eq!(parse_ref("$AA$10"), Some((27, 10)));
        assert_eq!(parse_ref("1B"), None);
        assert!(in_sqref("A1:B3 D5", 4, 5));
        assert!(!in_sqref("A1:B3", 3, 1));
    }

    #[test]
    fn quoting() {
        assert_eq!(quote_sheet("Sheet1"), "Sheet1");
        assert_eq!(quote_sheet("Ventas 2026"), "'Ventas 2026'");
        assert_eq!(quote_sheet("売上"), "売上");
        assert_eq!(quote_sheet("A1"), "'A1'");
        assert_eq!(quote_sheet("Q3"), "'Q3'");
        assert_eq!(quote_sheet("it's"), "'it''s'");
    }

    #[test]
    fn header_codes() {
        let a = hf_atoms("&L&\"Arial,Bold\"&12Page &P of &N&R&&Co");
        let shown: Vec<String> = a
            .iter()
            .map(|x| match x {
                Atom::Text { text, .. } => format!("T:{text}"),
                Atom::Code { xml, .. } => format!("C:{xml}"),
            })
            .collect();
        assert_eq!(
            shown,
            vec!["C:&L", "C:&\"Arial,Bold\"", "C:&12", "T:Page ", "C:&P", "T: of ", "C:&N", "C:&R", "T:&Co"]
        );
    }
}

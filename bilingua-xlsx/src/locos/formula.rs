//! openpyxl's formula `Translator` (and the `Tokenizer` it stands on), for shared formulas: a dependent reads as
//! its master's formula moved to the dependent's cell, and column mode gives dependents their own copy before it
//! overwrites a master. Errors are where openpyxl raises (a malformed formula, a reference moved above row 1 or
//! outside columns A to ZZZ): openpyxl then fails to load or write the workbook.

/// An operand that the translator moves (`OPERAND` of subtype `RANGE`; numbers and names come back unchanged).
struct Tok {
    value: String,
    range: bool,
}

const ENDERS: &str = ",;}) +-*/^&=><%";
const ERRORS: [&str; 8] = ["#NULL!", "#DIV/0!", "#VALUE!", "#REF!", "#NAME?", "#NUM!", "#N/A", "#GETTING_DATA"];

#[derive(Clone, Copy, PartialEq)]
enum Open {
    Func,
    Array,
    Paren,
}

fn operand(value: String) -> Tok {
    let range = !value.starts_with('"') && !value.starts_with('#') && value != "TRUE" && value != "FALSE";
    Tok { value, range }
}

/// `SN_RE`: `^[1-9](\.[0-9]+)?[Ee]$`.
fn scientific(t: &str) -> bool {
    let b = t.as_bytes();
    if b.len() < 2 || !(b'1'..=b'9').contains(&b[0]) || !matches!(b[b.len() - 1], b'E' | b'e') {
        return false;
    }
    let mid = &b[1..b.len() - 1];
    mid.is_empty() || (mid.len() >= 2 && mid[0] == b'.' && mid[1..].iter().all(u8::is_ascii_digit))
}

/// `Tokenizer(formula).items`; `Err` where it raises `TokenizerError` (or Python's own errors).
fn tokenize(f: &[char]) -> Result<Vec<Tok>, ()> {
    let mut items: Vec<Tok> = Vec::new();
    if f.is_empty() {
        return Ok(items);
    }
    if f[0] != '=' {
        items.push(Tok { value: f.iter().collect(), range: false });
        return Ok(items);
    }
    let mut stack: Vec<Open> = Vec::new();
    let mut token = String::new();
    // the last piece appended to the token (a char, a quoted name or a bracket group), for `assert_empty_token`
    let mut last = String::new();
    let mut i = 1;
    let save = |token: &mut String, items: &mut Vec<Tok>| {
        if !token.is_empty() {
            items.push(operand(std::mem::take(token)));
        }
    };
    while i < f.len() {
        let c = f[i];
        if (c == '+' || c == '-') && !token.is_empty() && scientific(&token) {
            token.push(c);
            last = c.to_string();
            i += 1;
            continue;
        }
        if ENDERS.contains(c) {
            save(&mut token, &mut items);
        }
        match c {
            '"' | '\'' => {
                if !token.is_empty() && last != ":" {
                    return Err(());
                }
                let mut j = i + 1;
                let end = loop {
                    let k = (j..f.len()).find(|&k| f[k] == c).ok_or(())?;
                    if f.get(k + 1) == Some(&c) {
                        j = k + 2;
                    } else {
                        break k;
                    }
                };
                let m: String = f[i..=end].iter().collect();
                if c == '"' {
                    items.push(operand(m));
                } else {
                    token.push_str(&m);
                    last = m;
                }
                i = end + 1;
            }
            '[' => {
                let mut depth = 0i64;
                let mut end = None;
                for (k, &ch) in f.iter().enumerate().skip(i) {
                    if ch == '[' {
                        depth += 1;
                    } else if ch == ']' {
                        depth -= 1;
                    }
                    if depth == 0 {
                        end = Some(k);
                        break;
                    }
                }
                let end = end.ok_or(())?;
                let m: String = f[i..=end].iter().collect();
                token.push_str(&m);
                last = m;
                i = end + 1;
            }
            '#' => {
                if !token.is_empty() && last != "!" {
                    return Err(());
                }
                let rest: String = f[i..f.len().min(i + 13)].iter().collect();
                let err = ERRORS.iter().find(|e| rest.starts_with(*e)).ok_or(())?;
                items.push(operand(format!("{}{err}", std::mem::take(&mut token))));
                i += err.chars().count();
            }
            ' ' | '\n' => {
                items.push(Tok { value: c.to_string(), range: false });
                while i < f.len() && (f[i] == ' ' || f[i] == '\n') {
                    i += 1;
                }
            }
            '+' | '-' | '*' | '/' | '^' | '&' | '=' | '>' | '<' | '%' => {
                let two: String = f[i..f.len().min(i + 2)].iter().collect();
                let n = if matches!(two.as_str(), ">=" | "<=" | "<>") { 2 } else { 1 };
                items.push(Tok { value: f[i..i + n].iter().collect(), range: false });
                i += n;
            }
            '{' | '(' => {
                let (value, kind) = if c == '{' {
                    if !token.is_empty() {
                        return Err(());
                    }
                    ("{".to_string(), Open::Array)
                } else if !token.is_empty() {
                    (format!("{}(", std::mem::take(&mut token)), Open::Func)
                } else {
                    ("(".to_string(), Open::Paren)
                };
                items.push(Tok { value, range: false });
                stack.push(kind);
                i += 1;
            }
            ')' | '}' => {
                let open = stack.pop().ok_or(())?;
                let closer = if open == Open::Array { '}' } else { ')' };
                if closer != c {
                    return Err(());
                }
                items.push(Tok { value: c.to_string(), range: false });
                i += 1;
            }
            ';' | ',' => {
                items.push(Tok { value: c.to_string(), range: false });
                i += 1;
            }
            _ => {
                token.push(c);
                last = c.to_string();
                i += 1;
            }
        }
    }
    save(&mut token, &mut items);
    Ok(items)
}

/// Python's `$` at `i`: the end, or a line break that ends the string.
fn at_end(s: &[u8], i: usize) -> bool {
    i == s.len() || (i + 1 == s.len() && s[i] == b'\n')
}

/// `\$?[1-9][0-9]{0,6}` at `i`: its end.
fn row_part(s: &[u8], mut i: usize) -> Option<usize> {
    if s.get(i) == Some(&b'$') {
        i += 1;
    }
    if !s.get(i).is_some_and(|b| (b'1'..=b'9').contains(b)) {
        return None;
    }
    let n = s[i + 1..].iter().take(6).take_while(|b| b.is_ascii_digit()).count();
    Some(i + 1 + n)
}

/// `\$?[A-Za-z]{1,3}` at `i`, as many letters as the next part allows: every possible end.
fn col_ends(s: &[u8], mut i: usize) -> Vec<usize> {
    if s.get(i) == Some(&b'$') {
        i += 1;
    }
    let n = s[i.min(s.len())..].iter().take(3).take_while(|b| b.is_ascii_alphabetic()).count();
    (1..=n).rev().map(|k| i + k).collect()
}

fn move_row(part: &str, d: i64) -> Result<String, ()> {
    if part.starts_with('$') {
        return Ok(part.to_string());
    }
    let n = part.parse::<i64>().map_err(|_| ())? + d;
    if n <= 0 {
        return Err(());
    }
    Ok(n.to_string())
}

fn move_col(part: &str, d: i64) -> Result<String, ()> {
    if part.starts_with('$') {
        return Ok(part.to_string());
    }
    let n = part.to_ascii_uppercase().bytes().fold(0i64, |n, b| n * 26 + (b - b'A' + 1) as i64) + d;
    if !(1..=18278).contains(&n) {
        return Err(());
    }
    Ok(super::col_letters(n))
}

/// `Translator.translate_range`.
fn translate_range(r: &str, dr: i64, dc: i64) -> Result<String, ()> {
    let (ws, r) = match r.rsplit_once('!') {
        Some((a, b)) => (format!("{a}!"), b),
        None => (String::new(), r),
    };
    let s = r.as_bytes();
    // ROW_RANGE_RE: `3:4`
    if let Some(e1) = row_part(s, 0) {
        if s.get(e1) == Some(&b':') {
            if let Some(e2) = row_part(s, e1 + 1).filter(|&e| at_end(s, e)) {
                return Ok(format!("{ws}{}:{}", move_row(&r[..e1], dr)?, move_row(&r[e1 + 1..e2], dr)?));
            }
        }
    }
    // COL_RANGE_RE: `A:BC`
    for e1 in col_ends(s, 0) {
        if s.get(e1) == Some(&b':') {
            if let Some(e2) = col_ends(s, e1 + 1).into_iter().find(|&e| at_end(s, e)) {
                return Ok(format!("{ws}{}:{}", move_col(&r[..e1], dc)?, move_col(&r[e1 + 1..e2], dc)?));
            }
        }
    }
    if r.contains(':') {
        let parts: Result<Vec<String>, ()> = r.split(':').map(|p| translate_range(p, dr, dc)).collect();
        return Ok(format!("{ws}{}", parts?.join(":")));
    }
    // CELL_REF_RE: `$A$1`
    for e1 in col_ends(s, 0) {
        if let Some(e2) = row_part(s, e1).filter(|&e| at_end(s, e)) {
            return Ok(format!("{ws}{}{}", move_col(&r[..e1], dc)?, move_row(&r[e1..e2], dr)?));
        }
    }
    Ok(format!("{ws}{r}"))
}

/// `Translator(formula, origin).translate_formula(dest)`: `formula` with its leading `=`; `origin` and `dest` are
/// (row, column), `dest` `None` for a cell without an address (openpyxl then moves nothing).
pub(super) fn translate(formula: &str, origin: (i64, i64), dest: Option<(i64, i64)>) -> Result<String, ()> {
    let chars: Vec<char> = formula.chars().collect();
    let toks = tokenize(&chars)?;
    let Some(first) = toks.first() else { return Ok(String::new()) };
    if chars.first() != Some(&'=') {
        return Ok(first.value.clone());
    }
    let (dr, dc) = dest.map_or((0, 0), |d| (d.0 - origin.0, d.1 - origin.1));
    let mut out = String::from("=");
    for t in &toks {
        if t.range {
            out.push_str(&translate_range(&t.value, dr, dc)?);
        } else {
            out.push_str(&t.value);
        }
    }
    Ok(out)
}

/// Whether openpyxl's tokenizer accepts the formula (a shared-formula master is tokenized when it is read).
pub(super) fn parses(formula: &str) -> bool {
    tokenize(&formula.chars().collect::<Vec<_>>()).is_ok()
}

#[cfg(test)]
mod formula_tests {
    use super::translate;

    fn t(f: &str, dr: i64, dc: i64) -> Result<String, ()> {
        translate(f, (10, 10), Some((10 + dr, 10 + dc)))
    }

    #[test]
    fn moves_relative_references_as_openpyxl() {
        assert_eq!(t("=UPPER(A2)", 1, 0).unwrap(), "=UPPER(A3)");
        assert_eq!(t("=$A$1+B$2+$C3", 2, 1).unwrap(), "=$A$1+C$2+$C5");
        assert_eq!(t("=SUM(A1:B2,\"A1\")", 1, 1).unwrap(), "=SUM(B2:C3,\"A1\")");
        assert_eq!(t("='My Sheet'!A1+Other!B1", 1, 0).unwrap(), "='My Sheet'!A2+Other!B2");
        assert_eq!(t("=SUM(A:B)+SUM(1:2)", 3, 1).unwrap(), "=SUM(B:C)+SUM(4:5)");
        assert_eq!(t("=LOG10(A1)*1.5E+3", 1, 0).unwrap(), "=LOG10(A2)*1.5E+3");
        assert_eq!(t("=T1[Col1]+A1", 1, 0).unwrap(), "=T1[Col1]+A2");
        assert_eq!(t("=A1  +   B1", 0, 0).unwrap(), "=A1 + B1"); // the tokenizer keeps one white-space character
        assert!(t("=A1", -1, 0).is_err());
        assert!(t("=SUM(A1", 0, 0).is_ok() && t("=A1)", 0, 0).is_err() && t("=\"open", 0, 0).is_err() && t("=#BAD", 0, 0).is_err());
        assert_eq!(t("=A1+#REF!", 1, 0).unwrap(), "=A2+#REF!");
        assert_eq!(t("=a1", 1, 0).unwrap(), "=A2");
    }
}

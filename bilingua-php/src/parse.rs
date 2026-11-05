//! The parser: accepts only array literals, `key => value` pairs, string values (static `'a' . 'b'` concatenation
//! included), numbers, `true` / `false` / `null`, `declare(strict_types=1);` and the CodeIgniter direct-access guard.
//! Everything else refuses the file with a line number.

use crate::lex::{lex, refuse, Kind, Quote, Tok};
use bilingua_model::{Error, LimitError, LimitKind, Limits};

const MAX_DEPTH: usize = 32;

/// One string value of the file that has a letter or digit (or any string, for `all`).
#[derive(Debug, Clone)]
pub struct RawUnit {
    pub start: usize,
    pub end: usize,
    pub quote: Quote,
    pub value: String,
    pub key: String,
    /// Has a letter or digit; the others (symbols) are kept for `include_locked`.
    pub translatable: bool,
}

pub struct Parsed {
    /// Every non-blank string value, in source order.
    pub units: Vec<RawUnit>,
    /// How many of them have a letter or digit.
    pub translatable: usize,
    /// `return` and assignment statements seen.
    pub statements: usize,
}

struct Parser<'a, 'l> {
    text: &'a str,
    toks: Vec<Tok<'a>>,
    i: usize,
    limits: &'l Limits,
    out: Parsed,
}

/// Where the code starts: after an optional BOM and white space, `<?php` followed by white space.
fn open_tag(text: &str) -> Option<usize> {
    let t = text.strip_prefix('\u{feff}').unwrap_or(text);
    let skipped = text.len() - t.len();
    let ws = t
        .bytes()
        .take_while(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'))
        .count();
    let rest = &t[ws..];
    let head = rest.as_bytes().get(..5)?;
    if !head.eq_ignore_ascii_case(b"<?php") {
        return None;
    }
    if !matches!(rest.as_bytes().get(5), Some(b' ' | b'\t' | b'\r' | b'\n')) {
        return None;
    }
    Some(skipped + ws + 5)
}

pub fn parse(text: &str, limits: &Limits) -> Result<Parsed, Error> {
    let Some(code) = open_tag(text) else {
        return Err(refuse(text, 0, "the file must start with <?php"));
    };
    let toks = lex(text, code, limits)?;
    let mut p = Parser {
        text,
        toks,
        i: 0,
        limits,
        out: Parsed {
            units: Vec::new(),
            translatable: 0,
            statements: 0,
        },
    };
    p.walk()?;
    Ok(p.out)
}

impl<'a> Parser<'a, '_> {
    fn peek(&self) -> Option<&Tok<'a>> {
        self.toks.get(self.i)
    }

    fn is(t: Option<&Tok<'a>>, kind: Kind, raw: Option<&str>) -> bool {
        match t {
            Some(t) if t.kind == kind => raw.is_none_or(|r| t.raw.eq_ignore_ascii_case(r)),
            _ => false,
        }
    }

    fn next(&mut self) -> Result<Tok<'a>, Error> {
        match self.toks.get(self.i) {
            Some(t) => {
                self.i += 1;
                Ok(t.clone())
            }
            None => Err(refuse(
                self.text,
                self.text.len(),
                "the file ends in the middle of a statement",
            )),
        }
    }

    fn expect(&mut self, kind: Kind, raw: Option<&str>, what: &str) -> Result<Tok<'a>, Error> {
        let t = self.next()?;
        if !Self::is(Some(&t), kind, raw) {
            let found: String = t.raw.chars().take(20).collect();
            return Err(refuse(
                self.text,
                t.start,
                &format!("expected {what}, found {found:?}"),
            ));
        }
        Ok(t)
    }

    fn punct(&mut self, p: &str) -> Result<(), Error> {
        self.expect(Kind::Punct, Some(p), &format!("'{p}'")).map(|_| ())
    }

    fn end_statement(&mut self) -> Result<(), Error> {
        if Self::is(self.peek(), Kind::Close, None) {
            return Ok(());
        }
        self.punct(";")
    }

    fn walk(&mut self) -> Result<(), Error> {
        loop {
            let Some(t) = self.peek() else { return Ok(()) };
            if t.kind == Kind::Close {
                return Ok(());
            }
            if Self::is(Some(t), Kind::Punct, Some(";")) {
                self.i += 1;
            } else if Self::is(Some(t), Kind::Ident, Some("return")) {
                self.i += 1;
                self.out.statements += 1;
                self.value(&mut Vec::new(), 0)?;
                self.end_statement()?;
            } else if Self::is(Some(t), Kind::Ident, Some("declare")) {
                self.declare()?;
            } else if Self::is(Some(t), Kind::Ident, Some("defined")) {
                self.guard()?;
            } else if t.kind == Kind::Var {
                self.assign()?;
            } else {
                let found: String = t.raw.chars().take(20).collect();
                return Err(refuse(
                    self.text,
                    t.start,
                    &format!("{found:?} is not an array literal"),
                ));
            }
        }
    }

    fn declare(&mut self) -> Result<(), Error> {
        self.i += 1;
        self.punct("(")?;
        self.expect(Kind::Ident, Some("strict_types"), "strict_types")?;
        self.punct("=")?;
        self.expect(Kind::Num, None, "a number")?;
        self.punct(")")?;
        self.end_statement()
    }

    /// `defined('BASEPATH') OR exit('No direct script access allowed');`
    fn guard(&mut self) -> Result<(), Error> {
        self.i += 1;
        self.punct("(")?;
        self.expect(Kind::Str, None, "a constant name")?;
        self.punct(")")?;
        let t = self.next()?;
        let only = "only `defined(...) OR exit(...)` is allowed";
        if !(Self::is(Some(&t), Kind::Ident, Some("or")) || Self::is(Some(&t), Kind::Punct, Some("||")))
        {
            return Err(refuse(self.text, t.start, only));
        }
        let t = self.next()?;
        if !(Self::is(Some(&t), Kind::Ident, Some("exit"))
            || Self::is(Some(&t), Kind::Ident, Some("die")))
        {
            return Err(refuse(self.text, t.start, only));
        }
        if Self::is(self.peek(), Kind::Punct, Some("(")) {
            self.i += 1;
            if Self::is(self.peek(), Kind::Str, None) {
                self.i += 1;
            }
            self.punct(")")?;
        }
        self.end_statement()
    }

    fn assign(&mut self) -> Result<(), Error> {
        let var = self.next()?;
        self.out.statements += 1;
        let name = &var.raw[1..];
        let mut path: Vec<String> = if name == "lang" || name == "LANG" {
            Vec::new()
        } else {
            vec![name.to_string()]
        };
        while Self::is(self.peek(), Kind::Punct, Some("[")) {
            self.i += 1;
            if Self::is(self.peek(), Kind::Punct, Some("]")) {
                path.push("[]".into());
            } else {
                path.push(self.key()?);
            }
            self.punct("]")?;
        }
        self.punct("=")?;
        self.value(&mut path, 0)?;
        self.end_statement()
    }

    fn key(&mut self) -> Result<String, Error> {
        let t = self.next()?;
        if t.kind == Kind::Str {
            return Ok(t.value.into_owned());
        }
        if Self::is(Some(&t), Kind::Punct, Some("-")) {
            let n = self.expect(Kind::Num, None, "a number")?;
            return Ok(format!("-{}", n.raw));
        }
        if t.kind == Kind::Num {
            return Ok(t.raw.to_string());
        }
        let found: String = t.raw.chars().take(20).collect();
        Err(refuse(
            self.text,
            t.start,
            &format!("array key {found:?} is not a literal"),
        ))
    }

    fn value(&mut self, path: &mut Vec<String>, depth: usize) -> Result<(), Error> {
        if depth > MAX_DEPTH {
            return Err(Error::Unsupported(
                "This PHP file is nested too deeply.".into(),
            ));
        }
        let t = self.next()?;
        if t.kind == Kind::Str {
            let (start, quote) = (t.start, t.quote);
            let mut end = t.end;
            let mut value = t.value.into_owned();
            while Self::is(self.peek(), Kind::Punct, Some(".")) {
                self.i += 1;
                let nxt = self.next()?;
                if nxt.kind != Kind::Str {
                    return Err(refuse(
                        self.text,
                        nxt.start,
                        "only string literals may be concatenated",
                    ));
                }
                end = nxt.end;
                value.push_str(&nxt.value);
            }
            let translatable = value.chars().any(char::is_alphanumeric);
            if translatable || !value.trim().is_empty() {
                self.out.units.push(RawUnit {
                    start,
                    end,
                    quote,
                    key: path.join("."),
                    value,
                    translatable,
                });
            }
            if translatable {
                self.out.translatable += 1;
                if self.out.translatable > self.limits.max_units {
                    return Err(LimitError::new(
                        LimitKind::Units,
                        format!(
                            "This file has more than {} segments. Split it into smaller files.",
                            self.limits.max_units
                        ),
                    )
                    .into());
                }
            }
            return Ok(());
        }
        if Self::is(Some(&t), Kind::Punct, Some("[")) {
            return self.items(path, depth, "]");
        }
        if Self::is(Some(&t), Kind::Ident, Some("array")) {
            self.punct("(")?;
            return self.items(path, depth, ")");
        }
        if Self::is(Some(&t), Kind::Punct, Some("-")) || Self::is(Some(&t), Kind::Punct, Some("+")) {
            self.expect(Kind::Num, None, "a number")?;
            return Ok(());
        }
        if t.kind == Kind::Num
            || (t.kind == Kind::Ident
                && ["true", "false", "null"]
                    .iter()
                    .any(|w| t.raw.eq_ignore_ascii_case(w)))
        {
            return Ok(());
        }
        let found: String = t.raw.chars().take(20).collect();
        Err(refuse(
            self.text,
            t.start,
            &format!("{found:?} is not a literal value"),
        ))
    }

    fn items(&mut self, path: &mut Vec<String>, depth: usize, closer: &str) -> Result<(), Error> {
        let mut auto: i128 = 0;
        loop {
            if Self::is(self.peek(), Kind::Punct, Some(closer)) {
                self.i += 1;
                return Ok(());
            }
            let save = self.i;
            let mut key: Option<String> = None;
            if let Some(t) = self.peek() {
                if matches!(t.kind, Kind::Str | Kind::Num) || Self::is(Some(t), Kind::Punct, Some("-")) {
                    let k = self.key()?;
                    if Self::is(self.peek(), Kind::Punct, Some("=>")) {
                        self.i += 1;
                        let digits = k.strip_prefix('-').unwrap_or(&k);
                        if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
                            if let Ok(n) = k.parse::<i128>() {
                                auto = auto.max(n.saturating_add(1));
                            }
                        }
                        key = Some(k);
                    } else {
                        self.i = save;
                    }
                }
            }
            let key = key.unwrap_or_else(|| {
                let k = auto.to_string();
                auto = auto.saturating_add(1);
                k
            });
            path.push(key);
            let r = self.value(path, depth + 1);
            path.pop();
            r?;
            let t = self.next()?;
            if Self::is(Some(&t), Kind::Punct, Some(closer)) {
                return Ok(());
            }
            if !Self::is(Some(&t), Kind::Punct, Some(",")) {
                let found: String = t.raw.chars().take(20).collect();
                return Err(refuse(
                    self.text,
                    t.start,
                    &format!("expected ',' or '{closer}', found {found:?}"),
                ));
            }
        }
    }
}

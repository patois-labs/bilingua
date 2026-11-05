//! The brace and tag scanner of the LocOS MDX filter (`_Scanner`): `{expressions}` (JavaScript aware: strings,
//! template literals and comments do not unbalance them) and JSX tags, with one shared step budget so a file of
//! unclosed braces is refused instead of rescanned from every brace. Nothing is executed.

use crate::pyx::is_space;
use bilingua_model::{Error, LimitError, LimitKind};

/// One expression or tag may span at most this many characters; past it the `{` or `<` is a literal character.
pub(crate) const MAX_SPAN: usize = 100_000;

#[derive(Debug, Clone)]
pub(crate) struct Tag {
    pub start: usize,
    pub end: usize,
    /// (name, value start, value end, quote) of every string attribute.
    pub attrs: Vec<(String, usize, usize, u8)>,
    /// Every attribute name, string or expression valued.
    pub names: Vec<String>,
}

pub(crate) struct Scanner<'a> {
    t: &'a str,
    b: &'a [u8],
    ascii: bool,
    budget: i64,
}

impl<'a> Scanner<'a> {
    pub fn new(t: &'a str) -> Self {
        let chars = if t.is_ascii() { t.len() } else { t.chars().count() };
        Scanner { t, b: t.as_bytes(), ascii: t.is_ascii(), budget: 4 * chars as i64 + 1_000_000 }
    }

    fn spend(&mut self, k: usize) -> Result<(), Error> {
        self.budget -= k as i64;
        if self.budget < 0 {
            return Err(LimitError::new(
                LimitKind::Units,
                "This MDX file has too much unclosed JSX or {expression} markup to read safely. Close the open braces and tags, or split the file.",
            )
            .into());
        }
        Ok(())
    }

    fn exact_limit(&self, i: usize) -> usize {
        self.t[i..].char_indices().nth(MAX_SPAN).map_or(self.b.len(), |(k, _)| i + k)
    }

    fn str_end(&self, j: usize, q: u8, limit: usize) -> usize {
        let mut k = j + 1;
        loop {
            if k >= limit {
                return limit;
            }
            let hit = self.b[k..limit].iter().position(|&c| c == b'\\' || c == q || (q != b'`' && c == b'\n'));
            let Some(p) = hit else { return limit };
            let at = k + p;
            if self.b[at] == b'\\' {
                k = at + 2;
                continue;
            }
            return at + 1;
        }
    }

    fn expr_scan(&self, i: usize, limit: usize) -> Option<usize> {
        let b = self.b;
        let mut depth = 0i32;
        let mut j = i;
        while j < limit {
            let Some(p) = b[j..limit].iter().position(|c| matches!(c, b'{' | b'}' | b'\'' | b'"' | b'`' | b'/')) else { break };
            j += p;
            match b[j] {
                b'{' => {
                    depth += 1;
                    j += 1;
                }
                b'}' => {
                    depth -= 1;
                    j += 1;
                    if depth == 0 {
                        return Some(j);
                    }
                }
                c @ (b'\'' | b'"' | b'`') => j = self.str_end(j, c, limit),
                _ => match b.get(j + 1) {
                    Some(b'/') => j = b[j..limit].iter().position(|&c| c == b'\n').map_or(limit, |q| j + q),
                    Some(b'*') => {
                        let from = j + 2;
                        let found = if from < limit { b[from..limit].windows(2).position(|w| w == b"*/").map(|q| from + q) } else { None };
                        j = found.map_or(limit, |k| k + 2);
                    }
                    _ => j += 1,
                },
            }
        }
        None
    }

    /// End offset (exclusive) of the balanced `{...}` that starts at `i`, or None.
    pub fn expr(&mut self, i: usize) -> Result<Option<usize>, Error> {
        let n = self.b.len();
        let mut limit = n.min(i + MAX_SPAN);
        let mut r = self.expr_scan(i, limit);
        if r.is_none() && !self.ascii && limit < n {
            let l2 = self.exact_limit(i);
            if l2 > limit {
                limit = l2;
                r = self.expr_scan(i, limit);
            }
        }
        match r {
            Some(e) => {
                self.spend(e - i)?;
                Ok(Some(e))
            }
            None => {
                self.spend(limit - i)?;
                Ok(None)
            }
        }
    }

    fn name_run(&self, j: usize) -> usize {
        let mut k = j;
        for (o, ch) in self.t[j..].char_indices() {
            if is_space(ch) || matches!(ch, '=' | '/' | '>' | '{' | '"' | '\'' | '<') {
                return j + o;
            }
            k = j + o + ch.len_utf8();
        }
        k
    }

    fn tag_scan(&mut self, i: usize, limit: usize) -> Result<Result<Tag, usize>, Error> {
        let b = self.b;
        let mut j = i + 1;
        let mut attrs: Vec<(String, usize, usize, u8)> = Vec::new();
        let mut names: Vec<String> = Vec::new();
        let mut last: Option<String> = None;
        let mut pending: Option<String> = None;
        while j < limit {
            let c = b[j];
            match c {
                b' ' | b'\t' | b'\r' | b'\n' | b'/' => j += 1,
                b'>' => return Ok(Ok(Tag { start: i, end: j + 1, attrs, names })),
                b'=' => {
                    pending = last.take();
                    j += 1;
                }
                b'"' | b'\'' => {
                    let Some(p) = b[j + 1..limit.max(j + 1)].iter().position(|&x| x == c) else { break };
                    let k = j + 1 + p;
                    if let Some(name) = pending.take() {
                        attrs.push((name, j + 1, k, c));
                    }
                    pending = None;
                    j = k + 1;
                }
                b'{' => {
                    match self.expr(j)? {
                        None => break,
                        Some(e) => {
                            pending = None;
                            last = None;
                            j = e;
                        }
                    }
                }
                b'<' => break,
                _ => {
                    let e = self.name_run(j);
                    if e == j {
                        // a Unicode space the Python pattern cannot match (LocOS raises there): skip it
                        j += self.t[j..].chars().next().map_or(1, char::len_utf8);
                        continue;
                    }
                    let name = self.t[j..e].to_string();
                    pending = None;
                    last = Some(name.clone());
                    if !names.contains(&name) {
                        names.push(name);
                    }
                    j = e;
                }
            }
        }
        Ok(Err(j))
    }

    /// The JSX tag that starts at the `<` at `i` (opening, closing, self-closing or a fragment), or None when it
    /// never closes.
    pub fn tag(&mut self, i: usize) -> Result<Option<Tag>, Error> {
        let n = self.b.len();
        let mut limit = n.min(i + MAX_SPAN);
        let mut r = self.tag_scan(i, limit)?;
        if r.is_err() && !self.ascii && limit < n {
            let l2 = self.exact_limit(i);
            if l2 > limit {
                limit = l2;
                r = self.tag_scan(i, limit)?;
            }
        }
        match r {
            Ok(tag) => {
                self.spend(tag.end - i)?;
                Ok(Some(tag))
            }
            Err(j) => {
                self.spend(j.min(n).saturating_sub(i))?;
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expressions() {
        let mut s = Scanner::new("{a {b} '}' `{` // }\n c} rest");
        assert_eq!(s.expr(0).unwrap(), Some(23));
        let mut s = Scanner::new("{ never");
        assert_eq!(s.expr(0).unwrap(), None);
    }

    #[test]
    fn tags() {
        let t = "<Note title=\"A > b\" x={1} />";
        let mut s = Scanner::new(t);
        let tag = s.tag(0).unwrap().unwrap();
        assert_eq!(tag.end, t.len());
        assert_eq!(tag.attrs.len(), 1);
        assert_eq!(&t[tag.attrs[0].1..tag.attrs[0].2], "A > b");
        let mut s = Scanner::new("<A <B>");
        assert!(s.tag(0).unwrap().is_none());
    }
}

//! Writing a translated unit back as AsciiDoc (`_render`): tags back to their markup, line breaks folded, table
//! separators escaped, constrained marks glued to a word character written in their unconstrained form (#695), and
//! URL links glued to a letter written as `link:` macros (#698).

use crate::lex::*;
use bilingua_model::{Inline, TagKind, Unit};
use std::collections::HashMap;

/// Attribute name -> whether the document defines it as a URL (`_doc_attrs`); built on first use.
pub struct Attrs<'a> {
    source: Box<dyn Fn() -> String + 'a>,
    map: Option<HashMap<String, bool>>,
}

impl<'a> Attrs<'a> {
    pub fn new(source: impl Fn() -> String + 'a) -> Attrs<'a> {
        Attrs { source: Box::new(source), map: None }
    }

    fn get(&mut self, name: &str) -> Option<bool> {
        if self.map.is_none() {
            let text = (self.source)();
            let mut m = HashMap::new();
            // `^:([A-Za-z0-9_][A-Za-z0-9_-]*):[ \t]*([^\r\n]*)$` with re.MULTILINE: lines ended by "\n" only
            for seg in text.split('\n') {
                if seg.contains('\r') {
                    continue;
                }
                let Some(rest) = seg.strip_prefix(':') else { continue };
                let b = rest.as_bytes();
                if b.is_empty() || !(b[0].is_ascii_alphanumeric() || b[0] == b'_') {
                    continue;
                }
                let n = b.iter().take_while(|c| c.is_ascii_alphanumeric() || **c == b'_' || **c == b'-').count();
                let Some(val) = rest[n..].strip_prefix(':') else { continue };
                m.insert(rest[..n].to_string(), is_url_start(strip(val)));
            }
            self.map = Some(m);
        }
        self.map.as_ref().and_then(|m| m.get(name).copied())
    }
}

fn is_url_start(s: &str) -> bool {
    ["http://", "https://", "ftp://", "ftps://", "irc://", "file://"].iter().any(|p| s.starts_with(p))
}

fn is_attr_ref(s: &str) -> bool {
    s.len() > 2
        && s.starts_with('{')
        && s.ends_with('}')
        && s[1..s.len() - 1].bytes().enumerate().all(|(i, c)| c.is_ascii_alphanumeric() || c == b'_' || (i > 0 && c == b'-'))
}

fn constrained_pair_mark(open: &str, close: &str) -> Option<char> {
    let mark = open.chars().last()?;
    if !matches!(mark, '*' | '_' | '#') || close != mark.to_string() {
        return None;
    }
    let mut it = open.chars().rev();
    it.next();
    if it.next() == Some(mark) {
        return None;
    }
    Some(mark)
}

fn constrained_point(raw: &str) -> bool {
    let c: Vec<char> = raw.chars().collect();
    c.len() >= 3 && (c[0] == '`' || c[0] == '+') && c[c.len() - 1] == c[0] && c[1] != c[0]
}

fn bad_before(mark: char, c: char) -> bool {
    let set = match mark {
        '*' | '_' => ";:}",
        '#' => "&;:}",
        '`' => ";:\"'`}",
        _ => ";:\\",
    };
    set.contains(c)
}

fn bad_after(mark: char, c: char) -> bool {
    mark == '`' && "\"'`".contains(c)
}

fn text_part(s: &str, sep: Option<char>, nested: bool) -> String {
    let t = plain(s);
    match sep {
        None => t,
        Some(sc) => {
            let mut out = String::with_capacity(t.len() + 2);
            let mut prev = '\0';
            for c in t.chars() {
                // a nested table sits inside an outer `|` cell: its text must not split that cell either (#788)
                if (c == sc || (nested && c == '|')) && prev != '\\' {
                    out.push('\\');
                }
                out.push(c);
                prev = c;
            }
            out
        }
    }
}

fn unconstrain(out: &mut [String], marks: &[(usize, usize, char)]) {
    if marks.is_empty() {
        return;
    }
    let joined: Vec<char> = out.iter().flat_map(|p| p.chars()).collect();
    let mut ends = Vec::with_capacity(out.len());
    let mut n = 0usize;
    for p in out.iter() {
        n += p.chars().count();
        ends.push(n);
    }
    let len_of = |i: usize, out: &[String]| out[i].chars().count();
    let lens: Vec<usize> = (0..out.len()).map(|i| len_of(i, out)).collect();
    for &(a, b, mark) in marks {
        let start = ends[a] - lens[a];
        let end = ends[b];
        let before = if start > 0 { Some(joined[start - 1]) } else { None };
        let after = joined.get(end).copied();
        let mut bad = before.is_some_and(|c| is_wordish(c) || bad_before(mark, c));
        bad = bad || after.is_some_and(|c| is_wordish(c) || bad_after(mark, c));
        if a != b {
            let inner = &joined[ends[a]..ends[b] - lens[b]];
            bad = bad || inner.is_empty() || is_space(inner[0]) || is_space(inner[inner.len() - 1]);
        }
        if !bad {
            continue;
        }
        if a == b {
            out[a] = format!("{mark}{}{mark}", out[a]);
        } else {
            out[a].push(mark);
            out[b].insert(0, mark);
        }
    }
}

fn explicit_links(out: &mut [String], links: &[usize], attrs: &mut Attrs) {
    if links.is_empty() {
        return;
    }
    let joined: Vec<char> = out.iter().flat_map(|p| p.chars()).collect();
    let mut starts = Vec::with_capacity(out.len());
    let mut n = 0usize;
    for p in out.iter() {
        starts.push(n);
        n += p.chars().count();
    }
    for &i in links {
        let start = starts[i];
        let end = start + out[i].chars().count();
        let before = if start > 0 { Some(joined[start - 1]) } else { None };
        let after = joined.get(end).copied();
        let raw = out[i].clone();
        let attr = is_attr_ref(&raw);
        if attr {
            let is_url = attrs.get(&raw[1..raw.len() - 1]);
            if is_url == Some(false) || (is_url.is_none() && after != Some('[')) {
                continue;
            }
        }
        let glued = before.is_some_and(|c| !" \t<>()[];\"'".contains(c));
        if (attr && after == Some('[')) || raw.contains('[') {
            if glued {
                out[i] = format!("link:{raw}");
            }
        } else {
            let tail: String = joined[end.min(joined.len())..].iter().take_while(|c| !is_space(**c) && !matches!(**c, '[' | ']' | '<')).collect();
            let tail = tail.trim_end_matches([',', '.', '?', '!', ')']);
            if glued || !tail.is_empty() {
                out[i] = format!("link:{raw}[]");
            }
        }
    }
}

/// The AsciiDoc for a translated unit.
pub fn render(unit: &Unit, target: &[Inline], sep: Option<char>, nested: bool, attrs: &mut Attrs) -> String {
    // the translation is stripped like Python's str.strip: only text at either end can be white space
    let mut t: Vec<Inline> = target.to_vec();
    if let Some(Inline::Text(s)) = t.first_mut() {
        *s = s.trim_start_matches(is_space).to_string();
    }
    if let Some(Inline::Text(s)) = t.last_mut() {
        *s = s.trim_end_matches(is_space).to_string();
    }
    let mut out: Vec<String> = Vec::new();
    let mut opens: HashMap<&str, usize> = HashMap::new();
    let mut marks: Vec<(usize, usize, char)> = Vec::new();
    let mut links: Vec<usize> = Vec::new();
    let mut prev_text = false;
    for inl in &t {
        match inl {
            Inline::Text(s) => {
                out.push(text_part(s, sep, nested));
                prev_text = true;
            }
            Inline::Tag { id, kind, .. } => {
                if !prev_text {
                    out.push(String::new());
                }
                prev_text = false;
                let Some(code) = unit.codes.get(id) else { continue };
                let closing = *kind == TagKind::Close;
                if let (Some(open), Some(close)) = (&code.open, &code.close) {
                    if let Some(mark) = constrained_pair_mark(open, close) {
                        if !closing {
                            opens.insert(id.as_str(), out.len());
                        } else if let Some(o) = opens.remove(id.as_str()) {
                            marks.push((o, out.len(), mark));
                        }
                    }
                    if !closing && is_url_start(open) {
                        links.push(out.len());
                    }
                    out.push(if closing { close.clone() } else { open.clone() });
                } else if let (Some(raw), false) = (&code.standalone, closing) {
                    if constrained_point(raw) {
                        marks.push((out.len(), out.len(), raw.chars().next().unwrap_or('`')));
                    }
                    if is_url_start(raw) || is_attr_ref(raw) {
                        links.push(out.len());
                    }
                    out.push(raw.clone());
                }
            }
        }
    }
    if !prev_text {
        out.push(String::new());
    }
    unconstrain(&mut out, &marks);
    explicit_links(&mut out, &links, attrs);
    out.concat()
}

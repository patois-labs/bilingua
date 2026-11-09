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

/// `x:: w` (also `:::`, `::::`, `;;`) would turn a paragraph, admonition or term into a description list item: a
/// run of two or more `:` or `;` followed by a blank or the end is written with AsciiDoc's built-in `{two-colons}` /
/// `{two-semicolons}`, which print the same characters (LocOS `_inert_dlist`, tracker #788).
fn inert_dlist(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut pos = 0;
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c != b':' && c != b';' {
            i += 1;
            continue;
        }
        let st = i;
        while i < b.len() && b[i] == c {
            i += 1;
        }
        let n = i - st;
        if n >= 2 && matches!(b.get(i), None | Some(b' ') | Some(b'\t')) {
            out.push_str(&s[pos..st]);
            let pair = if c == b':' { "{two-colons}" } else { "{two-semicolons}" };
            for _ in 0..n / 2 {
                out.push_str(pair);
            }
            if n % 2 == 1 {
                out.push(c as char);
            }
            pos = i;
        }
    }
    out.push_str(&s[pos..]);
    out
}

fn text_part(s: &str, sep: Option<char>, nested: bool) -> String {
    let t = inert_dlist(&plain(s));
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

/// The inline mark characters (`*`, `_`, backtick, `#`, `^`, `~`, `+`).
const LONE_MARKS: &[char] = &['*', '_', '`', '#', '^', '~', '+'];

/// The mark characters the codes of a translation open or close with.
fn code_mark_chars(code: &bilingua_model::CodeData, into: &mut Vec<char>) {
    let mut add = |c: Option<char>| {
        if let Some(c) = c.filter(|c| LONE_MARKS.contains(c)) {
            if !into.contains(&c) {
                into.push(c);
            }
        }
    };
    add(code.open.as_deref().and_then(|m| m.chars().next_back()));
    add(code.close.as_deref().and_then(|m| m.chars().next()));
    if let Some(raw) = code.standalone.as_deref().filter(|r| constrained_point(r)) {
        add(raw.chars().next());
    }
}

/// A lone mark character in translated text that could open or close formatting with a mark of a code further
/// along (or before it) is written as a character reference, which prints the same: the line is then formatted as
/// the translator typed it and the codes stay the codes. Marks that cannot be a delimiter (`5 * 3`, `a*b`) are left.
/// A mark right next to a code's own mark was handled when its text was written.
fn shield_marks(out: &mut [String], texts: &[usize], marks: &[char]) {
    if marks.is_empty() {
        return;
    }
    for &k in texts {
        let chars: Vec<char> = out[k].chars().collect();
        let before_piece = k.checked_sub(1).and_then(|j| out[j].chars().next_back());
        let after_piece = out.get(k + 1).and_then(|p| p.chars().next());
        // a mark character right against a code's mark of another character would end that mark's run (`*p*#`)
        let beside = |c: char, other: Option<char>| LONE_MARKS.contains(&c) && other.is_some_and(|o| o != c && LONE_MARKS.contains(&o));
        let edges = chars.first().is_some_and(|&c| beside(c, before_piece)) || chars.last().is_some_and(|&c| beside(c, after_piece));
        if !edges && !chars.iter().any(|c| marks.contains(c)) {
            continue;
        }
        let mut piece = String::with_capacity(out[k].len() + 8);
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            // a character reference (written by an earlier rule, or typed) is kept whole
            if c == '&' && chars.get(i + 1) == Some(&'#') {
                if let Some(len) = chars[i..].iter().take(12).position(|&x| x == ';') {
                    piece.extend(&chars[i..=i + len]);
                    i += len + 1;
                    continue;
                }
            }
            let edge = (i == 0 && beside(c, before_piece)) || (i + 1 == chars.len() && beside(c, after_piece));
            if edge {
                piece.push_str(&format!("&#{};", c as u32));
                i += 1;
                continue;
            }
            if !marks.contains(&c) {
                piece.push(c);
                i += 1;
                continue;
            }
            let prev = if i > 0 { Some(chars[i - 1]) } else { before_piece };
            let next = if i + 1 < chars.len() { Some(chars[i + 1]) } else { after_piece };
            // next to a code's own mark of the same character: the rule for that case already ran
            let at_code = (i == 0 && prev == Some(c)) || (i + 1 == chars.len() && next == Some(c));
            let run = (i > 0 && prev == Some(c)) || (i + 1 < chars.len() && next == Some(c));
            let opens = prev.is_none_or(|p| !is_wordish(p)) && next.is_some_and(|n| !is_space(n));
            let closes = prev.is_some_and(|p| !is_space(p)) && next.is_none_or(|n| !is_wordish(n));
            if !at_code && (run || opens || closes) {
                piece.push_str(&format!("&#{};", c as u32));
            } else {
                piece.push(c);
            }
            i += 1;
        }
        out[k] = piece;
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
    let mut texts: Vec<usize> = Vec::new();
    let mut code_marks: Vec<char> = Vec::new();
    let mut prev_text = false;
    let markup = |i: Option<&Inline>| -> Option<&str> {
        let Some(Inline::Tag { id, kind, .. }) = i else { return None };
        let c = unit.codes.get(id)?;
        match kind {
            TagKind::Open => c.open.as_deref(),
            TagKind::Close => c.close.as_deref(),
            TagKind::Standalone => c.standalone.as_deref(),
        }
    };
    // the character the source has right before (`true`) or after a code, to keep what the source already had
    let src = unit.source();
    let src_side = |tag: Option<&Inline>, before: bool| -> Option<char> {
        let Some(Inline::Tag { id, kind, .. }) = tag else { return None };
        let at = src.iter().position(|x| matches!(x, Inline::Tag { id: i, kind: k, .. } if i == id && k == kind))?;
        let side = if before { at.checked_sub(1).and_then(|j| src.get(j)) } else { src.get(at + 1) };
        match side {
            Some(Inline::Text(s)) if before => s.chars().next_back(),
            Some(Inline::Text(s)) => s.chars().next(),
            _ => None,
        }
    };
    for (k, inl) in t.iter().enumerate() {
        match inl {
            Inline::Text(s) => {
                let mut piece = text_part(s, sep, nested);
                // a translated mark character next to a code's own mark of the same character would join its run
                // (`*p*` + `*` reads as `*p**`): it is written as a character reference, which prints the same
                // (structure fuzz). What the source already had next to the code is kept.
                const MARKS: &[char] = &['*', '_', '`', '#', '+', '^', '~'];
                let prev = k.checked_sub(1).and_then(|j| t.get(j));
                if let Some(c) = markup(prev).and_then(|m| m.chars().last()).filter(|c| MARKS.contains(c)) {
                    if piece.starts_with(c) && src_side(prev, false) != Some(c) {
                        piece.replace_range(..1, &format!("&#{};", c as u32));
                    }
                }
                let next = t.get(k + 1);
                if let Some(c) = markup(next).and_then(|m| m.chars().next()).filter(|c| MARKS.contains(c)) {
                    // a `]` right before a mark makes a translated `[...]` the mark's attribute list
                    if let Some(last) = piece.chars().next_back().filter(|&l| l == c || (l == ']' && piece.contains('['))) {
                        if src_side(next, true) != Some(last) {
                            piece.replace_range(piece.len() - 1.., &format!("&#{};", last as u32));
                        }
                    }
                }
                texts.push(out.len());
                out.push(piece);
                prev_text = true;
            }
            Inline::Tag { id, kind, .. } => {
                if !prev_text {
                    out.push(String::new());
                }
                prev_text = false;
                let Some(code) = unit.codes.get(id) else { continue };
                code_mark_chars(code, &mut code_marks);
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
    // a mark in two pieces of text has a code between them, which the pair would wrap (`` `*p` ``) and swallow
    for &m in LONE_MARKS {
        if !code_marks.contains(&m) && texts.iter().filter(|&&k| out[k].contains(m)).count() > 1 {
            code_marks.push(m);
        }
    }
    shield_marks(&mut out, &texts, &code_marks);
    // a character reference written above opens a `#` mark of its own (`&#42;`), which a lone `#` further on would close
    if !code_marks.contains(&'#') && texts.iter().any(|&k| out[k].contains("&#")) {
        shield_marks(&mut out, &texts, &['#']);
    }
    unconstrain(&mut out, &marks);
    explicit_links(&mut out, &links, attrs);
    out.concat()
}

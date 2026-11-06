//! The HTML scanner the server-page filters sit on: a port of LocOS `html_filter` (tokenizer, block units with
//! inline tags, `translate="no"`, accessibility and metadata attribute units, the target language declaration).
//! Offsets are byte offsets; every delimiter the scanner looks at is ASCII, so they always fall on characters.

use crate::pyx::*;
use std::borrow::Cow;
use std::collections::HashMap;

pub const SKIP_CONTAINERS: [&str; 2] = ["script", "style"];

/// What a token is. `Island` and `InlineIsland` are masked server code (markup tokens that are not tags).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum K {
    Text,
    Markup,
    Island,
    InlineIsland,
}

#[derive(Clone, Debug)]
pub struct Tok<'a> {
    pub k: K,
    pub s: Cow<'a, str>,
}

impl Tok<'_> {
    /// LocOS `_is_tag_token`: markup of any kind, code islands included.
    pub fn is_tag(&self) -> bool {
        self.k != K::Text
    }
}

/// The scan budget ran out: far more markup than the segment limit allows.
#[derive(Debug)]
pub struct BudgetExceeded;

pub fn is_inline_tag(n: &str) -> bool {
    matches!(
        n,
        "a" | "abbr"
            | "b"
            | "bdi"
            | "bdo"
            | "big"
            | "cite"
            | "code"
            | "data"
            | "dfn"
            | "em"
            | "font"
            | "i"
            | "ins"
            | "del"
            | "kbd"
            | "label"
            | "mark"
            | "output"
            | "q"
            | "rp"
            | "rt"
            | "ruby"
            | "s"
            | "samp"
            | "small"
            | "span"
            | "strong"
            | "sub"
            | "sup"
            | "time"
            | "tt"
            | "u"
            | "var"
            | "br"
            | "img"
            | "wbr"
    )
}

fn is_void_inline(n: &str) -> bool {
    matches!(n, "br" | "img" | "wbr")
}

fn is_void_element(n: &str) -> bool {
    matches!(
        n,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
            | "basefont"
            | "bgsound"
            | "frame"
            | "keygen"
    )
}

pub fn closes_p(n: &str) -> bool {
    matches!(
        n,
        "address"
            | "article"
            | "aside"
            | "blockquote"
            | "details"
            | "dialog"
            | "div"
            | "dl"
            | "fieldset"
            | "figcaption"
            | "figure"
            | "footer"
            | "form"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "header"
            | "hgroup"
            | "hr"
            | "main"
            | "menu"
            | "nav"
            | "ol"
            | "p"
            | "pre"
            | "section"
            | "summary"
            | "table"
            | "ul"
    )
}

/// (start names, names they close, barrier names) of LocOS `_IMPLIED_END`.
type ImpliedEnd = (&'static [&'static str], &'static [&'static str], &'static [&'static str]);
const IMPLIED_END: [ImpliedEnd; 8] = [
    (&["li"], &["li"], &["ul", "ol", "menu"]),
    (&["dt", "dd"], &["dt", "dd"], &["dl"]),
    (&["td", "th"], &["td", "th"], &["tr", "table"]),
    (&["tr"], &["tr"], &["table", "thead", "tbody", "tfoot"]),
    (&["thead", "tbody", "tfoot"], &["thead", "tbody", "tfoot"], &["table"]),
    (&["option"], &["option"], &["select", "datalist", "optgroup"]),
    (&["optgroup"], &["optgroup", "option"], &["select"]),
    (&["rt", "rp"], &["rt", "rp"], &["ruby"]),
];

fn is_tag_ws(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | 0x0c | b'\r')
}

/// LocOS `_parse_tag`: (closing, lower-case name, self closing) of a tag-shaped token; no name for anything else.
pub fn parse_tag_str(s: &str) -> (bool, String, bool) {
    let b = s.as_bytes();
    if b.first() != Some(&b'<') {
        return (false, String::new(), false);
    }
    let closing = b.get(1) == Some(&b'/');
    let i = if closing { 2 } else { 1 };
    if !b.get(i).is_some_and(|c| c.is_ascii_alphabetic()) {
        return (false, String::new(), false);
    }
    let mut e = i + 1;
    while e < b.len() && !matches!(b[e], b'\t' | b'\n' | 0x0c | b'\r' | b' ' | b'/' | b'>') {
        e += 1;
    }
    let name = s[i..e].to_lowercase();
    let sc = trim_end(s.trim_end_matches('>')).ends_with('/');
    (closing, name, sc)
}

pub fn parse_tag(t: &Tok) -> (bool, String, bool) {
    match t.k {
        K::Markup => parse_tag_str(&t.s),
        _ => (false, String::new(), false),
    }
}

// --- the scanner ----------------------------------------------------------------------------------------

fn tag_end(b: &[u8], pos: usize, budget: &mut Option<i64>) -> Result<Option<usize>, BudgetExceeded> {
    let mut quote = 0u8;
    let mut value_start = false;
    let mut unquoted = false;
    let declaration = matches!(b[pos + 1], b'!' | b'?');
    let n = b.len();
    let mut i = pos + 1;
    while i < n {
        if let Some(bd) = budget {
            *bd -= 1;
            if *bd < 0 {
                return Err(BudgetExceeded);
            }
        }
        let c = b[i];
        if quote != 0 {
            match b[i..].iter().position(|&x| x == quote) {
                None => return Ok(None),
                Some(p) => {
                    quote = 0;
                    i += p + 1;
                    continue;
                }
            }
        } else if c == b'>' {
            return Ok(Some(i + 1));
        } else if (c == b'"' || c == b'\'') && (value_start || declaration) {
            quote = c;
            value_start = false;
        } else if c == b'=' && !unquoted {
            value_start = true;
        } else if is_tag_ws(c) {
            unquoted = false;
            while i < n && is_tag_ws(b[i]) {
                i += 1;
            }
            continue;
        } else {
            unquoted = unquoted || value_start;
            value_start = false;
            let mut j = i;
            while j < n && !matches!(b[j], b'>' | b'"' | b'\'' | b'=') && !is_tag_ws(b[j]) {
                j += 1;
            }
            if j > i {
                i = j;
                continue;
            }
        }
        i += 1;
    }
    Ok(None)
}

/// LocOS `_raw_end`: where the raw-text body of a script or style element ends (the index of its end tag).
pub fn raw_end(b: &[u8], pos: usize, name: &str) -> usize {
    #[derive(PartialEq, Clone, Copy)]
    enum St {
        Data,
        Escaped,
        Double,
    }
    let n = b.len();
    let nb = name.as_bytes();
    let mut state = St::Data;
    let mut i = pos;
    let mut next_lt: Option<usize> = None; // cached index of the next "<" at or after i (None = unknown)
    while i < n {
        let c = b[i];
        if c != b'<' && (state == St::Data || !b[i..].starts_with(b"-->")) {
            let lt = match next_lt {
                Some(x) if x >= i => x,
                _ => {
                    let x = b[i..].iter().position(|&c| c == b'<').map_or(n, |p| p + i);
                    next_lt = Some(x);
                    x
                }
            };
            let mut nxt = lt;
            if state != St::Data {
                let hi = (nxt + 2).min(n);
                if let Some(p) = b[i..hi].windows(3).position(|w| w == b"-->") {
                    if i + p < nxt {
                        nxt = i + p;
                    }
                }
            }
            i = nxt;
            continue;
        }
        if name == "script" && b[i..].starts_with(b"<!--") && state == St::Data {
            state = St::Escaped;
            i += 2;
            continue;
        }
        if state != St::Data && b[i..].starts_with(b"-->") {
            state = St::Data;
            i += 3;
            continue;
        }
        if b[i] == b'<' {
            let closing = b[i..].starts_with(b"</");
            let start = i + if closing { 2 } else { 1 };
            let end = start + nb.len();
            if end < n && b[start..end].eq_ignore_ascii_case(nb) && matches!(b[end], b' ' | b'\t' | b'\n' | b'\r' | 0x0c | b'/' | b'>') {
                if closing {
                    if state != St::Double {
                        return i;
                    }
                    state = St::Escaped;
                } else if name == "script" && state == St::Escaped {
                    state = St::Double;
                }
                i = end;
                continue;
            }
        }
        i += 1;
    }
    n
}

/// LocOS `_tokenize` with no template pattern: tags own their quoted attributes, comments and CDATA own their
/// whole contents, script and style bodies are opaque text. Tokens alternate text, markup, text, markup, ...
pub fn tokenize<'a>(text: &'a str, budget: &mut Option<i64>) -> Result<Vec<Tok<'a>>, BudgetExceeded> {
    tokenize_with(text, None, budget)
}

/// The spans of `template` in `text` (LocOS `{m.start(): m.end() for m in template_pattern.finditer(text)}`).
/// An empty match is left out: it cannot hide a `<`, and LocOS would loop on it forever.
fn template_spans(text: &str, template: Option<&regex::Regex>) -> (HashMap<usize, usize>, Vec<usize>) {
    let mut spans = HashMap::new();
    let mut starts = Vec::new();
    if let Some(re) = template {
        for m in re.find_iter(text) {
            if m.end() > m.start() {
                spans.insert(m.start(), m.end());
                starts.push(m.start());
            }
        }
    }
    (spans, starts)
}

/// LocOS `_tokenize(text, template_pattern)`: as [`tokenize`], and a template span (a `{{ ... }}` of a template
/// language) that starts in ordinary text stays in that text, so a `<` inside it never starts a tag.
pub fn tokenize_with<'a>(text: &'a str, template: Option<&regex::Regex>, budget: &mut Option<i64>) -> Result<Vec<Tok<'a>>, BudgetExceeded> {
    let b = text.as_bytes();
    let size = b.len();
    let (templates, starts) = template_spans(text, template);
    let mut tokens: Vec<Tok<'a>> = Vec::new();
    let mut pos = 0;
    let mut i = 0;
    while i < size {
        if let Some(&e) = templates.get(&i) {
            i = e;
            continue;
        }
        if b[i] != b'<' || i + 1 == size {
            let mut nxt = b[i + 1..].iter().position(|&c| c == b'<').map_or(size, |p| p + i + 1);
            let k = starts.partition_point(|&s| s < i + 1);
            if k < starts.len() && starts[k] < nxt {
                nxt = starts[k];
            }
            i = nxt;
            continue;
        }
        let mut start = i + 1;
        if b[start] == b'/' {
            start += 1;
        }
        if start == size || !(b[start].is_ascii_alphabetic() || (start == i + 1 && matches!(b[start], b'!' | b'?'))) {
            i += 1;
            continue;
        }
        let end;
        if b[i..].starts_with(b"<!--") || b[i..].starts_with(b"<![CDATA[") {
            let (delim, skip): (&[u8], usize) = if b[i..].starts_with(b"<!--") { (b"-->", 4) } else { (b"]]>", 9) };
            end = find_bytes(b, i + skip, delim).map_or(size, |p| p + delim.len());
        } else {
            match tag_end(b, i, budget)? {
                Some(e) => end = e,
                None => break,
            }
        }
        tokens.push(Tok { k: K::Text, s: Cow::Borrowed(&text[pos..i]) });
        tokens.push(Tok { k: K::Markup, s: Cow::Borrowed(&text[i..end]) });
        let (closing, name, _sc) = parse_tag_str(&text[i..end]);
        i = end;
        pos = end;
        if !closing && SKIP_CONTAINERS.contains(&name.as_str()) {
            i = raw_end(b, pos, &name);
        }
    }
    tokens.push(Tok { k: K::Text, s: Cow::Borrowed(&text[pos..]) });
    Ok(tokens)
}

// --- attributes -----------------------------------------------------------------------------------------

/// LocOS `_ANY_ATTR_RE` over a start tag: every attribute, quoted or not, as (lower-case name, raw value) with the
/// first occurrence winning; a bare attribute has the empty string.
pub fn tag_attr_values(tok: &str) -> Vec<(String, String)> {
    let (closing, name, _) = parse_tag_str(tok);
    let _ = closing;
    if name.is_empty() {
        return Vec::new();
    }
    // the name regex `<(/?)([a-zA-Z][^\t\n\f\r />]*)` ends here
    let b = tok.as_bytes();
    let mut s = if b.get(1) == Some(&b'/') { 2 } else { 1 };
    s += 1;
    while s < b.len() && !matches!(b[s], b'\t' | b'\n' | 0x0c | b'\r' | b' ' | b'/' | b'>') {
        s += 1;
    }
    let end = if tok.ends_with('>') { tok.len() - 1 } else { tok.len() };
    let body = &tok[s.min(end)..end];
    let mut out: Vec<(String, String)> = Vec::new();
    let mut i = 0;
    let bad_name = |c: char| is_space(c) || matches!(c, '"' | '\'' | '>' | '/' | '=');
    while i < body.len() {
        let rest = &body[i..];
        let nlen = rest.find(bad_name).unwrap_or(rest.len());
        if nlen == 0 {
            i += rest.chars().next().map_or(1, char::len_utf8);
            continue;
        }
        let name = rest[..nlen].to_lowercase();
        let mut j = i + nlen;
        let mut raw = String::new();
        // optional `\s*=\s*value`
        let after_ws = |from: usize| from + body[from..].len() - trim_start(&body[from..]).len();
        let k = after_ws(j);
        if body[k..].starts_with('=') {
            let v = after_ws(k + 1);
            let vr = &body[v..];
            if let Some(inner) = vr.strip_prefix('"').and_then(|r| r.find('"').map(|p| (&r[..p], 2 + p))) {
                raw = inner.0.to_string();
                j = v + inner.1;
            } else if let Some(inner) = vr.strip_prefix('\'').and_then(|r| r.find('\'').map(|p| (&r[..p], 2 + p))) {
                raw = inner.0.to_string();
                j = v + inner.1;
            } else {
                let l = vr.find(|c: char| is_space(c) || c == '>').unwrap_or(vr.len());
                if l > 0 {
                    raw = vr[..l].to_string();
                    j = v + l;
                }
            }
        }
        if !out.iter().any(|(n, _)| *n == name) {
            out.push((name, unescape(&raw)));
        }
        i = j;
    }
    out
}

/// One quoted attribute of a start tag: (lower-case name, value start, value end, raw value), LocOS `_tag_attrs`.
pub fn tag_attrs(tok: &str) -> Vec<(String, usize, usize, String)> {
    let b = tok.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if !tok.is_char_boundary(i) {
            i += 1;
            continue;
        }
        let prev_ok = i > 0 && {
            let p = tok[..i].chars().next_back().unwrap_or(' ');
            is_space(p) || p == '"' || p == '\''
        };
        if !(prev_ok && (c.is_ascii_alphabetic() || c == b'_' || c == b':')) {
            i += 1;
            continue;
        }
        let mut e = i + 1;
        while e < b.len() && (b[e].is_ascii_alphanumeric() || matches!(b[e], b'-' | b'_' | b':' | b'.')) {
            e += 1;
        }
        let skip_ws = |from: usize| from + tok[from..].len() - trim_start(&tok[from..]).len();
        let k = skip_ws(e);
        if !tok[k..].starts_with('=') {
            i = e;
            continue;
        }
        let v = skip_ws(k + 1);
        let q = match tok.as_bytes().get(v) {
            Some(&q @ (b'"' | b'\'')) => q,
            _ => {
                i = e;
                continue;
            }
        };
        let Some(p) = b[v + 1..].iter().position(|&x| x == q) else {
            i = e;
            continue;
        };
        let (vs, ve) = (v + 1, v + 1 + p);
        out.push((tok[i..e].to_lowercase(), vs, ve, tok[vs..ve].to_string()));
        i = ve + 1;
    }
    out
}

// --- translate="no" -------------------------------------------------------------------------------------

fn own_translate(tok: &str) -> Option<bool> {
    if !contains_ci(tok.as_bytes(), b"translate") {
        return None;
    }
    let attrs = tag_attr_values(tok);
    let get = |n: &str| attrs.iter().find(|(k, _)| k == n).map(|(_, v)| v.as_str());
    if let Some(v) = get("translate") {
        if v.is_ascii() {
            match v.to_ascii_lowercase().as_str() {
                "" | "yes" => return Some(true),
                "no" => return Some(false),
                _ => {}
            }
        }
    }
    if get("class").is_some_and(|c| c.split(is_space).any(|w| w == "notranslate")) {
        return Some(false);
    }
    None
}

/// LocOS `_TranslateScope`: the inherited translate mode across a token stream.
pub struct TranslateScope {
    stack: Vec<(String, bool)>,
    open: HashMap<String, i64>,
}

impl Default for TranslateScope {
    fn default() -> Self {
        Self::new()
    }
}

impl TranslateScope {
    pub fn new() -> Self {
        TranslateScope { stack: Vec::new(), open: HashMap::new() }
    }

    pub fn current(&self) -> bool {
        self.stack.last().is_none_or(|s| s.1)
    }

    fn cut(&mut self, i: usize) {
        for (name, _) in &self.stack[i..] {
            if let Some(c) = self.open.get_mut(name) {
                *c -= 1;
            }
        }
        self.stack.truncate(i);
    }

    /// `barriers` None = the inline elements.
    fn pop_to(&mut self, names: &[&str], barriers: Option<&[&str]>) {
        if !names.iter().any(|n| self.open.get(*n).copied().unwrap_or(0) > 0) {
            return;
        }
        let len = self.stack.len();
        let low = len.saturating_sub(64);
        for i in (low..len).rev() {
            let name = self.stack[i].0.as_str();
            if names.contains(&name) {
                self.cut(i);
                return;
            }
            let stop = match barriers {
                None => !is_inline_tag(name),
                Some(bs) => bs.contains(&name),
            };
            if stop {
                return;
            }
        }
    }

    /// Takes every tag token outside script and style in order; returns the mode of that tag.
    pub fn feed(&mut self, tok: &str, closing: bool, name: &str, self_closing: bool) -> bool {
        if name.is_empty() || tok.starts_with("<!") || tok.starts_with("<?") {
            return self.current();
        }
        if closing {
            if self.open.get(name).copied().unwrap_or(0) <= 0 {
                return self.current();
            }
            for i in (0..self.stack.len()).rev() {
                if self.stack[i].0 == name {
                    let mode = self.stack[i].1;
                    self.cut(i);
                    return mode;
                }
            }
            return self.current();
        }
        if closes_p(name) {
            self.pop_to(&["p"], None);
        }
        for (starts, closes, barriers) in IMPLIED_END.iter() {
            if starts.contains(&name) {
                self.pop_to(closes, Some(barriers));
            }
        }
        let own = own_translate(tok);
        let mode = own.unwrap_or_else(|| self.current());
        if !(self_closing || is_void_element(name)) {
            self.stack.push((name.to_string(), mode));
            *self.open.entry(name.to_string()).or_insert(0) += 1;
        }
        mode
    }
}

/// True when any token mentions `translate` (otherwise every element is translatable and no tracking is needed).
fn mentions_translate(tokens: &[Tok]) -> bool {
    tokens.iter().any(|t| contains_ci(t.s.as_bytes(), b"translate"))
}

// --- body units -----------------------------------------------------------------------------------------

/// One piece of a unit's text, in order.
#[derive(Clone, Debug)]
pub enum Piece<'t> {
    Text(&'t str),
    Open(u32),
    Close(u32),
    Point(u32),
}

/// The original markup behind one inline token (LocOS `inline_map` entry).
#[derive(Clone, Debug, Default)]
pub struct Entry {
    pub open: Option<String>,
    pub close: Option<String>,
    pub point: Option<String>,
    /// The point is a server-code output expression (`_Island`).
    pub island: bool,
    /// Name of the element for an open/point entry, for the tag label.
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct BodyUnit<'t> {
    /// Inclusive token span.
    pub start: usize,
    pub end: usize,
    pub lead: String,
    pub trail: String,
    pub parts: Vec<Piece<'t>>,
    /// Entry of token id `n` is `map[n - 1]`.
    pub map: Vec<Entry>,
}

/// LocOS `_iter_units`. `cap`: refuse as soon as the unit count passes it.
pub fn iter_units<'t>(tokens: &'t [Tok<'_>], cap: Option<usize>) -> Result<Vec<BodyUnit<'t>>, usize> {
    struct St<'t> {
        parts: Vec<Piece<'t>>,
        indices: Vec<usize>,
        map: Vec<Entry>,
        stack: Vec<(u32, String)>,
        run_id: u32,
    }
    impl<'t> St<'t> {
        fn new() -> Self {
            St { parts: Vec::new(), indices: Vec::new(), map: Vec::new(), stack: Vec::new(), run_id: 0 }
        }
        fn counter(&self) -> u32 {
            self.map.len() as u32
        }
        fn protect(&mut self, ti: usize, tok: &str) {
            if self.run_id != 0 {
                if let Some(p) = self.map[self.run_id as usize - 1].point.as_mut() {
                    p.push_str(tok);
                }
            } else {
                self.map.push(Entry { point: Some(tok.to_string()), ..Default::default() });
                self.run_id = self.counter();
                self.parts.push(Piece::Point(self.run_id));
            }
            self.indices.push(ti);
        }
    }
    let mut units: Vec<BodyUnit<'t>> = Vec::new();
    let mut st = St::new();
    let mut skip_depth = false;
    let mut skip_name = String::new();
    let mut scope = if mentions_translate(tokens) { Some(TranslateScope::new()) } else { None };

    fn flush<'t>(st: &mut St<'t>, units: &mut Vec<BodyUnit<'t>>, cap: Option<usize>) -> Result<(), usize> {
        st.run_id = 0;
        if !st.indices.is_empty() {
            let mut parts = std::mem::take(&mut st.parts);
            // split off the surrounding whitespace, as str.strip does on the joined text
            let mut lead = String::new();
            let mut front = 0;
            while front < parts.len() {
                if let Piece::Text(t) = parts[front] {
                    let tl = trim_start(t);
                    lead.push_str(&t[..t.len() - tl.len()]);
                    if tl.is_empty() {
                        front += 1;
                        continue;
                    }
                    parts[front] = Piece::Text(tl);
                }
                break;
            }
            let mut back = parts.len();
            let mut trail_rev: Vec<&str> = Vec::new();
            while back > front {
                if let Piece::Text(t) = parts[back - 1] {
                    let tl = trim_end(t);
                    trail_rev.push(&t[tl.len()..]);
                    if tl.is_empty() {
                        back -= 1;
                        continue;
                    }
                    parts[back - 1] = Piece::Text(tl);
                }
                break;
            }
            let core: Vec<Piece<'t>> = parts[front..back].to_vec();
            if !core.is_empty() {
                let mut plain = String::new();
                for p in &core {
                    if let Piece::Text(t) = p {
                        plain.push_str(t);
                    } else {
                        plain.push('\u{1}'); // a placeholder: never a letter
                    }
                }
                if translatable_text(&plain) {
                    let trail: String = trail_rev.iter().rev().copied().collect();
                    units.push(BodyUnit {
                        start: st.indices[0],
                        end: *st.indices.last().unwrap_or(&0),
                        lead,
                        trail,
                        parts: core,
                        map: std::mem::take(&mut st.map),
                    });
                    if let Some(c) = cap {
                        if units.len() > c {
                            return Err(units.len());
                        }
                    }
                }
            }
        }
        st.parts.clear();
        st.indices.clear();
        st.map.clear();
        st.stack.clear();
        Ok(())
    }

    for (ti, tok) in tokens.iter().enumerate() {
        if tok.k == K::InlineIsland && !skip_depth {
            st.run_id = 0;
            st.map.push(Entry { point: Some(tok.s.to_string()), island: true, ..Default::default() });
            st.parts.push(Piece::Point(st.counter()));
            st.indices.push(ti);
            continue;
        }
        if tok.is_tag() {
            let (closing, name, self_closing) = parse_tag(tok);
            if skip_depth {
                if closing && name == skip_name {
                    skip_depth = false;
                }
                continue;
            }
            if SKIP_CONTAINERS.contains(&name.as_str()) {
                flush(&mut st, &mut units, cap)?;
                if !closing {
                    skip_depth = true;
                    skip_name = name;
                }
                continue;
            }
            let translate = match scope.as_mut() {
                Some(s) => s.feed(&tok.s, closing, &name, self_closing),
                None => true,
            };
            if is_inline_tag(&name) && !translate {
                st.protect(ti, &tok.s);
            } else if is_inline_tag(&name) {
                st.run_id = 0;
                if is_void_inline(&name) || self_closing {
                    st.map.push(Entry { point: Some(tok.s.to_string()), name, ..Default::default() });
                    st.parts.push(Piece::Point(st.counter()));
                    st.indices.push(ti);
                } else if closing {
                    if let Some(si) = st.stack.iter().rposition(|(_, n)| *n == name) {
                        let (oid, _) = st.stack.remove(si);
                        st.map[oid as usize - 1].close = Some(tok.s.to_string());
                        st.parts.push(Piece::Close(oid));
                        st.indices.push(ti);
                    }
                } else {
                    st.map.push(Entry { open: Some(tok.s.to_string()), name: name.clone(), ..Default::default() });
                    let id = st.counter();
                    st.stack.push((id, name));
                    st.parts.push(Piece::Open(id));
                    st.indices.push(ti);
                }
            } else {
                flush(&mut st, &mut units, cap)?;
            }
            continue;
        }
        if !skip_depth && !tok.s.is_empty() {
            if scope.as_ref().is_some_and(|s| !s.current()) {
                st.protect(ti, &tok.s);
            } else {
                st.run_id = 0;
                st.parts.push(Piece::Text(&tok.s));
                st.indices.push(ti);
            }
        }
    }
    flush(&mut st, &mut units, cap)?;
    Ok(units)
}

// --- attribute units ------------------------------------------------------------------------------------

/// Every start tag outside script and style, as (token index, name). `translatable_only`: leave out elements whose
/// translate mode is no.
pub fn iter_start_tags(tokens: &[Tok], translatable_only: bool) -> Vec<(usize, String)> {
    let mut scope = if translatable_only && mentions_translate(tokens) { Some(TranslateScope::new()) } else { None };
    let mut skip_name = String::new();
    let mut out = Vec::new();
    for (ti, tok) in tokens.iter().enumerate() {
        if !tok.is_tag() {
            continue;
        }
        let (closing, name, sc) = parse_tag(tok);
        if !skip_name.is_empty() {
            if closing && name == skip_name {
                skip_name.clear();
            }
            continue;
        }
        if SKIP_CONTAINERS.contains(&name.as_str()) {
            if !closing {
                skip_name = name;
            }
            continue;
        }
        if let Some(s) = scope.as_mut() {
            if !s.feed(&tok.s, closing, &name, sc) {
                continue;
            }
        }
        if closing || name.is_empty() || tok.s.starts_with("<!") {
            continue;
        }
        out.push((ti, name));
    }
    out
}

/// A translatable attribute: token index, value span inside the token, unescaped text, unit style.
#[derive(Clone, Debug)]
pub struct AttrUnit {
    pub ti: usize,
    pub start: usize,
    pub end: usize,
    pub text: String,
    pub style: String,
}

fn a11y_style(elem: &str, attr: &str) -> Option<&'static str> {
    match (elem, attr) {
        ("img" | "area", "alt") => Some("a11y:img-alt"),
        ("img" | "area", "title") => Some("a11y:img-title"),
        ("img" | "area", "aria-label") => Some("a11y:img-aria-label"),
        ("a", "title") => Some("a11y:link-title"),
        ("a", "aria-label") => Some("a11y:link-aria-label"),
        _ => None,
    }
}

const META_NAME_STYLES: [(&str, &str); 2] = [("description", "meta:description"), ("keywords", "meta:keywords")];
const META_SOCIAL_STYLES: [(&str, &str); 4] = [
    ("og:title", "meta:og-title"),
    ("og:description", "meta:og-description"),
    ("twitter:title", "meta:twitter-title"),
    ("twitter:description", "meta:twitter-description"),
];

/// LocOS `meta_text_style`: the unit style of a `<meta>` whose `content` is page text.
pub fn meta_text_style(attrs: &[(String, Option<String>)]) -> Option<&'static str> {
    let first = |n: &str| attrs.iter().find(|(k, _)| k == n).map(|(_, v)| v);
    if first("http-equiv").is_some() {
        return None;
    }
    let key = |n: &str| -> String { trim(&unescape(first(n).and_then(|v| v.as_deref()).unwrap_or(""))).to_lowercase() };
    let (name, prop) = (key("name"), key("property"));
    let find = |tbl: &[(&'static str, &'static str)], k: &str| tbl.iter().find(|(n, _)| *n == k).map(|(_, s)| *s);
    find(&META_NAME_STYLES, &name).or_else(|| find(&META_SOCIAL_STYLES, &prop)).or_else(|| find(&META_SOCIAL_STYLES, &name))
}

/// LocOS `_iter_page_attr_units`: the accessibility attributes, then the page metadata.
pub fn iter_page_attr_units(tokens: &[Tok]) -> Vec<AttrUnit> {
    iter_page_attr_units_with(tokens, true)
}

/// LocOS `_iter_page_attr_units(tokens, page_meta)`: the accessibility attributes, then (with `page_meta`) the page
/// metadata. Callers that join several pages into one document (EPUB chapters, e-mail parts) leave it off.
pub fn iter_page_attr_units_with(tokens: &[Tok], page_meta: bool) -> Vec<AttrUnit> {
    let mut out = Vec::new();
    let tags = iter_start_tags(tokens, true);
    for (ti, name) in &tags {
        if !matches!(name.as_str(), "img" | "area" | "a") {
            continue;
        }
        for (attr, start, end, raw) in tag_attrs(&tokens[*ti].s) {
            let Some(style) = a11y_style(name, &attr) else { continue };
            let core = trim(&unescape(&raw)).to_string();
            if core.is_empty() || !core.chars().any(is_alpha) {
                continue;
            }
            out.push(AttrUnit { ti: *ti, start, end, text: core, style: style.to_string() });
        }
    }
    for (ti, name) in &tags {
        if !page_meta || name != "meta" {
            continue;
        }
        let tok = &tokens[*ti].s;
        let attrs: Vec<(String, Option<String>)> = tag_attr_values(tok).into_iter().map(|(k, v)| (k, Some(v))).collect();
        let Some(style) = meta_text_style(&attrs) else { continue };
        let Some((_, start, end, raw)) = tag_attrs(tok).into_iter().find(|a| a.0 == "content") else { continue };
        let core = trim(&unescape(&raw)).to_string();
        if core.is_empty() || !core.chars().any(is_alpha) {
            continue;
        }
        out.push(AttrUnit { ti: *ti, start, end, text: core, style: style.to_string() });
    }
    out
}

// --- target language ------------------------------------------------------------------------------------

fn name_end(tag: &str) -> usize {
    let (closing, _, _) = parse_tag_str(tag);
    let b = tag.as_bytes();
    let mut e = if closing { 3 } else { 2 };
    while e < b.len() && !matches!(b[e], b'\t' | b'\n' | 0x0c | b'\r' | b' ' | b'/' | b'>') {
        e += 1;
    }
    e
}

/// LocOS `_set_attr`.
pub fn set_attr(tag: &str, attr: &str, value: &str, add_if_missing: bool) -> String {
    // `(\sATTR\s*=\s*)("[^"]*"|'[^']*'|[^\s>"']+)`, case-insensitive, first match
    let ab = attr.as_bytes();
    let tb = tag.as_bytes();
    let mut i = 0;
    while i < tag.len() {
        let c = tag[i..].chars().next().unwrap_or(' ');
        if is_space(c) {
            let a0 = i + c.len_utf8();
            if tb.len() >= a0 + ab.len() && tb[a0..a0 + ab.len()].eq_ignore_ascii_case(ab) {
                let skip_ws = |from: usize| from + tag[from..].len() - trim_start(&tag[from..]).len();
                let k = skip_ws(a0 + ab.len());
                if tag[k..].starts_with('=') {
                    let v = skip_ws(k + 1);
                    let r = &tag[v..];
                    let vlen = if let Some(x) = r.strip_prefix('"').and_then(|x| x.find('"')) {
                        Some(x + 2)
                    } else if let Some(x) = r.strip_prefix('\'').and_then(|x| x.find('\'')) {
                        Some(x + 2)
                    } else {
                        let l = r.find(|c: char| is_space(c) || matches!(c, '>' | '"' | '\'')).unwrap_or(r.len());
                        (l > 0).then_some(l)
                    };
                    if let Some(l) = vlen {
                        return format!("{}\"{}\"{}", &tag[..v], value, &tag[v + l..]);
                    }
                }
            }
        }
        i += c.len_utf8();
    }
    if !add_if_missing {
        return tag.to_string();
    }
    let e = name_end(tag);
    format!("{} {}=\"{}\"{}", &tag[..e], attr, value, &tag[e..])
}

/// LocOS `_declare_language`, in place over the token strings.
pub fn declare_language(tokens: &mut [Tok], tag: &str, direction: &str) {
    for (ti, name) in iter_start_tags(tokens, false) {
        let tok = tokens[ti].s.to_string();
        if name == "html" {
            let mut t = set_attr(&tok, "lang", tag, true);
            if find_ci(t.as_bytes(), 0, b"xml:lang").is_some() && has_attr_assign(&t, "xml:lang") {
                t = set_attr(&t, "xml:lang", tag, true);
            }
            if direction == "rtl" {
                t = set_attr(&t, "dir", "rtl", true);
            } else if direction == "ltr" {
                t = set_attr(&t, "dir", "ltr", false);
            }
            tokens[ti].s = Cow::Owned(t);
        } else if name == "meta" {
            let equiv = tag_attrs(&tok).into_iter().find(|a| a.0 == "http-equiv").map(|a| a.3).unwrap_or_default();
            if trim(&equiv).to_lowercase() == "content-language" {
                tokens[ti].s = Cow::Owned(set_attr(&tok, "content", tag, false));
            }
        }
    }
}

/// `re.search(r"\sNAME\s*=", tok, re.I)`.
fn has_attr_assign(tok: &str, name: &str) -> bool {
    let nb = name.as_bytes();
    let tb = tok.as_bytes();
    let mut from = 0;
    while let Some(p) = find_ci(tb, from, nb) {
        let before_ws = tok[..p].chars().next_back().is_some_and(is_space);
        if before_ws && trim_start(&tok[p + nb.len()..]).starts_with('=') {
            return true;
        }
        from = p + 1;
    }
    false
}

/// The BCP 47 form of a locale (language lower case, script title case, region upper case), or `None`.
pub fn html_lang(code: &str) -> Option<String> {
    let tag = trim(code).replace('_', "-");
    if tag.is_empty() || tag.len() > 64 {
        return None;
    }
    let parts: Vec<&str> = tag.split('-').collect();
    let first = parts[0];
    if !(2..=8).contains(&first.len()) || !first.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    if parts[1..].iter().any(|p| p.is_empty() || p.len() > 8 || !p.bytes().all(|b| b.is_ascii_alphanumeric())) {
        return None;
    }
    let mut out = vec![first.to_lowercase()];
    let mut extension = false;
    for sub in &parts[1..] {
        if sub.len() == 1 {
            extension = true;
            out.push(sub.to_lowercase());
        } else if extension {
            out.push(sub.to_lowercase());
        } else if sub.len() == 4 && sub.bytes().all(|b| b.is_ascii_alphabetic()) {
            let mut c = sub.chars();
            let f = c.next().map(|x| x.to_ascii_uppercase()).unwrap_or(' ');
            out.push(format!("{f}{}", c.as_str().to_lowercase()));
        } else if (sub.len() == 2 && sub.bytes().all(|b| b.is_ascii_alphabetic())) || (sub.len() == 3 && sub.bytes().all(|b| b.is_ascii_digit())) {
            out.push(sub.to_uppercase());
        } else {
            out.push(sub.to_lowercase());
        }
    }
    Some(out.join("-"))
}

/// "rtl" or "ltr" for a canonical tag (LocOS `text_direction`).
pub fn text_direction(tag: &str) -> &'static str {
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
            return if RTL_SCRIPTS.contains(&sub.to_lowercase().as_str()) { "rtl" } else { "ltr" };
        }
    }
    if RTL_LANGS.contains(&parts[0]) {
        "rtl"
    } else {
        "ltr"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &str) -> Vec<String> {
        tokenize(s, &mut None).unwrap().iter().map(|t| t.s.to_string()).collect()
    }

    #[test]
    fn tokenizer_shapes() {
        assert_eq!(toks("a<b>c</b>"), ["a", "<b>", "c", "</b>", ""]);
        assert_eq!(toks("<p a=\"x>y\">t</p>"), ["", "<p a=\"x>y\">", "t", "</p>", ""]);
        assert_eq!(toks("<script>if (a<b) {}</script>x"), ["", "<script>", "if (a<b) {}", "</script>", "x"]);
        assert_eq!(toks("<!-- c --><?x y?>"), ["", "<!-- c -->", "", "<?x y?>", ""]);
        assert_eq!(toks("a < b"), ["a < b"]);
    }

    #[test]
    fn units_with_inline_tags() {
        let t = tokenize("<p>Hello <b>big</b> <img src=a> world</p><p>2</p>", &mut None).unwrap();
        let u = iter_units(&t, None).unwrap();
        assert_eq!(u.len(), 2);
        assert_eq!(u[0].map.len(), 2);
    }

    #[test]
    fn attrs() {
        let a = tag_attrs("<img src=\"a\" alt='Hi there' title=\"\">");
        assert_eq!(a.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(), ["src", "alt", "title"]);
        assert_eq!(set_attr("<html lang=\"en\">", "lang", "fr", true), "<html lang=\"fr\">");
        assert_eq!(set_attr("<html>", "lang", "fr", true), "<html lang=\"fr\">");
        assert_eq!(set_attr("<html>", "dir", "ltr", false), "<html>");
        assert_eq!(html_lang("pt_br").as_deref(), None.or(Some("pt-BR")));
    }
}

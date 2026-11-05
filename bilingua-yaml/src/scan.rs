//! A YAML 1.1 reader that produces events with exact byte spans, so values can be edited in place.
//!
//! It follows libyaml/PyYAML on everything the filter depends on: where a scalar token starts and ends (anchor and
//! tag excluded; a block scalar runs to the start of the line after its last line, trailing blank lines included),
//! the decoded value, the style, the tag, and which inputs are errors. Aliases are never expanded.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    Plain,
    Single,
    Double,
    Literal,
    Folded,
}

#[derive(Debug, Clone)]
pub struct ScalarEv {
    pub start: usize,
    pub end: usize,
    pub value: String,
    pub style: Style,
    pub tag: Option<String>,
    /// False for the empty scalar of a missing value.
    pub token: bool,
}

#[derive(Debug, Clone)]
pub enum Ev {
    DocStart,
    Scalar(ScalarEv),
    Alias(String),
    MapStart { tag: Option<String>, flow: bool },
    MapEnd,
    SeqStart { tag: Option<String>, flow: bool },
    SeqEnd,
}

#[derive(Debug)]
pub struct ScanError {
    pub msg: String,
    pub pos: usize,
    pub limit: bool,
}

type R<T> = Result<T, ScanError>;

#[derive(Default, Clone)]
struct Props {
    tag: Option<String>,
    start: Option<usize>,
}

struct P<'a> {
    s: &'a str,
    b: &'a [u8],
    pos: usize,
    ev: Vec<Ev>,
    depth: usize,
    max_depth: usize,
    handles: Vec<String>,
}

pub fn scan(text: &str, max_depth: usize) -> R<Vec<Ev>> {
    let mut p = P { s: text, b: text.as_bytes(), pos: 0, ev: Vec::new(), depth: 0, max_depth, handles: Vec::new() };
    p.stream()?;
    Ok(p.ev)
}

fn is_flow_ind(c: u8) -> bool {
    matches!(c, b',' | b'[' | b']' | b'{' | b'}')
}

impl<'a> P<'a> {
    fn err<T>(&self, msg: &str) -> R<T> {
        Err(ScanError { msg: msg.to_string(), pos: self.pos, limit: false })
    }
    fn peek(&self) -> u8 {
        self.b.get(self.pos).copied().unwrap_or(0)
    }
    fn at(&self, i: usize) -> u8 {
        self.b.get(i).copied().unwrap_or(0)
    }
    fn eof(&self) -> bool {
        self.pos >= self.b.len()
    }
    fn blankz(&self, i: usize) -> bool {
        i >= self.b.len() || matches!(self.b[i], b' ' | b'\t' | b'\n' | b'\r')
    }
    fn is_break(c: u8) -> bool {
        c == b'\n' || c == b'\r'
    }
    fn eol(&self) -> bool {
        self.eof() || Self::is_break(self.peek())
    }
    fn line_start(&self, pos: usize) -> usize {
        self.b[..pos.min(self.b.len())].iter().rposition(|&c| c == b'\n' || c == b'\r').map_or(0, |i| i + 1)
    }
    fn col(&self, pos: usize) -> usize {
        pos - self.line_start(pos)
    }
    fn skip_spaces(&mut self) {
        while matches!(self.peek(), b' ' | b'\t') {
            self.pos += 1;
        }
    }
    fn skip_to_eol(&mut self) {
        while !self.eol() {
            self.pos += 1;
        }
    }
    fn consume_break(&mut self) {
        if self.peek() == b'\r' && self.at(self.pos + 1) == b'\n' {
            self.pos += 2;
        } else if Self::is_break(self.peek()) {
            self.pos += 1;
        }
    }
    fn doc_marker_at(&self, i: usize) -> bool {
        (i == 0 || Self::is_break(self.b[i - 1])) && (self.b[i..].starts_with(b"---") || self.b[i..].starts_with(b"...")) && self.blankz(i + 3)
    }
    fn empty(&mut self, tag: Option<String>) {
        self.ev.push(Ev::Scalar(ScalarEv { start: self.pos, end: self.pos, value: String::new(), style: Style::Plain, tag, token: false }));
    }
    fn enter(&mut self) -> R<()> {
        self.depth += 1;
        if self.depth > self.max_depth {
            return Err(ScanError { msg: "nested too deeply".into(), pos: self.pos, limit: true });
        }
        Ok(())
    }
    fn leave(&mut self) {
        self.depth -= 1;
    }

    /// Skips blanks, comments and line breaks to the next token. Returns true when a line break was crossed.
    fn skip_to_token(&mut self) -> R<bool> {
        let mut crossed = false;
        let mut line_start = self.pos == 0 || (self.pos > 0 && Self::is_break(self.b[self.pos - 1]));
        loop {
            while self.peek() == b' ' {
                self.pos += 1;
            }
            if self.peek() == b'\t' {
                while matches!(self.peek(), b' ' | b'\t') {
                    self.pos += 1;
                }
                if line_start && !(self.eol() || self.peek() == b'#') {
                    return self.err("found a tab character where an indentation space is expected");
                }
            }
            if self.peek() == b'#' {
                self.skip_to_eol();
            }
            if Self::is_break(self.peek()) {
                self.consume_break();
                crossed = true;
                line_start = true;
            } else {
                return Ok(crossed);
            }
        }
    }

    // ------------------------------------------------------------------ documents

    fn stream(&mut self) -> R<()> {
        let mut first = true;
        loop {
            self.skip_to_token()?;
            if self.eof() {
                return Ok(());
            }
            let mut directives = false;
            self.handles.clear();
            while self.peek() == b'%' && self.col(self.pos) == 0 {
                directives = true;
                let s0 = self.pos;
                self.skip_to_eol();
                let line = &self.s[s0..self.pos];
                if let Some(rest) = line.strip_prefix("%TAG") {
                    if let Some(h) = rest.split_whitespace().next() {
                        self.handles.push(h.to_string());
                    }
                }
                self.skip_to_token()?;
            }
            let explicit = self.col(self.pos) == 0 && self.b[self.pos..].starts_with(b"---") && self.blankz(self.pos + 3);
            if directives && !explicit {
                return self.err("did not find expected <document start>");
            }
            if self.doc_marker_at(self.pos) && !explicit {
                // a stray `...`: an empty document end
                if first {
                    return self.err("did not find expected <document start>");
                }
                self.skip_to_eol();
                first = false;
                continue;
            }
            if !explicit && !first {
                return self.err("expected '<document start>'");
            }
            self.ev.push(Ev::DocStart);
            if explicit {
                self.pos += 3;
                self.parse_block_value(-1, false, false)?;
            } else {
                self.parse_block_value(-1, true, false)?;
            }
            first = false;
            self.skip_to_token()?;
            if self.eof() {
                return Ok(());
            }
            if self.col(self.pos) == 0 && self.b[self.pos..].starts_with(b"...") && self.blankz(self.pos + 3) {
                self.skip_to_eol();
                continue;
            }
            if self.col(self.pos) == 0 && self.b[self.pos..].starts_with(b"---") && self.blankz(self.pos + 3) {
                continue;
            }
            return self.err("did not find expected <document start>");
        }
    }

    // ------------------------------------------------------------------ properties

    fn parse_props(&mut self) -> R<Props> {
        let mut p = Props::default();
        loop {
            match self.peek() {
                b'&' => {
                    p.start.get_or_insert(self.pos);
                    self.pos += 1;
                    let s = self.pos;
                    while self.peek().is_ascii_alphanumeric() || matches!(self.peek(), b'_' | b'-') {
                        self.pos += 1;
                    }
                    if self.pos == s || !(self.blankz(self.pos) || b"?:,]}%@`".contains(&self.peek())) {
                        return self.err("did not find expected alphabetic or numeric character (anchor)");
                    }
                }
                b'!' => {
                    p.start.get_or_insert(self.pos);
                    let s = self.pos;
                    if self.at(self.pos + 1) == b'<' {
                        while !self.eol() && self.peek() != b'>' {
                            self.pos += 1;
                        }
                        if self.peek() != b'>' {
                            return self.err("did not find the expected '>' of a verbatim tag");
                        }
                        self.pos += 1;
                    } else {
                        self.pos += 1;
                        while self.peek().is_ascii_alphanumeric() || (self.peek() != 0 && b";/?:@&=+$.!~*'()%-_".contains(&self.peek())) {
                            self.pos += 1;
                        }
                    }
                    let t = &self.s[s..self.pos];
                    if t == "!!" || (t.len() > 2 && t.ends_with('!') && !t.starts_with("!<")) {
                        return self.err("did not find expected tag URI");
                    }
                    if t.len() > 2 && !t.starts_with("!<") && !t.starts_with("!!") {
                        if let Some(i) = t[1..].find('!') {
                            let handle = &t[..i + 2];
                            if !self.handles.iter().any(|h| h == handle) {
                                return self.err("found undefined tag handle");
                            }
                        }
                    }
                    p.tag = Some(t.to_string());
                }
                _ => break,
            }
            if !(self.blankz(self.pos) || is_flow_ind(self.peek())) {
                return self.err("did not find expected whitespace after a node property");
            }
            let save = self.pos;
            self.skip_spaces();
            if !matches!(self.peek(), b'&' | b'!') {
                self.pos = save;
                break;
            }
        }
        Ok(p)
    }

    fn merge_tag(a: &Props, b: &Props) -> Option<String> {
        b.tag.clone().or_else(|| a.tag.clone())
    }

    // ------------------------------------------------------------------ block context

    /// Parses the node after an indicator. `n` is the indent of the enclosing block collection (-1 at the root);
    /// children need a column above it. `same_line_ok`: a block collection may start on this line (after `-`,
    /// `?` or an explicit `:`). `seq_at_n_ok`: a block sequence may sit at column `n` (value of a mapping key).
    fn parse_block_value(&mut self, n: i32, same_line_ok: bool, seq_at_n_ok: bool) -> R<()> {
        self.skip_spaces();
        let mut nl_props = Props::default(); // properties that ended a line: they belong to the whole node
        let mut inline_props = Props::default();
        let mut crossed = false;
        loop {
            if crossed {
                let at_end = self.eof() || self.doc_marker_at(self.pos);
                let c = self.col(self.pos) as i32;
                let seq_here = self.peek() == b'-' && self.blankz(self.pos + 1);
                if at_end || (c <= n && !(seq_at_n_ok && c == n && seq_here)) {
                    let t = Self::merge_tag(&nl_props, &inline_props);
                    self.empty(t);
                    return Ok(());
                }
            }
            if matches!(self.peek(), b'&' | b'!') {
                let p = self.parse_props()?;
                self.skip_spaces();
                if self.eol() || self.peek() == b'#' {
                    nl_props.tag = p.tag.or(nl_props.tag);
                    nl_props.start.get_or_insert(p.start.unwrap_or(self.pos));
                    self.skip_to_token()?;
                    crossed = true;
                    continue;
                }
                inline_props = p;
                break;
            }
            if !crossed && (self.eol() || self.peek() == b'#') {
                self.skip_to_token()?;
                crossed = true;
                continue;
            }
            break;
        }
        let empty_tag = Self::merge_tag(&nl_props, &inline_props);
        if crossed {
            if self.eof() || self.doc_marker_at(self.pos) {
                self.empty(empty_tag);
                return Ok(());
            }
            let c = self.col(self.pos) as i32;
            let seq_here = self.peek() == b'-' && self.blankz(self.pos + 1);
            if c <= n && !(seq_at_n_ok && c == n && seq_here) {
                self.empty(empty_tag);
                return Ok(());
            }
        } else if self.eof() {
            self.empty(empty_tag);
            return Ok(());
        }
        let collections_ok = crossed || same_line_ok;
        let node_pos = inline_props.start.unwrap_or(self.pos);
        let node_col = self.col(node_pos);
        let c = self.peek();
        match c {
            b'-' if self.blankz(self.pos + 1) => {
                if !collections_ok || inline_props.start.is_some() {
                    return self.err("block sequence entries are not allowed in this context");
                }
                self.ev.push(Ev::SeqStart { tag: nl_props.tag, flow: false });
                self.block_seq(node_col)
            }
            b'?' if self.blankz(self.pos + 1) => {
                if !collections_ok || inline_props.start.is_some() {
                    return self.err("mapping keys are not allowed in this context");
                }
                self.ev.push(Ev::MapStart { tag: nl_props.tag, flow: false });
                self.block_map(node_col)
            }
            b'|' | b'>' => {
                let mut sc = self.block_scalar(n)?;
                sc.tag = Self::merge_tag(&nl_props, &inline_props);
                self.ev.push(Ev::Scalar(sc));
                Ok(())
            }
            _ => self.scalar_or_key(n, collections_ok, node_col, &nl_props, &inline_props),
        }
    }

    /// An alias, quoted/plain scalar or flow collection that may turn out to be the first key of a block mapping.
    fn scalar_or_key(&mut self, n: i32, collections_ok: bool, node_col: usize, nl: &Props, inl: &Props) -> R<()> {
        let save_pos = self.pos;
        let save_len = self.ev.len();
        let start_line = self.line_start(self.pos);
        let combined = Self::merge_tag(nl, inl);
        // Parse one token. `held` is a scalar not yet pushed (its tag depends on the key decision).
        let mut held: Option<ScalarEv> = None;
        let mut plain_first_line: Option<(usize, String, bool)> = None; // (end, value, ended by key colon)
        match self.peek() {
            b'*' => {
                if inl.start.is_some() {
                    return self.err("an alias cannot have properties");
                }
                let name = self.alias_name()?;
                self.ev.push(Ev::Alias(name));
            }
            b'[' | b'{' => {
                self.flow_collection()?;
            }
            b'\'' | b'"' => {
                held = Some(self.quoted()?);
            }
            _ => {
                let (end, value, stop_colon) = self.plain_first_line(false)?;
                plain_first_line = Some((end, value, stop_colon));
            }
        }
        let token_end = match (&held, &plain_first_line) {
            (Some(h), _) => h.end,
            (_, Some((e, _, _))) => *e,
            _ => self.pos,
        };
        let single_line = self.line_start(token_end.max(save_pos)) == start_line;
        // key?
        let after = {
            let mut i = if plain_first_line.is_some() { token_end } else { self.pos };
            while matches!(self.at(i), b' ' | b'\t') {
                i += 1;
            }
            i
        };
        let is_key = single_line && self.at(after) == b':' && self.blankz(after + 1);
        if is_key {
            if !collections_ok {
                self.pos = after;
                return self.err("mapping values are not allowed here");
            }
            self.ev.truncate(save_len);
            self.pos = inl.start.unwrap_or(save_pos);
            self.ev.push(Ev::MapStart { tag: nl.tag.clone(), flow: false });
            return self.block_map_with_first_props(node_col, inl);
        }
        // a value
        if nl.start.is_some() && matches!(self.ev.get(save_len), Some(Ev::Alias(_))) {
            return self.err("an alias cannot have properties");
        }
        if let Some((end, value, _)) = plain_first_line {
            let sc = self.plain_rest(save_pos, end, value, n, false)?;
            let mut sc = sc;
            sc.tag = combined;
            self.ev.push(Ev::Scalar(sc));
        } else if let Some(mut h) = held {
            h.tag = combined;
            self.ev.push(Ev::Scalar(h));
        } else if combined.is_some() {
            if let Some(Ev::SeqStart { tag, .. } | Ev::MapStart { tag, .. }) = self.ev.get_mut(save_len) {
                *tag = combined;
            }
        }
        self.finish_line()
    }

    fn finish_line(&mut self) -> R<()> {
        self.skip_spaces();
        if self.eol() || self.peek() == b'#' {
            return Ok(());
        }
        if self.peek() == b':' && self.blankz(self.pos + 1) {
            return self.err("mapping values are not allowed here");
        }
        self.err("did not find expected comment or line break")
    }

    fn alias_name(&mut self) -> R<String> {
        self.pos += 1;
        let s = self.pos;
        while self.peek().is_ascii_alphanumeric() || matches!(self.peek(), b'_' | b'-') {
            self.pos += 1;
        }
        if self.pos == s || !(self.blankz(self.pos) || b"?:,]}%@`".contains(&self.peek())) {
            return self.err("did not find expected alphabetic or numeric character (alias)");
        }
        Ok(self.s[s..self.pos].to_string())
    }

    fn block_seq(&mut self, m: usize) -> R<()> {
        self.enter()?;
        loop {
            self.pos += 1; // '-'
            self.parse_block_value(m as i32, true, false)?;
            self.skip_to_token()?;
            if self.eof() || self.doc_marker_at(self.pos) {
                break;
            }
            let c = self.col(self.pos);
            if c < m {
                break;
            }
            if c > m {
                return self.err("bad indentation of a sequence entry");
            }
            if !(self.peek() == b'-' && self.blankz(self.pos + 1)) {
                break;
            }
        }
        self.ev.push(Ev::SeqEnd);
        self.leave();
        Ok(())
    }

    fn block_map(&mut self, m: usize) -> R<()> {
        self.block_map_with_first_props(m, &Props::default())
    }

    fn block_map_with_first_props(&mut self, m: usize, _first: &Props) -> R<()> {
        self.enter()?;
        loop {
            self.map_entry(m)?;
            self.skip_to_token()?;
            if self.eof() || self.doc_marker_at(self.pos) {
                break;
            }
            let c = self.col(self.pos);
            if c < m {
                break;
            }
            if c > m {
                return self.err("bad indentation of a mapping entry");
            }
            if self.peek() == b'-' && self.blankz(self.pos + 1) {
                break;
            }
        }
        self.ev.push(Ev::MapEnd);
        self.leave();
        Ok(())
    }

    fn map_entry(&mut self, m: usize) -> R<()> {
        if self.peek() == b'?' && self.blankz(self.pos + 1) {
            self.pos += 1;
            self.parse_block_value(m as i32, true, false)?;
            self.skip_to_token()?;
            if !self.eof() && self.col(self.pos) == m && self.peek() == b':' && self.blankz(self.pos + 1) {
                self.pos += 1;
                self.parse_block_value(m as i32, true, false)?;
            } else {
                self.empty(None);
            }
            return Ok(());
        }
        // implicit key
        let props = if matches!(self.peek(), b'&' | b'!') {
            let p = self.parse_props()?;
            self.skip_spaces();
            p
        } else {
            Props::default()
        };
        let start_line = self.line_start(self.pos);
        let key_start = self.pos;
        match self.peek() {
            b'*' => {
                if props.start.is_some() {
                    return self.err("an alias cannot have properties");
                }
                let name = self.alias_name()?;
                self.ev.push(Ev::Alias(name));
            }
            b'[' | b'{' => {
                let at = self.ev.len();
                self.flow_collection()?;
                if let Some(Ev::SeqStart { tag, .. } | Ev::MapStart { tag, .. }) = self.ev.get_mut(at) {
                    *tag = props.tag.clone();
                }
            }
            b'\'' | b'"' => {
                let mut q = self.quoted()?;
                q.tag = props.tag.clone();
                self.ev.push(Ev::Scalar(q));
            }
            b'|' | b'>' => return self.err("a block scalar cannot be a mapping key"),
            c if Self::is_break(c) || c == 0 => return self.err("could not find expected ':'"),
            _ => {
                let (end, value, stop_colon) = self.plain_first_line(false)?;
                if !stop_colon {
                    self.pos = end;
                    return self.err("could not find expected ':'");
                }
                self.ev.push(Ev::Scalar(ScalarEv { start: key_start, end, value, style: Style::Plain, tag: props.tag.clone(), token: true }));
                self.pos = end;
            }
        }
        if self.line_start(self.pos) != start_line {
            return self.err("could not find expected ':' (key spans lines)");
        }
        self.skip_spaces();
        if !(self.peek() == b':' && self.blankz(self.pos + 1)) {
            return self.err("could not find expected ':'");
        }
        self.pos += 1;
        self.parse_block_value(m as i32, false, true)
    }

    // ------------------------------------------------------------------ plain scalars

    fn first_char_ok(&self, flow: bool) -> R<()> {
        let c = self.peek();
        let bad = matches!(c, b'%' | b'@' | b'`') || (flow && matches!(c, b',' | b']' | b'}')) || (!flow && matches!(c, b']' | b'}' | b','));
        if bad {
            return self.err("found character that cannot start any token");
        }
        if matches!(c, b'-' | b'?' | b':') && self.blankz(self.pos + 1) {
            return self.err("found character that cannot start any token");
        }
        Ok(())
    }

    /// Scans one line of a plain scalar from the cursor. Returns (end of text, text, stopped at a key colon) and
    /// leaves the cursor at the stop position.
    fn plain_first_line(&mut self, flow: bool) -> R<(usize, String, bool)> {
        self.first_char_ok(flow)?;
        let start = self.pos;
        let (end, stop, colon) = self.plain_line(start, flow);
        self.pos = stop;
        Ok((end, self.s[start..end].to_string(), colon))
    }

    /// (end of trimmed text, stop position, stopped at `:`)
    fn plain_line(&self, from: usize, flow: bool) -> (usize, usize, bool) {
        let mut i = from;
        let mut colon = false;
        while i < self.b.len() {
            let c = self.b[i];
            if Self::is_break(c) {
                break;
            }
            if c == b':' {
                let n = self.at(i + 1);
                if self.blankz(i + 1) || (flow && is_flow_ind(n)) {
                    colon = true;
                    break;
                }
            } else if (c == b'#' && i > from && matches!(self.b[i - 1], b' ' | b'\t')) || (flow && is_flow_ind(c)) {
                break;
            }
            i += 1;
        }
        let mut end = i;
        while end > from && matches!(self.b[end - 1], b' ' | b'\t') {
            end -= 1;
        }
        (end, i, colon)
    }

    /// Continues a plain scalar over further lines. `start` is the token start, `end`/`value` the first line.
    fn plain_rest(&mut self, start: usize, mut end: usize, mut value: String, n: i32, flow: bool) -> R<ScalarEv> {
        let mut stop = self.pos;
        // after the first line the cursor is at the stop position; continue only when the line ended at a break
        // (after trailing spaces)
        loop {
            let mut i = stop;
            while matches!(self.at(i), b' ' | b'\t') {
                i += 1;
            }
            if !(i < self.b.len() && Self::is_break(self.b[i])) {
                break; // comment, colon or end of input
            }
            // look at the following lines
            let mut j = i;
            let mut blanks = 0usize;
            let next;
            loop {
                // j at a break: move to the next line start
                if self.at(j) == b'\r' && self.at(j + 1) == b'\n' {
                    j += 2;
                } else {
                    j += 1;
                }
                let ls = j;
                while matches!(self.at(j), b' ' | b'\t') {
                    j += 1;
                }
                if j >= self.b.len() {
                    next = None;
                    break;
                }
                if Self::is_break(self.b[j]) {
                    blanks += 1;
                    continue;
                }
                next = Some((ls, j));
                break;
            }
            let Some((ls, j)) = next else { break };
            if !flow && ((j - ls) as i32) < n + 1 {
                break;
            }
            if self.doc_marker_at(ls) || self.b[j] == b'#' {
                break;
            }
            if flow && (is_flow_ind(self.b[j]) && matches!(self.b[j], b']' | b'}' | b',')) {
                break;
            }
            let (e2, s2, colon2) = self.plain_line(j, flow);
            if e2 == j {
                break;
            }
            if blanks == 0 {
                value.push(' ');
            } else {
                for _ in 0..blanks {
                    value.push('\n');
                }
            }
            value.push_str(&self.s[j..e2]);
            end = e2;
            stop = s2;
            self.pos = s2;
            if colon2 {
                break;
            }
        }
        self.pos = stop.max(end);
        Ok(ScalarEv { start, end, value, style: Style::Plain, tag: None, token: true })
    }

    // ------------------------------------------------------------------ quoted scalars

    fn quoted(&mut self) -> R<ScalarEv> {
        let start = self.pos;
        let q = self.peek();
        let double = q == b'"';
        self.pos += 1;
        let mut out = String::new();
        loop {
            let run = self.pos;
            while self.pos < self.b.len() {
                let c = self.b[self.pos];
                if c == q || (double && c == b'\\') || matches!(c, b' ' | b'\t' | b'\n' | b'\r') {
                    break;
                }
                self.pos += 1;
            }
            out.push_str(&self.s[run..self.pos]);
            if self.eof() {
                return self.err("found unexpected end of stream while scanning a quoted scalar");
            }
            let c = self.peek();
            if c == q {
                if !double && self.at(self.pos + 1) == b'\'' {
                    out.push('\'');
                    self.pos += 2;
                    continue;
                }
                self.pos += 1;
                break;
            } else if c == b'\\' {
                self.pos += 1;
                let e = self.peek();
                if Self::is_break(e) {
                    self.consume_break();
                    self.quoted_breaks(&mut out, false)?;
                    continue;
                }
                let simple = match e {
                    b'0' => Some('\0'),
                    b'a' => Some('\x07'),
                    b'b' => Some('\x08'),
                    b't' | b'\t' => Some('\t'),
                    b'n' => Some('\n'),
                    b'v' => Some('\x0b'),
                    b'f' => Some('\x0c'),
                    b'r' => Some('\r'),
                    b'e' => Some('\x1b'),
                    b' ' => Some(' '),
                    b'"' => Some('"'),
                    b'/' => Some('/'),
                    b'\\' => Some('\\'),
                    b'N' => Some('\u{85}'),
                    b'_' => Some('\u{a0}'),
                    b'L' => Some('\u{2028}'),
                    b'P' => Some('\u{2029}'),
                    _ => None,
                };
                if let Some(ch) = simple {
                    out.push(ch);
                    self.pos += 1;
                } else {
                    let len = match e {
                        b'x' => 2,
                        b'u' => 4,
                        b'U' => 8,
                        _ => return self.err("found unknown escape character"),
                    };
                    self.pos += 1;
                    let hex = self.s.get(self.pos..self.pos + len).filter(|h| h.bytes().all(|b| b.is_ascii_hexdigit()));
                    let Some(hex) = hex else { return self.err("did not find expected hexadecimal number") };
                    let Some(ch) = u32::from_str_radix(hex, 16).ok().and_then(char::from_u32) else {
                        return self.err("found invalid Unicode character escape code");
                    };
                    out.push(ch);
                    self.pos += len;
                }
            } else {
                // whitespace or break
                let ws = self.pos;
                while matches!(self.peek(), b' ' | b'\t') {
                    self.pos += 1;
                }
                if Self::is_break(self.peek()) {
                    self.consume_break();
                    self.quoted_breaks(&mut out, true)?;
                } else if self.eof() {
                    return self.err("found unexpected end of stream while scanning a quoted scalar");
                } else {
                    out.push_str(&self.s[ws..self.pos]);
                }
            }
        }
        Ok(ScalarEv { start, end: self.pos, value: out, style: if double { Style::Double } else { Style::Single }, tag: None, token: true })
    }

    /// After a line break inside a quoted scalar: folds it (`fold`) and swallows the next lines' indentation.
    fn quoted_breaks(&mut self, out: &mut String, fold: bool) -> R<()> {
        let mut breaks = 0;
        loop {
            if self.pos < self.b.len() && self.doc_marker_at(self.pos) {
                return self.err("found unexpected document separator");
            }
            while matches!(self.peek(), b' ' | b'\t') {
                self.pos += 1;
            }
            if Self::is_break(self.peek()) {
                self.consume_break();
                breaks += 1;
            } else {
                break;
            }
        }
        if fold && breaks == 0 {
            out.push(' ');
        }
        for _ in 0..breaks {
            out.push('\n');
        }
        Ok(())
    }

    // ------------------------------------------------------------------ block scalars

    fn block_scalar(&mut self, n: i32) -> R<ScalarEv> {
        let start = self.pos;
        let folded = self.peek() == b'>';
        self.pos += 1;
        let mut chomping: Option<bool> = None;
        let mut incr: Option<usize> = None;
        for _ in 0..2 {
            match self.peek() {
                b'+' | b'-' if chomping.is_none() => {
                    chomping = Some(self.peek() == b'+');
                    self.pos += 1;
                }
                b'0' => return self.err("found an indentation indicator equal to 0"),
                b'1'..=b'9' if incr.is_none() => {
                    incr = Some((self.peek() - b'0') as usize);
                    self.pos += 1;
                }
                _ => break,
            }
        }
        self.skip_spaces();
        if self.peek() == b'#' {
            self.skip_to_eol();
        }
        if !self.eol() {
            return self.err("did not find expected comment or line break");
        }
        self.consume_break();
        let min_indent = ((n + 1).max(1)) as usize;
        let mut ls = self.pos; // start of the current line
        let col = |p: &P, pos: usize, ls: usize| -> usize {
            let _ = p;
            pos - ls
        };
        let mut breaks: usize;
        let mut end = self.pos;
        let indent: usize;
        match incr {
            None => {
                let mut max_indent = 0;
                let mut lead_breaks = 0usize;
                loop {
                    let c = self.peek();
                    if c == b' ' {
                        self.pos += 1;
                        max_indent = max_indent.max(col(self, self.pos, ls));
                    } else if Self::is_break(c) {
                        self.consume_break();
                        ls = self.pos;
                        end = self.pos;
                        lead_breaks += 1;
                    } else {
                        break;
                    }
                }
                indent = min_indent.max(max_indent);
                breaks = lead_breaks;
            }
            Some(i) => {
                indent = min_indent + i - 1;
                breaks = self.block_breaks(indent, &mut ls, &mut end);
            }
        }
        let mut out = String::new();
        let mut line_break_s = String::new();
        while col(self, self.pos, ls) == indent && !self.eof() {
            for _ in 0..breaks {
                out.push('\n');
            }
            let leading_non_space = !matches!(self.peek(), b' ' | b'\t' | b'\r' | b'\n');
            let s0 = self.pos;
            while !self.eol() {
                self.pos += 1;
            }
            out.push_str(&self.s[s0..self.pos]);
            line_break_s.clear();
            if !self.eof() {
                self.consume_break();
                line_break_s.push('\n');
                ls = self.pos;
            }
            end = self.pos;
            breaks = self.block_breaks(indent, &mut ls, &mut end);
            if col(self, self.pos, ls) == indent && !self.eof() {
                let next_non_space = !matches!(self.peek(), b' ' | b'\t' | b'\r' | b'\n');
                if folded && !line_break_s.is_empty() && leading_non_space && next_non_space {
                    if breaks == 0 {
                        out.push(' ');
                    }
                } else {
                    out.push_str(&line_break_s);
                }
            } else {
                break;
            }
        }
        if chomping != Some(false) {
            out.push_str(&line_break_s);
        }
        if chomping == Some(true) {
            for _ in 0..breaks {
                out.push('\n');
            }
        }
        // the token ends where the last consumed break ended
        self.pos = end;
        Ok(ScalarEv { start, end, value: out, style: if folded { Style::Folded } else { Style::Literal }, tag: None, token: true })
    }

    /// PyYAML's scan_block_scalar_breaks: swallow indentation up to `indent` and blank lines; returns the number of
    /// breaks and moves `end` after the last one.
    fn block_breaks(&mut self, indent: usize, ls: &mut usize, end: &mut usize) -> usize {
        let mut count = 0;
        *end = self.pos;
        while self.pos - *ls < indent && self.peek() == b' ' {
            self.pos += 1;
        }
        while Self::is_break(self.peek()) {
            self.consume_break();
            *ls = self.pos;
            count += 1;
            *end = self.pos;
            while self.pos - *ls < indent && self.peek() == b' ' {
                self.pos += 1;
            }
        }
        count
    }

    // ------------------------------------------------------------------ flow context

    fn flow_skip(&mut self) -> R<()> {
        self.skip_to_token_flow()?;
        if !self.eof() && self.doc_marker_at(self.pos) {
            return self.err("found unexpected document indicator inside a flow collection");
        }
        Ok(())
    }

    fn skip_to_token_flow(&mut self) -> R<()> {
        loop {
            while matches!(self.peek(), b' ' | b'\t') {
                self.pos += 1;
            }
            if self.peek() == b'#' && (self.pos == 0 || matches!(self.b[self.pos - 1], b' ' | b'\t' | b'\n' | b'\r')) {
                self.skip_to_eol();
            }
            if Self::is_break(self.peek()) {
                self.consume_break();
            } else {
                return Ok(());
            }
        }
    }

    fn flow_collection(&mut self) -> R<()> {
        self.enter()?;
        let seq = self.peek() == b'[';
        let close = if seq { b']' } else { b'}' };
        self.ev.push(if seq { Ev::SeqStart { tag: None, flow: true } } else { Ev::MapStart { tag: None, flow: true } });
        self.pos += 1;
        loop {
            self.flow_skip()?;
            if self.eof() {
                return self.err("did not find expected ',' or closing bracket (unclosed flow collection)");
            }
            if self.peek() == close {
                self.pos += 1;
                break;
            }
            if self.peek() == b',' {
                return self.err("unexpected ',' in a flow collection");
            }
            if seq {
                self.flow_seq_entry()?;
            } else {
                self.flow_map_entry()?;
            }
            self.flow_skip()?;
            if self.peek() == b',' {
                self.pos += 1;
            } else if self.peek() != close {
                return self.err("did not find expected ',' or closing bracket in a flow collection");
            }
        }
        self.ev.push(if seq { Ev::SeqEnd } else { Ev::MapEnd });
        self.leave();
        Ok(())
    }

    fn flow_value_follows(&self) -> bool {
        self.peek() == b':'
    }

    fn flow_seq_entry(&mut self) -> R<()> {
        let at = self.ev.len();
        let explicit = self.peek() == b'?' && (self.blankz(self.pos + 1) || is_flow_ind(self.at(self.pos + 1)));
        if explicit {
            self.pos += 1;
            self.flow_skip()?;
            self.ev.insert(at, Ev::MapStart { tag: None, flow: true });
            self.flow_key_or_empty()?;
            self.flow_skip()?;
            self.flow_pair_value()?;
            self.ev.push(Ev::MapEnd);
            return Ok(());
        }
        if self.peek() == b':' {
            return self.err("did not find expected node content");
        }
        self.flow_node()?;
        self.flow_skip()?;
        if self.flow_value_follows() {
            self.ev.insert(at, Ev::MapStart { tag: None, flow: true });
            self.flow_pair_value()?;
            self.ev.push(Ev::MapEnd);
        }
        Ok(())
    }

    fn flow_key_or_empty(&mut self) -> R<()> {
        if matches!(self.peek(), b',' | b']' | b'}') {
            return self.err("did not find expected node content");
        }
        if self.peek() == b':' {
            self.empty(None);
            Ok(())
        } else {
            self.flow_node()
        }
    }

    /// At `:` (or not): pushes the value of a flow pair.
    fn flow_pair_value(&mut self) -> R<()> {
        if self.flow_value_follows() {
            self.pos += 1;
            self.flow_skip()?;
            if matches!(self.peek(), b',' | b']' | b'}') {
                self.empty(None);
                Ok(())
            } else {
                self.flow_node()
            }
        } else {
            self.empty(None);
            Ok(())
        }
    }

    fn flow_map_entry(&mut self) -> R<()> {
        let explicit = self.peek() == b'?' && (self.blankz(self.pos + 1) || is_flow_ind(self.at(self.pos + 1)));
        if explicit {
            self.pos += 1;
            self.flow_skip()?;
        }
        if self.peek() == b':' && !explicit {
            return self.err("did not find expected node content");
        }
        self.flow_key_or_empty()?;
        self.flow_skip()?;
        self.flow_pair_value()
    }

    fn flow_node(&mut self) -> R<()> {
        let at = self.ev.len();
        let mut tag = None;
        let mut props_seen = false;
        while matches!(self.peek(), b'&' | b'!') {
            props_seen = true;
            let p = self.parse_props()?;
            tag = p.tag.or(tag);
            self.flow_skip()?;
        }
        match self.peek() {
            b'*' => {
                if props_seen {
                    return self.err("an alias cannot have properties");
                }
                let name = self.alias_name()?;
                self.ev.push(Ev::Alias(name));
            }
            b'[' | b'{' => {
                let at2 = self.ev.len();
                self.flow_collection()?;
                if let Some(Ev::SeqStart { tag: t, .. } | Ev::MapStart { tag: t, .. }) = self.ev.get_mut(at2) {
                    *t = tag;
                }
            }
            b'\'' | b'"' => {
                let mut q = self.quoted()?;
                q.tag = tag;
                self.ev.push(Ev::Scalar(q));
            }
            b',' | b']' | b'}' => self.empty(tag),
            b':' => self.empty(tag),
            b'|' | b'>' => return self.err("block scalars are not allowed in a flow collection"),
            _ if self.eof() => return self.err("found unexpected end of stream in a flow collection"),
            _ => {
                let start = self.pos;
                let (end, value, colon) = self.plain_first_line(true)?;
                let mut sc = if colon || !(self.pos < self.b.len() && Self::is_break(self.b[self.pos]) || matches!(self.at(self.pos), b' ' | b'\t')) {
                    ScalarEv { start, end, value, style: Style::Plain, tag: None, token: true }
                } else {
                    self.plain_rest(start, end, value, -1, true)?
                };
                sc.tag = tag;
                self.pos = sc.end;
                self.ev.push(Ev::Scalar(sc));
            }
        }
        let _ = at;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scalars(t: &str) -> Vec<(String, Style)> {
        scan(t, 200)
            .unwrap()
            .into_iter()
            .filter_map(|e| if let Ev::Scalar(s) = e { Some((s.value, s.style)) } else { None })
            .collect()
    }

    #[test]
    fn basics() {
        let s = scalars("a: 1\nb:\n  - x\n  - 'y z'\nc: |\n  l1\n  l2\n");
        let v: Vec<&str> = s.iter().map(|x| x.0.as_str()).collect();
        assert_eq!(v, vec!["a", "1", "b", "x", "y z", "c", "l1\nl2\n"]);
    }

    #[test]
    fn errors() {
        for bad in ["a: b: c\n", "a: [unclosed\n", "key: 'unterminated\n", "a: 1\nb\n", "- a\nb: 1\n"] {
            assert!(scan(bad, 200).is_err(), "{bad:?}");
        }
    }
}

//! A Fluent 1.0 parser that follows `fluent.syntax` (python-fluent 0.19) step for step, so that entry boundaries,
//! Junk and pattern spans come out the same as in the Localization OS filter. It keeps only what the filter needs:
//! messages and terms with their patterns (text and placeable spans, select expressions and their variants) and the
//! regular comment attached to each entry.
//!
//! Offsets are byte offsets into the source. Every structural character is ASCII, so cutting at them is always on a
//! character boundary. A CRLF pair is one line end, a lone CR is an ordinary character (as in the reference parser).

type R<T> = Result<T, Perr>;

/// A parse error. The parser turns it into Junk and carries on at the next entry.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Perr;

/// One element of a pattern after the common indent was removed.
#[derive(Debug, Clone)]
pub(crate) enum El {
    /// Text with line ends as `\n`, indentation removed. `start..end` is the source span (it can be longer than the
    /// value: the trailing blanks of the last line are trimmed from the value but not from the span).
    Text { start: usize, end: usize, value: String },
    /// A `{ ... }` placeable. `start..end` is the source span including the braces.
    Place { start: usize, end: usize, kind: PKind },
}

#[derive(Debug, Clone)]
pub(crate) enum PKind {
    /// A select expression: ends a run of text, each variant is its own pattern.
    Select(Select),
    /// A string literal placeable, with the literal's text (escapes as written).
    Str(String),
    /// Anything else: a variable, term or message reference, a function call, a number, a nested placeable.
    Other,
}

#[derive(Debug, Clone)]
pub(crate) struct Select {
    /// `NUMBER($n, type: "ordinal")`.
    pub ordinal: bool,
    pub variants: Vec<Variant>,
}

#[derive(Debug, Clone)]
pub(crate) struct Variant {
    pub start: usize,
    pub end: usize,
    pub default: bool,
    /// The key as the unit label shows it: the identifier, or the number as written.
    pub key: String,
    /// The key is an identifier (not a number).
    pub ident: bool,
    /// End of the key's identifier.
    pub key_end: usize,
    pub value: Vec<El>,
}

#[derive(Debug, Clone)]
pub(crate) struct Attr {
    pub name: String,
    pub value: Vec<El>,
}

#[derive(Debug, Clone)]
pub(crate) struct Item {
    pub id: String,
    pub is_term: bool,
    pub value: Option<Vec<El>>,
    pub attrs: Vec<Attr>,
    /// The regular `#` comment directly above the entry.
    pub comment: Option<String>,
}

enum Entry {
    Comment(String),
    OtherComment,
    Junk,
    Item(Item),
}

enum Expr {
    Nested,
    Number,
    Str(String),
    Var,
    Term { attr: bool },
    Msg { name: String, attr: bool },
    Func { ordinal: bool },
    Select(Select),
}

enum Arg {
    Pos,
    Named { name: String, ordinal: bool },
}

const EOL: u8 = b'\n';

struct Ps<'a> {
    src: &'a str,
    s: &'a [u8],
    index: usize,
    peek_offset: usize,
}

fn is_id_start(c: Option<u8>) -> bool {
    matches!(c, Some(b'a'..=b'z' | b'A'..=b'Z'))
}

fn is_id_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'-'
}

impl<'a> Ps<'a> {
    fn new(src: &'a str) -> Ps<'a> {
        Ps { src, s: src.as_bytes(), index: 0, peek_offset: 0 }
    }

    fn get(&self, off: usize) -> Option<u8> {
        self.s.get(off).copied()
    }

    fn char_at(&self, off: usize) -> Option<u8> {
        match self.get(off) {
            Some(b'\r') if self.get(off + 1) == Some(b'\n') => Some(b'\n'),
            c => c,
        }
    }

    fn cur(&self) -> Option<u8> {
        self.char_at(self.index)
    }

    fn cur_peek(&self) -> Option<u8> {
        self.char_at(self.index + self.peek_offset)
    }

    fn next(&mut self) -> Option<u8> {
        self.peek_offset = 0;
        if self.get(self.index) == Some(b'\r') && self.get(self.index + 1) == Some(b'\n') {
            self.index += 1;
        }
        self.index += 1;
        self.get(self.index)
    }

    fn peek(&mut self) -> Option<u8> {
        let p = self.index + self.peek_offset;
        if self.get(p) == Some(b'\r') && self.get(p + 1) == Some(b'\n') {
            self.peek_offset += 1;
        }
        self.peek_offset += 1;
        self.get(self.index + self.peek_offset)
    }

    fn reset_peek(&mut self, offset: usize) {
        self.peek_offset = offset;
    }

    fn skip_to_peek(&mut self) {
        self.index += self.peek_offset;
        self.peek_offset = 0;
    }

    fn peek_blank_inline(&mut self) -> usize {
        let start = self.index + self.peek_offset;
        while self.cur_peek() == Some(b' ') {
            self.peek();
        }
        self.index + self.peek_offset - start
    }

    fn skip_blank_inline(&mut self) -> usize {
        let n = self.peek_blank_inline();
        self.skip_to_peek();
        n
    }

    /// Number of line ends in the blank block (the reference returns that many `\n`).
    fn peek_blank_block(&mut self) -> usize {
        let mut blank = 0;
        loop {
            let line_start = self.peek_offset;
            self.peek_blank_inline();
            if self.cur_peek() == Some(EOL) {
                blank += 1;
                self.peek();
                continue;
            }
            if self.cur_peek().is_none() {
                return blank;
            }
            self.reset_peek(line_start);
            return blank;
        }
    }

    fn skip_blank_block(&mut self) -> usize {
        let n = self.peek_blank_block();
        self.skip_to_peek();
        n
    }

    fn peek_blank(&mut self) {
        while matches!(self.cur_peek(), Some(b' ' | EOL)) {
            self.peek();
        }
    }

    fn skip_blank(&mut self) {
        self.peek_blank();
        self.skip_to_peek();
    }

    fn expect_char(&mut self, ch: u8) -> R<()> {
        if self.cur() == Some(ch) {
            self.next();
            Ok(())
        } else {
            Err(Perr)
        }
    }

    fn expect_line_end(&mut self) -> R<()> {
        match self.cur() {
            None => Ok(()),
            Some(EOL) => {
                self.next();
                Ok(())
            }
            _ => Err(Perr),
        }
    }

    fn is_identifier_start(&self) -> bool {
        is_id_start(self.cur_peek())
    }

    fn is_number_start(&mut self) -> bool {
        let ch = if self.cur() == Some(b'-') { self.peek() } else { self.cur() };
        let r = matches!(ch, Some(b'0'..=b'9'));
        self.reset_peek(0);
        r
    }

    fn is_value_start(&self) -> bool {
        !matches!(self.cur_peek(), None | Some(EOL))
    }

    fn is_value_continuation(&mut self) -> bool {
        let column1 = self.peek_offset;
        self.peek_blank_inline();
        if self.cur_peek() == Some(b'{') {
            self.reset_peek(column1);
            return true;
        }
        if self.peek_offset == column1 {
            return false;
        }
        if let Some(c) = self.cur_peek() {
            if !matches!(c, b'}' | b'.' | b'[' | b'*') {
                self.reset_peek(column1);
                return true;
            }
        }
        false
    }

    fn is_next_line_comment(&mut self, level: i32) -> bool {
        if self.cur_peek() != Some(EOL) {
            return false;
        }
        let mut i = 0;
        while i <= level || (level == -1 && i < 3) {
            if self.peek() != Some(b'#') {
                if i <= level && level != -1 {
                    self.reset_peek(0);
                    return false;
                }
                break;
            }
            i += 1;
        }
        if matches!(self.peek(), Some(b' ' | EOL)) {
            self.reset_peek(0);
            return true;
        }
        self.reset_peek(0);
        false
    }

    fn is_variant_start(&mut self) -> bool {
        let saved = self.peek_offset;
        if self.cur_peek() == Some(b'*') {
            self.peek();
        }
        let r = self.cur_peek() == Some(b'[') && self.peek() != Some(b'[');
        self.reset_peek(saved);
        r
    }

    fn is_attribute_start(&self) -> bool {
        self.cur_peek() == Some(b'.')
    }

    fn skip_to_next_entry_start(&mut self, junk_start: usize) {
        let upto = self.index.min(self.s.len());
        let last_newline = self.s[..upto].iter().rposition(|&b| b == EOL);
        if let Some(ln) = last_newline {
            if junk_start < ln {
                self.index = ln;
            }
        }
        while self.cur().is_some() {
            if self.cur() != Some(EOL) {
                self.next();
                continue;
            }
            let first = self.next();
            if is_id_start(first) || first == Some(b'-') || first == Some(b'#') {
                break;
            }
            let peek = self.peek();
            self.reset_peek(0);
            if (first, peek) == (Some(b'/'), Some(b'/')) || (first, peek) == (Some(b'['), Some(b'[')) {
                break;
            }
        }
    }
}

/// Parses a Fluent resource into its messages and terms (comments attached, Junk dropped).
pub(crate) fn parse(src: &str) -> Vec<Item> {
    parse_counted(src).0
}

/// Like [`parse`], also returning how many Junk entries the file has.
pub(crate) fn parse_counted(src: &str) -> (Vec<Item>, usize) {
    let mut p = Ps::new(src);
    p.skip_blank_block();
    let mut out: Vec<Item> = Vec::new();
    let mut last_comment: Option<String> = None;
    let mut junk = 0usize;
    while p.cur().is_some() {
        let entry = entry_or_junk(&mut p);
        let blank = p.skip_blank_block();
        if let Entry::Comment(c) = &entry {
            if blank == 0 && p.cur().is_some() {
                last_comment = Some(c.clone());
                continue;
            }
        }
        let comment = last_comment.take();
        match entry {
            Entry::Item(mut it) => {
                it.comment = comment;
                out.push(it);
            }
            Entry::Junk => junk += 1,
            _ => {}
        }
    }
    (out, junk)
}

fn entry_or_junk(p: &mut Ps) -> Entry {
    let start = p.index;
    let r = entry(p).and_then(|e| p.expect_line_end().map(|_| e));
    match r {
        Ok(e) => e,
        Err(Perr) => {
            p.skip_to_next_entry_start(start);
            Entry::Junk
        }
    }
}

fn entry(p: &mut Ps) -> R<Entry> {
    match p.cur() {
        Some(b'#') => comment(p),
        Some(b'-') => term(p),
        _ if p.is_identifier_start() => message(p),
        _ => Err(Perr),
    }
}

fn comment(p: &mut Ps) -> R<Entry> {
    let mut level: i32 = -1;
    let mut content: Vec<u8> = Vec::new();
    loop {
        let mut i: i32 = -1;
        while p.cur() == Some(b'#') && i < (if level == -1 { 2 } else { level }) {
            p.next();
            i += 1;
        }
        if level == -1 {
            level = i;
        }
        if p.cur() != Some(EOL) {
            p.expect_char(b' ')?;
            while let Some(c) = p.cur() {
                if c == EOL {
                    break;
                }
                content.push(c);
                p.next();
            }
        }
        if p.is_next_line_comment(level) {
            content.push(b'\n');
            p.next();
        } else {
            break;
        }
    }
    Ok(if level == 0 { Entry::Comment(String::from_utf8_lossy(&content).into_owned()) } else { Entry::OtherComment })
}

fn identifier(p: &mut Ps) -> R<String> {
    let start = p.index;
    if !is_id_start(p.cur()) {
        return Err(Perr);
    }
    p.next();
    while let Some(c) = p.cur() {
        if !is_id_char(c) {
            break;
        }
        p.next();
    }
    Ok(p.src[start..p.index].to_string())
}

fn message(p: &mut Ps) -> R<Entry> {
    let id = identifier(p)?;
    p.skip_blank_inline();
    p.expect_char(b'=')?;
    let value = maybe_pattern(p)?;
    let attrs = attributes(p)?;
    if value.is_none() && attrs.is_empty() {
        return Err(Perr);
    }
    Ok(Entry::Item(Item { id, is_term: false, value, attrs, comment: None }))
}

fn term(p: &mut Ps) -> R<Entry> {
    p.expect_char(b'-')?;
    let id = identifier(p)?;
    p.skip_blank_inline();
    p.expect_char(b'=')?;
    let value = maybe_pattern(p)?;
    if value.is_none() {
        return Err(Perr);
    }
    let attrs = attributes(p)?;
    Ok(Entry::Item(Item { id, is_term: true, value, attrs, comment: None }))
}

fn attributes(p: &mut Ps) -> R<Vec<Attr>> {
    let mut attrs = Vec::new();
    p.peek_blank();
    while p.is_attribute_start() {
        p.skip_to_peek();
        p.expect_char(b'.')?;
        let name = identifier(p)?;
        p.skip_blank_inline();
        p.expect_char(b'=')?;
        let value = maybe_pattern(p)?.ok_or(Perr)?;
        attrs.push(Attr { name, value });
        p.peek_blank();
    }
    Ok(attrs)
}

fn maybe_pattern(p: &mut Ps) -> R<Option<Vec<El>>> {
    p.peek_blank_inline();
    if p.is_value_start() {
        p.skip_to_peek();
        return pattern(p, false).map(Some);
    }
    p.peek_blank_block();
    if p.is_value_continuation() {
        p.skip_to_peek();
        return pattern(p, true).map(Some);
    }
    Ok(None)
}

enum Raw {
    Text { start: usize, end: usize },
    Place { start: usize, end: usize, kind: PKind },
    /// A line end with the blank lines before the next text (`nl` line ends) and the next line's indent.
    Indent { nl: usize, spaces: usize, start: usize, end: usize },
}

fn pattern(p: &mut Ps, is_block: bool) -> R<Vec<El>> {
    let mut els: Vec<Raw> = Vec::new();
    let mut common = usize::MAX;
    if is_block {
        let blank_start = p.index;
        let first = p.skip_blank_inline();
        els.push(Raw::Indent { nl: 0, spaces: first, start: blank_start, end: p.index });
        common = first;
    }
    while let Some(c) = p.cur() {
        if c == EOL {
            let blank_start = p.index;
            let nl = p.peek_blank_block();
            if p.is_value_continuation() {
                p.skip_to_peek();
                let spaces = p.skip_blank_inline();
                common = common.min(spaces);
                els.push(Raw::Indent { nl, spaces, start: blank_start, end: p.index });
                continue;
            }
            p.reset_peek(0);
            break;
        }
        if c == b'}' {
            return Err(Perr);
        }
        if c == b'{' {
            let start = p.index;
            let kind = placeable(p)?;
            els.push(Raw::Place { start, end: p.index, kind });
        } else {
            let start = p.index;
            while let Some(c) = p.cur() {
                if c == b'{' || c == b'}' || c == EOL {
                    break;
                }
                p.next();
            }
            els.push(Raw::Text { start, end: p.index });
        }
    }
    Ok(dedent(p.src, els, common))
}

fn dedent(src: &str, els: Vec<Raw>, common: usize) -> Vec<El> {
    let mut out: Vec<El> = Vec::with_capacity(els.len());
    for el in els {
        let (start, end, value) = match el {
            Raw::Place { start, end, kind } => {
                out.push(El::Place { start, end, kind });
                continue;
            }
            Raw::Indent { nl, spaces, start, end } => {
                let keep = (nl + spaces).saturating_sub(common);
                if keep == 0 {
                    continue;
                }
                let mut v = String::with_capacity(keep);
                for k in 0..keep {
                    v.push(if k < nl { '\n' } else { ' ' });
                }
                (start, end, v)
            }
            Raw::Text { start, end } => (start, end, src[start..end].to_string()),
        };
        if let Some(El::Text { end: pe, value: pv, .. }) = out.last_mut() {
            pv.push_str(&value);
            *pe = end;
            continue;
        }
        out.push(El::Text { start, end, value });
    }
    if let Some(El::Text { value, .. }) = out.last_mut() {
        let keep = value.trim_end_matches([' ', '\n', '\r']).len();
        value.truncate(keep);
        if value.is_empty() {
            out.pop();
        }
    }
    out
}

fn placeable(p: &mut Ps) -> R<PKind> {
    p.expect_char(b'{')?;
    p.skip_blank();
    let e = expression(p)?;
    p.expect_char(b'}')?;
    Ok(match e {
        Expr::Select(s) => PKind::Select(s),
        Expr::Str(v) => PKind::Str(v),
        _ => PKind::Other,
    })
}

fn expression(p: &mut Ps) -> R<Expr> {
    let selector = inline_expression(p)?;
    p.skip_blank();
    if p.cur() == Some(b'-') {
        if p.peek() != Some(b'>') {
            p.reset_peek(0);
            return Ok(selector);
        }
        match &selector {
            Expr::Msg { .. } => return Err(Perr),
            Expr::Term { attr: false } => return Err(Perr),
            Expr::Term { attr: true } => {}
            Expr::Str(_) | Expr::Number | Expr::Var | Expr::Func { .. } => {}
            _ => return Err(Perr),
        }
        let ordinal = matches!(selector, Expr::Func { ordinal: true });
        p.next();
        p.next();
        p.skip_blank_inline();
        p.expect_line_end()?;
        let variants = variants(p)?;
        return Ok(Expr::Select(Select { ordinal, variants }));
    }
    if matches!(selector, Expr::Term { attr: true }) {
        return Err(Perr);
    }
    Ok(selector)
}

fn variants(p: &mut Ps) -> R<Vec<Variant>> {
    let mut out: Vec<Variant> = Vec::new();
    let mut has_default = false;
    p.skip_blank();
    while p.is_variant_start() {
        let v = variant(p, has_default)?;
        if v.default {
            has_default = true;
        }
        out.push(v);
        p.expect_line_end()?;
        p.skip_blank();
    }
    if out.is_empty() || !has_default {
        return Err(Perr);
    }
    Ok(out)
}

fn variant(p: &mut Ps, has_default: bool) -> R<Variant> {
    let start = p.index;
    let mut default = false;
    if p.cur() == Some(b'*') {
        if has_default {
            return Err(Perr);
        }
        p.next();
        default = true;
    }
    p.expect_char(b'[')?;
    p.skip_blank();
    let (key, ident, key_end) = match p.cur() {
        None => return Err(Perr),
        Some(b'0'..=b'9' | b'-') => {
            let s = p.index;
            number(p)?;
            (p.src[s..p.index].to_string(), false, p.index)
        }
        Some(_) => {
            let k = identifier(p)?;
            (k, true, p.index)
        }
    };
    p.skip_blank();
    p.expect_char(b']')?;
    let value = maybe_pattern(p)?.ok_or(Perr)?;
    Ok(Variant { start, end: p.index, default, key, ident, key_end, value })
}

fn digits(p: &mut Ps) -> R<()> {
    let mut n = 0;
    while matches!(p.cur(), Some(b'0'..=b'9')) {
        p.next();
        n += 1;
    }
    if n == 0 {
        Err(Perr)
    } else {
        Ok(())
    }
}

fn number(p: &mut Ps) -> R<()> {
    if p.cur() == Some(b'-') {
        p.next();
    }
    digits(p)?;
    if p.cur() == Some(b'.') {
        p.next();
        digits(p)?;
    }
    Ok(())
}

fn inline_expression(p: &mut Ps) -> R<Expr> {
    if p.cur() == Some(b'{') {
        placeable(p)?;
        return Ok(Expr::Nested);
    }
    if p.is_number_start() {
        number(p)?;
        return Ok(Expr::Number);
    }
    if p.cur() == Some(b'"') {
        return string(p).map(Expr::Str);
    }
    if p.cur() == Some(b'$') {
        p.next();
        identifier(p)?;
        return Ok(Expr::Var);
    }
    if p.cur() == Some(b'-') {
        p.next();
        identifier(p)?;
        let mut attr = false;
        if p.cur() == Some(b'.') {
            p.next();
            identifier(p)?;
            attr = true;
        }
        p.peek_blank();
        if p.cur_peek() == Some(b'(') {
            p.skip_to_peek();
            call_arguments(p)?;
        }
        return Ok(Expr::Term { attr });
    }
    if p.is_identifier_start() {
        let name = identifier(p)?;
        p.peek_blank();
        if p.cur_peek() == Some(b'(') {
            let b = name.as_bytes();
            if !(b[0].is_ascii_uppercase() && b.iter().all(|&c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == b'_' || c == b'-')) {
                return Err(Perr);
            }
            p.skip_to_peek();
            let type_ordinal = call_arguments(p)?;
            return Ok(Expr::Func { ordinal: name == "NUMBER" && type_ordinal });
        }
        let mut attr = false;
        if p.cur() == Some(b'.') {
            p.next();
            identifier(p)?;
            attr = true;
        }
        return Ok(Expr::Msg { name, attr });
    }
    Err(Perr)
}

fn call_argument(p: &mut Ps) -> R<Arg> {
    let exp = inline_expression(p)?;
    p.skip_blank();
    if p.cur() != Some(b':') {
        return Ok(Arg::Pos);
    }
    if let Expr::Msg { name, attr: false } = exp {
        p.next();
        p.skip_blank();
        let ordinal = if p.is_number_start() {
            number(p)?;
            false
        } else if p.cur() == Some(b'"') {
            string(p)? == "ordinal"
        } else {
            return Err(Perr);
        };
        return Ok(Arg::Named { name, ordinal });
    }
    Err(Perr)
}

/// Parses `( ... )`; returns true when a named argument `type` is the string "ordinal".
fn call_arguments(p: &mut Ps) -> R<bool> {
    let mut names: Vec<String> = Vec::new();
    let mut type_ordinal = false;
    p.expect_char(b'(')?;
    p.skip_blank();
    loop {
        if p.cur() == Some(b')') {
            break;
        }
        match call_argument(p)? {
            Arg::Named { name, ordinal } => {
                if names.contains(&name) {
                    return Err(Perr);
                }
                if name == "type" && ordinal {
                    type_ordinal = true;
                }
                names.push(name);
            }
            Arg::Pos => {
                if !names.is_empty() {
                    return Err(Perr);
                }
            }
        }
        p.skip_blank();
        if p.cur() == Some(b',') {
            p.next();
            p.skip_blank();
            continue;
        }
        break;
    }
    p.expect_char(b')')?;
    Ok(type_ordinal)
}

fn string(p: &mut Ps) -> R<String> {
    p.expect_char(b'"')?;
    let start = p.index;
    loop {
        match p.cur() {
            Some(b'"') | Some(EOL) | None => break,
            Some(b'\\') => {
                p.next();
                match p.cur() {
                    Some(b'\\' | b'"') => {
                        p.next();
                    }
                    Some(u @ (b'u' | b'U')) => {
                        p.next();
                        let n = if u == b'u' { 4 } else { 6 };
                        for _ in 0..n {
                            if !matches!(p.cur(), Some(b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F')) {
                                return Err(Perr);
                            }
                            p.next();
                        }
                    }
                    _ => return Err(Perr),
                }
            }
            Some(_) => {
                p.next();
            }
        }
    }
    if p.cur() == Some(EOL) {
        return Err(Perr);
    }
    let end = p.index;
    p.expect_char(b'"')?;
    Ok(p.src[start..end].to_string())
}

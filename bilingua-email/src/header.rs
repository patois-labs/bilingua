//! Header fields: splitting a block into raw fields, the Subject (RFC 2047 encoded words decoded and re-encoded the
//! way Python's `email.header` does for LocOS), and the edits a rebuilt part needs (Subject, Content-Type charset,
//! Content-Transfer-Encoding, Content-Language).

use crate::codec::Codec;
use crate::cte;
use crate::pyish::{byte_lines_keep, is_space};
use regex::bytes::Regex as BRegex;
use std::sync::OnceLock;

/// A raw header field: lower-case name and its bytes (with continuation lines and line ending).
pub struct Field<'a> {
    pub name: String,
    pub raw: &'a [u8],
    pub offset: usize,
}

fn trim_ascii(b: &[u8]) -> &[u8] {
    let s = b.iter().position(|c| !c.is_ascii_whitespace()).unwrap_or(b.len());
    let e = b.iter().rposition(|c| !c.is_ascii_whitespace()).map_or(s, |p| p + 1);
    &b[s..e]
}

/// Splits a header block into fields (continuation lines stay with their field).
pub fn fields(block: &[u8]) -> Vec<Field<'_>> {
    let mut out: Vec<Field<'_>> = Vec::new();
    let mut pos = 0;
    for line in byte_lines_keep(block) {
        if matches!(line.first(), Some(b' ' | b'\t')) && !out.is_empty() {
            let f = out.last_mut().unwrap();
            let start = f.offset;
            f.raw = &block[start..pos + line.len()];
        } else {
            let name_end = line.iter().position(|c| *c == b':').unwrap_or(line.len());
            let name: String = trim_ascii(&line[..name_end]).to_ascii_lowercase().iter().map(|c| *c as char).collect();
            out.push(Field { name, raw: line, offset: pos });
        }
        pos += line.len();
    }
    out
}

/// `re.sub(rb"\r?\n(?=[ \t])", b"", raw).rstrip(b"\r\n")`.
fn unfold(raw: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        if raw[i] == b'\n' && matches!(raw.get(i + 1), Some(b' ' | b'\t')) {
            if out.last() == Some(&b'\r') {
                out.pop();
            }
            i += 1;
            continue;
        }
        out.push(raw[i]);
        i += 1;
    }
    while matches!(out.last(), Some(b'\r' | b'\n')) {
        out.pop();
    }
    out
}

// ----------------------------------------------------------------------------------------- decoding

/// The Subject field of the top-level header block.
pub struct Subject {
    pub text: String,
    pub eight_bit: bool,
    /// Charset of the first encoded word ("" when there is none).
    pub hint: String,
}

pub fn read_subject(data: &[u8], block_start: usize, block_end: usize) -> Option<Subject> {
    let block = &data[block_start..block_end];
    for f in fields(block) {
        if f.name != "subject" {
            continue;
        }
        let un = unfold(f.raw);
        let colon = un.iter().position(|c| *c == b':')?;
        let value: &[u8] = {
            let v = &un[colon + 1..];
            let s = v.iter().position(|c| *c != b' ' && *c != b'\t').unwrap_or(v.len());
            &v[s..]
        };
        let hint = first_charset(value);
        let eight = !value.is_ascii();
        let text = if eight { String::from_utf8(value.to_vec()).ok()? } else { String::from_utf8_lossy(value).into_owned() };
        let text = if text.contains("=?") { decode_words(&text)? } else { text };
        return Some(Subject { text, eight_bit: eight, hint });
    }
    None
}

/// `=\?([^?*]+)`: the charset of the first encoded word.
fn first_charset(v: &[u8]) -> String {
    let mut i = 0;
    while i + 2 < v.len() {
        if v[i] == b'=' && v[i + 1] == b'?' {
            let rest = &v[i + 2..];
            let n = rest.iter().position(|c| *c == b'?' || *c == b'*').unwrap_or(rest.len());
            if n > 0 {
                return String::from_utf8_lossy(&rest[..n]).into_owned();
            }
        }
        i += 1;
    }
    String::new()
}

struct Word {
    /// decoded bytes (encoded word) or the text itself (plain)
    bytes: Vec<u8>,
    charset: Option<String>,
    encoded: bool,
}

/// One `=?charset?q|b?text?=` match in `line`: (start, end, charset, encoding letter, text).
fn next_word(line: &str, from: usize) -> Option<(usize, usize, &str, char, &str)> {
    let b = line.as_bytes();
    let mut i = from;
    while i + 1 < b.len() {
        if b[i] == b'=' && b[i + 1] == b'?' {
            // charset: non-greedy up to the next '?'
            let cs_start = i + 2;
            if let Some(q1) = line[cs_start..].find('?').map(|p| p + cs_start) {
                let enc = b.get(q1 + 1).copied();
                if matches!(enc, Some(b'q' | b'Q' | b'b' | b'B')) && b.get(q1 + 2) == Some(&b'?') {
                    let t_start = q1 + 3;
                    // no closing "?=" here means none later either: stop looking (keeps a hostile line linear)
                    let e = line[t_start..].find("?=").map(|p| p + t_start)?;
                    return Some((i, e + 2, &line[cs_start..q1], enc.unwrap() as char, &line[t_start..e]));
                }
            }
        }
        i += 1;
    }
    None
}

fn q_decode(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'_' => out.push(b' '),
            b'=' if i + 2 < b.len() && b[i + 1].is_ascii_hexdigit() && b[i + 2].is_ascii_hexdigit() => {
                out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).unwrap());
                i += 2;
            }
            c => out.push(c),
        }
        i += 1;
    }
    out
}

/// Python's `str(make_header(decode_header(text)))`. `None` where LocOS finds no usable Subject (an unknown
/// charset, bytes that do not fit it, a damaged base64 word).
pub fn decode_words(text: &str) -> Option<String> {
    if next_word(text, 0).is_none() {
        return Some(text.to_string());
    }
    let mut words: Vec<Word> = Vec::new();
    for line in crate::pyish::splitlines_keep(text) {
        let line = line.trim_end_matches(['\r', '\n', '\u{0b}', '\u{0c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}', '\u{2029}']);
        let mut pos = 0;
        let mut first = true;
        loop {
            let m = next_word(line, pos);
            let unenc = &line[pos..m.map_or(line.len(), |x| x.0)];
            let unenc = if first { unenc.trim_start_matches(is_space) } else { unenc };
            first = false;
            if !unenc.is_empty() {
                words.push(Word { bytes: raw_unicode_escape(unenc), charset: None, encoded: false });
            }
            let Some((_, e, cs, enc, body)) = m else { break };
            let bytes = if enc.eq_ignore_ascii_case(&'q') {
                q_decode(body)
            } else {
                let mut b = body.to_string();
                let pad = b.len() % 4;
                if pad != 0 {
                    b.push_str(&"==="[..4 - pad]);
                }
                cte::b64_decode(b.as_bytes()).ok()?
            };
            words.push(Word { bytes, charset: Some(cs.to_ascii_lowercase()), encoded: true });
            pos = e;
        }
    }
    // whitespace between two encoded words is dropped
    let mut drop = Vec::new();
    for n in 2..words.len() {
        if words[n].encoded && words[n - 2].encoded && !words[n - 1].encoded && !words[n - 1].bytes.is_empty() && words[n - 1].bytes.iter().all(|c| c.is_ascii_whitespace()) {
            drop.push(n - 1);
        }
    }
    for d in drop.into_iter().rev() {
        words.remove(d);
    }
    // collapse neighbours with the same charset
    let mut collapsed: Vec<(Vec<u8>, Option<String>)> = Vec::new();
    for w in words {
        match collapsed.last_mut() {
            Some((b, cs)) if *cs == w.charset => {
                if cs.is_none() {
                    b.push(b' ');
                }
                b.extend_from_slice(&w.bytes);
            }
            _ => collapsed.push((w.bytes, w.charset)),
        }
    }
    // decode each chunk and join with Header.__str__'s spacing rules
    let mut out = String::new();
    let mut lastcs_enc = false; // last chunk was a non-ASCII charset
    let mut lastspace = false;
    let mut first = true;
    for (bytes, cs) in collapsed {
        let codec = match cs.as_deref() {
            None | Some("us-ascii") => Codec::Ascii,
            Some(l) => Codec::lookup(l)?,
        };
        let s = codec.decode(&bytes).ok()?;
        let next_enc = !matches!(cs.as_deref(), None | Some("us-ascii"));
        let hasspace = s.chars().next().is_some_and(nonctext);
        if !first {
            if lastcs_enc {
                if !next_enc && !hasspace {
                    out.push(' ');
                }
            } else if next_enc && !lastspace {
                out.push(' ');
            }
        }
        lastspace = s.chars().last().is_some_and(nonctext);
        lastcs_enc = next_enc;
        first = false;
        out.push_str(&s);
    }
    Some(out)
}

fn nonctext(c: char) -> bool {
    is_space(c) || matches!(c, '(' | ')' | '\\')
}

/// `bytes(word, "raw-unicode-escape")`: Latin-1 characters as themselves, the rest as `\uXXXX`.
fn raw_unicode_escape(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for c in s.chars() {
        let n = c as u32;
        if n < 256 {
            out.push(n as u8);
        } else if n < 0x10000 {
            out.extend_from_slice(format!("\\u{n:04x}").as_bytes());
        } else {
            out.extend_from_slice(format!("\\U{n:08x}").as_bytes());
        }
    }
    out
}

// ----------------------------------------------------------------------------------------- encoding

#[derive(Clone, Copy, PartialEq)]
enum HEnc {
    None,
    Qp,
    B64,
    Shortest,
}

struct Cs {
    /// charset name written into the encoded word
    out_name: String,
    out_codec: Codec,
    enc: HEnc,
}

fn canonical_charset(label: &str) -> String {
    let l = label.trim().to_ascii_lowercase();
    match l.as_str() {
        "latin_1" | "latin-1" | "l1" | "iso8859-1" | "iso_8859-1" => "iso-8859-1".into(),
        "latin_2" | "latin-2" => "iso-8859-2".into(),
        "latin_3" | "latin-3" => "iso-8859-3".into(),
        "latin_4" | "latin-4" => "iso-8859-4".into(),
        "latin_5" | "latin-5" => "iso-8859-9".into(),
        "latin_6" | "latin-6" => "iso-8859-10".into(),
        "latin_7" | "latin-7" => "iso-8859-13".into(),
        "latin_8" | "latin-8" => "iso-8859-14".into(),
        "latin_9" | "latin-9" => "iso-8859-15".into(),
        "latin_10" | "latin-10" => "iso-8859-16".into(),
        "utf8" => "utf-8".into(),
        "ascii" => "us-ascii".into(),
        "euc_jp" => "euc-jp".into(),
        "euc_kr" => "euc-kr".into(),
        "eucgb2312_cn" => "gb2312".into(),
        "big5_tw" => "big5".into(),
        _ => l,
    }
}

/// `email.charset.Charset(label)` for the labels a Subject can reasonably carry.
fn charset_for(label: &str) -> Option<Cs> {
    let input = canonical_charset(label);
    let (enc, conv): (HEnc, Option<&str>) = match input.as_str() {
        "iso-8859-1" | "iso-8859-2" | "iso-8859-3" | "iso-8859-4" | "iso-8859-9" | "iso-8859-10" | "iso-8859-13" | "iso-8859-14" | "iso-8859-15" | "iso-8859-16" | "windows-1252" | "viscii" => (HEnc::Qp, None),
        "us-ascii" => (HEnc::None, None),
        "big5" | "gb2312" | "koi8-r" | "iso-2022-jp" => (HEnc::B64, None),
        "euc-jp" | "shift_jis" => (HEnc::B64, Some("iso-2022-jp")),
        "utf-8" => (HEnc::Shortest, None),
        _ => (HEnc::Shortest, None),
    };
    let out_name = conv.map(str::to_string).unwrap_or_else(|| input.clone());
    let out_codec = if out_name == "us-ascii" { Codec::Ascii } else { Codec::lookup(&out_name)? };
    Some(Cs { out_name, out_codec, enc })
}

fn qp_header_len(b: &[u8]) -> usize {
    b.iter().map(|c| if c.is_ascii_alphanumeric() || b"-!*+/ ".contains(c) { 1 } else { 3 }).sum()
}

fn b64_header_len(b: &[u8]) -> usize {
    b.len().div_ceil(3) * 4
}

fn qp_header_encode(b: &[u8], charset: &str) -> String {
    if b.is_empty() {
        return String::new();
    }
    let mut s = String::new();
    for c in b {
        if c.is_ascii_alphanumeric() || b"-!*+/".contains(c) {
            s.push(*c as char);
        } else if *c == b' ' {
            s.push('_');
        } else {
            s.push_str(&format!("={c:02X}"));
        }
    }
    format!("=?{charset}?q?{s}?=")
}

fn b64_header_encode(b: &[u8], charset: &str) -> String {
    if b.is_empty() {
        return String::new();
    }
    format!("=?{charset}?b?{}?=", cte::b64_encode(b))
}

/// The accumulator of `email.header._ValueFormatter`.
struct Fmt {
    maxlen: usize,
    lines: Vec<String>,
    cur: Vec<(String, String)>,
    initial: usize,
}

impl Fmt {
    fn new(headerlen: usize, maxlen: usize) -> Fmt {
        Fmt { maxlen, lines: vec![], cur: vec![], initial: headerlen }
    }
    fn cur_len(&self) -> usize {
        self.initial + self.cur.iter().map(|(f, p)| f.chars().count() + p.chars().count()).sum::<usize>()
    }
    fn cur_str(&self) -> String {
        self.cur.iter().map(|(f, p)| format!("{f}{p}")).collect()
    }
    fn reset(&mut self, start: Vec<(String, String)>) {
        self.cur = start;
        self.initial = 0;
    }
    fn newline(&mut self) {
        let end = self.cur.pop().unwrap_or((String::new(), String::new()));
        if end != (" ".to_string(), String::new()) {
            self.cur.push(end);
        }
        if self.cur_len() > 0 {
            let only_ws = self.initial == 0 && (self.cur.is_empty() || self.cur_str().chars().all(is_space) && !self.cur_str().is_empty());
            if only_ws && !self.lines.is_empty() {
                let s = self.cur_str();
                self.lines.last_mut().unwrap().push_str(&s);
            } else {
                let s = self.cur_str();
                self.lines.push(s);
            }
        }
        self.reset(vec![]);
    }
    fn append_chunk(&mut self, fws: &str, string: &str) {
        self.cur.push((fws.to_string(), string.to_string()));
        if self.cur_len() <= self.maxlen {
            return;
        }
        // find the best split point, working backward from the end
        let n = self.cur.len();
        let mut found: Option<usize> = None;
        'outer: for ch in [';', ',', ' ', '\t'] {
            for i in (1..n).rev() {
                if ch.is_whitespace() {
                    let f = &self.cur[i].0;
                    if f.starts_with(ch) {
                        found = Some(i);
                        break 'outer;
                    }
                }
                if self.cur[i - 1].1.ends_with(ch) {
                    found = Some(i);
                    break 'outer;
                }
            }
        }
        match found {
            None => {
                let (mut fws, part) = self.cur.pop().unwrap();
                if self.initial > 0 {
                    self.newline();
                    if fws.is_empty() {
                        fws = " ".into();
                    }
                }
                self.cur.push((fws, part));
            }
            Some(i) => {
                let remainder = self.cur.split_off(i);
                let s = self.cur_str();
                self.lines.push(s);
                self.reset(remainder);
            }
        }
    }
    fn ascii_split(&mut self, string: &str) {
        // re.split on runs of space and tab: words and white space alternate, starting and ending with a word
        let mut toks: Vec<(bool, String)> = Vec::new();
        for c in string.chars() {
            let ws = c == ' ' || c == '\t';
            match toks.last_mut() {
                Some((w, t)) if *w == ws => t.push(c),
                _ => toks.push((ws, c.to_string())),
            }
        }
        let mut seq: Vec<String> = Vec::new();
        if toks.first().is_none_or(|(ws, _)| *ws) {
            seq.push(String::new());
        }
        let ends_ws = toks.last().is_some_and(|(ws, _)| *ws);
        seq.extend(toks.into_iter().map(|(_, t)| t));
        if ends_ws {
            seq.push(String::new());
        }
        if seq[0].is_empty() {
            seq.remove(0);
        } else {
            seq.insert(0, String::new());
        }
        for pair in seq.chunks(2) {
            if pair.len() == 2 {
                self.append_chunk(&pair[0], &pair[1]);
            }
        }
    }
    fn str(mut self, sep: &str) -> String {
        self.newline();
        self.lines.join(sep)
    }
}

/// `Header(text, charset, header_name="Subject").encode(linesep=eol, maxlinelen=78)`. `None` when the text does
/// not fit the charset's output codec.
pub fn encode_header(text: &str, label: &str, eol: &str) -> Option<String> {
    let mut f = Fmt::new("Subject".len() + 2, 78);
    let is_ascii_cs = label.eq_ignore_ascii_case("us-ascii");
    if is_ascii_cs {
        f.ascii_split(text);
        return Some(f.str(eol));
    }
    let cs = charset_for(label)?;
    if cs.enc == HEnc::None {
        f.ascii_split(text);
        return Some(f.str(eol));
    }
    let header_bytes = cs.out_codec.encode(text)?;
    let encoder = match cs.enc {
        HEnc::Qp => HEnc::Qp,
        HEnc::B64 => HEnc::B64,
        _ => {
            if b64_header_len(&header_bytes) < qp_header_len(&header_bytes) {
                HEnc::B64
            } else {
                HEnc::Qp
            }
        }
    };
    let length = |b: &[u8]| if encoder == HEnc::B64 { b64_header_len(b) } else { qp_header_len(b) };
    let encode = |b: &[u8]| if encoder == HEnc::B64 { b64_header_encode(b, &cs.out_name) } else { qp_header_encode(b, &cs.out_name) };
    let extra = cs.out_name.chars().count() + 7;
    // header_encode_lines
    let first_max = f.maxlen.saturating_sub(f.cur_len());
    let mut maxlens = std::iter::once(first_max).chain(std::iter::repeat(f.maxlen - 1));
    let mut lines: Vec<Option<String>> = Vec::new();
    let mut cur = String::new();
    let mut maxlen = maxlens.next().unwrap().saturating_sub(extra);
    for ch in text.chars() {
        cur.push(ch);
        let enc = cs.out_codec.encode(&cur)?;
        if length(&enc) > maxlen {
            cur.pop();
            if lines.is_empty() && cur.is_empty() {
                lines.push(None);
            }
            lines.push(Some(encode(&cs.out_codec.encode(&cur)?)));
            cur.clear();
            cur.push(ch);
            maxlen = maxlens.next().unwrap().saturating_sub(extra);
        }
    }
    lines.push(Some(encode(&cs.out_codec.encode(&cur)?)));
    // _ValueFormatter.feed
    let first = lines.remove(0);
    if let Some(l) = first {
        f.append_chunk("", &l);
    }
    if lines.is_empty() {
        return Some(f.str(eol));
    }
    let last = lines.pop().unwrap().unwrap_or_default();
    f.newline();
    f.cur.push((" ".into(), last));
    for l in lines {
        f.lines.push(format!(" {}", l.unwrap_or_default()));
    }
    Some(f.str(eol))
}

/// The bytes of a new Subject field (with its line ending).
pub fn encode_subject(sub: &Subject, new: &str, eol: &str) -> Option<Vec<u8>> {
    let new = crate::pyish::collapse_ws(new);
    if sub.eight_bit {
        return Some(format!("Subject: {new}{eol}").into_bytes());
    }
    let enc = if new.is_ascii() {
        encode_header(&new, "us-ascii", eol)?
    } else {
        let mut cs = "utf-8".to_string();
        let hint = sub.hint.to_ascii_lowercase();
        if !hint.is_empty() {
            if let Some(c) = Codec::lookup(&hint) {
                if c.encode(&new).is_some() && charset_for(&hint).is_some_and(|k| k.out_codec.encode(&new).is_some()) {
                    cs = hint;
                }
            }
        }
        encode_header(&new, &cs, eol).or_else(|| encode_header(&new, "utf-8", eol))?
    };
    Some(format!("Subject: {enc}{eol}").into_bytes())
}

// ------------------------------------------------------------------------------------------- edits

#[derive(Default, Clone)]
pub struct Edits {
    pub subject: Option<Vec<u8>>,
    pub cte: Option<String>,
    pub charset: Option<String>,
    pub lang: Option<String>,
}

fn charset_re() -> &'static BRegex {
    static R: OnceLock<BRegex> = OnceLock::new();
    R.get_or_init(|| BRegex::new(r#"(?i)(;\s*charset\s*=\s*)("[^"]*"|[^;\s]*)"#).unwrap())
}

fn set_charset(raw: &[u8], charset: &str, eol: &[u8]) -> Vec<u8> {
    let mut end = raw.len();
    while end > 0 && matches!(raw[end - 1], b'\r' | b'\n') {
        end -= 1;
    }
    let body = &raw[..end];
    let ending: &[u8] = if end < raw.len() { &raw[end..] } else { eol };
    let mut out = Vec::with_capacity(raw.len() + 16);
    match charset_re().captures(body) {
        Some(c) => {
            let m = c.get(0).unwrap();
            out.extend_from_slice(&body[..m.start()]);
            out.extend_from_slice(&c[1]);
            out.push(b'"');
            out.extend_from_slice(charset.as_bytes());
            out.push(b'"');
            out.extend_from_slice(&body[m.end()..]);
        }
        None => {
            out.extend_from_slice(body);
            out.extend_from_slice(b"; charset=\"");
            out.extend_from_slice(charset.as_bytes());
            out.push(b'"');
        }
    }
    out.extend_from_slice(ending);
    out
}

/// Applies edits to one raw header block.
pub fn edit_block(block: &[u8], eol: &[u8], e: &Edits) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::with_capacity(block.len() + 64);
    let (mut seen_ct, mut seen_cte) = (false, false);
    for f in fields(block) {
        match f.name.as_str() {
            "subject" if e.subject.is_some() => out.extend_from_slice(e.subject.as_ref().unwrap()),
            "content-transfer-encoding" if e.cte.is_some() => {
                seen_cte = true;
                out.extend_from_slice(b"Content-Transfer-Encoding: ");
                out.extend_from_slice(e.cte.as_ref().unwrap().as_bytes());
                out.extend_from_slice(eol);
            }
            "content-type" if e.charset.is_some() => {
                seen_ct = true;
                out.extend_from_slice(&set_charset(f.raw, e.charset.as_ref().unwrap(), eol));
            }
            "content-language" if e.lang.as_deref().is_some_and(|l| !l.is_empty()) => {
                out.extend_from_slice(b"Content-Language: ");
                out.extend_from_slice(e.lang.as_ref().unwrap().as_bytes());
                out.extend_from_slice(eol);
            }
            _ => out.extend_from_slice(f.raw),
        }
    }
    let needs_new = (e.charset.is_some() && !seen_ct) || (e.cte.is_some() && !seen_cte);
    if needs_new && !matches!(out.last(), None | Some(b'\n' | b'\r')) {
        out.extend_from_slice(eol);
    }
    if let (Some(cs), false) = (&e.charset, seen_ct) {
        out.extend_from_slice(b"Content-Type: text/plain; charset=\"");
        out.extend_from_slice(cs.as_bytes());
        out.push(b'"');
        out.extend_from_slice(eol);
    }
    if let (Some(c), false) = (&e.cte, seen_cte) {
        out.extend_from_slice(b"Content-Transfer-Encoding: ");
        out.extend_from_slice(c.as_bytes());
        out.extend_from_slice(eol);
    }
    out
}

/// True when the block has a Content-Language field.
pub fn has_field(block: &[u8], name: &str) -> bool {
    fields(block).iter().any(|f| f.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_words() {
        assert_eq!(decode_words("=?ISO-8859-1?Q?Caf=E9_menu?= for {0} guests").unwrap(), "Caf\u{e9} menu for {0} guests");
        assert_eq!(decode_words("=?utf-8?b?w6k=?= =?utf-8?q?=C3=A9?=").unwrap(), "\u{e9}\u{e9}");
        assert_eq!(decode_words("Re: =?utf-8?q?x?=").unwrap(), "Re: x");
        assert_eq!(decode_words("a=?utf-8?q?b?=").unwrap(), "a b");
        assert_eq!(decode_words("=?foo").unwrap(), "=?foo");
        assert!(decode_words("=?x-nope?q?a?=").is_none());
    }

    #[test]
    fn encodes_like_python() {
        assert_eq!(encode_header("Hello world", "us-ascii", "\n").unwrap(), "Hello world");
        assert_eq!(encode_header("Men\u{fc} f\u{fc}r {0}", "iso-8859-1", "\n").unwrap(), "=?iso-8859-1?q?Men=FC_f=FCr_=7B0=7D?=");
        assert_eq!(encode_header("\u{4e2d}\u{6587}", "utf-8", "\n").unwrap(), "=?utf-8?b?5Lit5paH?=");
    }

    #[test]
    fn folds_long_subjects() {
        let t = "word ".repeat(40);
        let e = encode_header(t.trim(), "us-ascii", "\r\n").unwrap();
        assert!(e.split("\r\n").skip(1).all(|l| l.starts_with(' ')));
        assert!(e.split("\r\n").all(|l| l.len() <= 78));
        let u = "\u{e9}t\u{e9} ".repeat(40);
        let e = encode_header(u.trim(), "iso-8859-1", "\n").unwrap();
        assert!(e.split('\n').all(|l| l.len() <= 78));
    }

    #[test]
    fn edits_blocks() {
        let b = b"Subject: x\r\nContent-Type: text/plain;\r\n\tcharset=us-ascii\r\nContent-Transfer-Encoding: 7bit\r\n";
        let e = Edits { cte: Some("quoted-printable".into()), charset: Some("utf-8".into()), ..Default::default() };
        let out = edit_block(b, b"\r\n", &e);
        assert_eq!(out, b"Subject: x\r\nContent-Type: text/plain;\r\n\tcharset=\"utf-8\"\r\nContent-Transfer-Encoding: quoted-printable\r\n");
    }
}


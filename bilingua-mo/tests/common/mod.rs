#![allow(dead_code)]
//! Helpers shared by the test files: a hand packer (independent of the crate's own writer, straight from the GNU
//! manual layout), a small gettext reader with a plural-expression evaluator to check what the crate writes,
//! and the usual extract/merge shortcuts.

use bilingua_model::*;
use bilingua_mo::Mo;
use std::collections::HashMap;

pub const HEADER_FR: &str = "Project-Id-Version: demo 1.0\nLanguage: fr\nMIME-Version: 1.0\nContent-Type: text/plain; charset=UTF-8\nContent-Transfer-Encoding: 8bit\nPlural-Forms: nplurals=2; plural=(n > 1);\n";

pub fn catalog() -> Vec<(Vec<u8>, Vec<u8>)> {
    pairs(&[
        ("", HEADER_FR),
        ("%d file\0%d files", "%d fichier\0%d fichiers"),
        ("Hello %(name)s, you have {0} messages", "Bonjour %(name)s, vous avez {0} messages"),
        ("menu\u{4}Open", "Ouvrir"),
        ("state\u{4}Open", "Ouvert"),
    ])
}

pub fn pairs(p: &[(&str, &str)]) -> Vec<(Vec<u8>, Vec<u8>)> {
    p.iter().map(|(k, v)| (k.as_bytes().to_vec(), v.as_bytes().to_vec())).collect()
}

#[derive(Default)]
pub struct Pack {
    pub big: bool,
    pub hash_size: u32,
    pub revision: u32,
    pub sysdep: Option<[u32; 5]>,
}

/// Packs (msgid, msgstr) pairs per the GNU manual, strings in table order (callers pass them sorted), NUL
/// terminated; a zero-filled hash table sits between the tables and the strings.
pub fn pack_with(pairs: &[(Vec<u8>, Vec<u8>)], o: &Pack) -> Vec<u8> {
    let w = |v: u32| if o.big { v.to_be_bytes() } else { v.to_le_bytes() };
    let n = pairs.len() as u32;
    let head_words = if o.sysdep.is_some() { 12 } else { 7 };
    let otab_at = head_words * 4;
    let ttab_at = otab_at + 8 * n;
    let hash_at = ttab_at + 8 * n;
    let mut pos = hash_at + 4 * o.hash_size;
    let (mut otab, mut ttab, mut blob) = (Vec::new(), Vec::new(), Vec::new());
    for (k, _) in pairs {
        otab.extend_from_slice(&w(k.len() as u32));
        otab.extend_from_slice(&w(pos));
        blob.extend_from_slice(k);
        blob.push(0);
        pos += k.len() as u32 + 1;
    }
    for (_, v) in pairs {
        ttab.extend_from_slice(&w(v.len() as u32));
        ttab.extend_from_slice(&w(pos));
        blob.extend_from_slice(v);
        blob.push(0);
        pos += v.len() as u32 + 1;
    }
    let mut words = vec![0x9504_12de, o.revision, n, otab_at, ttab_at, o.hash_size, hash_at];
    if let Some(s) = o.sysdep {
        words.extend_from_slice(&s);
    }
    let mut out: Vec<u8> = words.iter().flat_map(|x| w(*x)).collect();
    out.extend(otab);
    out.extend(ttab);
    out.extend(vec![0u8; 4 * o.hash_size as usize]);
    out.extend(blob);
    out
}

pub fn pack(pairs: &[(Vec<u8>, Vec<u8>)]) -> Vec<u8> {
    pack_with(pairs, &Pack::default())
}

pub fn mo_le() -> Vec<u8> {
    pack(&catalog())
}

pub fn mo_be_hash() -> Vec<u8> {
    pack_with(&catalog(), &Pack { big: true, hash_size: 7, ..Default::default() })
}

pub fn opts(target: Option<&str>) -> ExtractOptions {
    ExtractOptions { source_lang: Some("en".into()), target_lang: target.map(str::to_string), ..Default::default() }
}

pub fn ex(bytes: &[u8]) -> Document {
    Mo.extract(bytes, &opts(None)).unwrap_or_else(|e| panic!("extract failed: {e}"))
}

pub fn ex_to(bytes: &[u8], target: &str) -> Document {
    Mo.extract(bytes, &opts(Some(target))).unwrap_or_else(|e| panic!("extract failed: {e}"))
}

pub fn try_ex(bytes: &[u8]) -> Result<Document, Error> {
    Mo.extract(bytes, &opts(None))
}

/// Unit text the way Localization OS shows it: placeholder tags flattened to their source text.
pub fn flat(u: &Unit) -> String {
    u.source()
        .iter()
        .map(|i| match i {
            Inline::Text(t) => t.clone(),
            Inline::Tag { display, .. } => display.clone(),
        })
        .collect()
}

pub fn texts(d: &Document) -> Vec<String> {
    d.units.iter().map(flat).collect()
}

pub fn merged(d: &Document) -> Vec<u8> {
    Mo.merge(d).unwrap_or_else(|e| panic!("merge failed: {e}"))
}

/// Sets the target of the unit with this id (a plain-text target; placeholders are kept as text the way an
/// editor sends them back, linked to the source codes by `set_target` rules only when tags are given).
pub fn set(d: &mut Document, id: &str, text: &str) {
    let u = d.units.iter_mut().find(|u| u.id == id).unwrap_or_else(|| panic!("no unit {id}"));
    let src: Vec<Inline> = u.segments[0].source.clone();
    let mut tags: Vec<(String, String)> = Vec::new();
    for i in &src {
        if let Inline::Tag { id, display, .. } = i {
            tags.push((id.clone(), display.clone()));
        }
    }
    // Put the source tags back where their text appears, so the target keeps its codes.
    let mut target = Vec::new();
    let mut rest = text;
    'outer: while !rest.is_empty() {
        let mut best: Option<(usize, usize)> = None;
        for (k, (_, d)) in tags.iter().enumerate() {
            if let Some(p) = rest.find(d.as_str()) {
                if best.is_none_or(|(bp, _)| p < bp) {
                    best = Some((p, k));
                }
            }
        }
        match best {
            Some((p, k)) => {
                if p > 0 {
                    target.push(Inline::text(&rest[..p]));
                }
                let (tid, d) = tags.remove(k);
                let free = src.iter().any(|i| i.is_free() && matches!(i, Inline::Tag { id, .. } if *id == tid));
                target.push(if free { Inline::free_tag(tid, d.clone()) } else { Inline::tag(tid, TagKind::Standalone, d.clone()) });
                rest = &rest[p + d.len()..];
            }
            None => {
                target.push(Inline::text(rest));
                break 'outer;
            }
        }
    }
    let seg = u.segments[0].id.clone();
    d.set_target(&seg, target).unwrap();
}

pub fn pseudo(d: &mut Document) {
    for u in d.units.iter_mut() {
        let mut t = vec![Inline::text("\u{ab}")];
        t.extend(u.segments[0].source.iter().cloned());
        t.push(Inline::text("\u{bb}"));
        u.segments[0].target = t;
        u.segments[0].state = SegState::Draft;
    }
}

// ---------------------------------------------------------------------------------------------------------------
// A small gettext reader: what CPython's `gettext.GNUTranslations` does, for checking the files the crate writes.

pub struct Gettext {
    pub messages: HashMap<Vec<u8>, Vec<u8>>,
    pub header: String,
    pub charset: String,
}

fn rd(b: &[u8], off: usize, big: bool) -> u32 {
    let x: [u8; 4] = b[off..off + 4].try_into().unwrap();
    if big {
        u32::from_be_bytes(x)
    } else {
        u32::from_le_bytes(x)
    }
}

impl Gettext {
    pub fn parse(b: &[u8]) -> Gettext {
        let big = match u32::from_le_bytes(b[..4].try_into().unwrap()) {
            0x9504_12de => false,
            0xde12_0495 => true,
            m => panic!("bad magic {m:#x}"),
        };
        let n = rd(b, 8, big) as usize;
        let (o, t) = (rd(b, 12, big) as usize, rd(b, 16, big) as usize);
        let mut messages = HashMap::new();
        for i in 0..n {
            let (kl, ko) = (rd(b, o + 8 * i, big) as usize, rd(b, o + 8 * i + 4, big) as usize);
            let (vl, vo) = (rd(b, t + 8 * i, big) as usize, rd(b, t + 8 * i + 4, big) as usize);
            assert_eq!(b[ko + kl], 0, "original {i} is not NUL terminated");
            assert_eq!(b[vo + vl], 0, "translation {i} is not NUL terminated");
            messages.insert(b[ko..ko + kl].to_vec(), b[vo..vo + vl].to_vec());
        }
        let header = messages.get(&b""[..]).map(|h| String::from_utf8_lossy(h).into_owned()).unwrap_or_default();
        let charset = header
            .lines()
            .find_map(|l| l.to_ascii_lowercase().strip_prefix("content-type:").and_then(|r| r.split("charset=").nth(1).map(|c| c.trim().to_string())))
            .unwrap_or_else(|| "utf-8".into());
        Gettext { messages, header, charset }
    }

    pub fn info(&self, field: &str) -> Option<String> {
        self.header.lines().find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim().eq_ignore_ascii_case(field).then(|| v.trim().to_string())
        })
    }

    /// The text of a stored string in the charset the header declares.
    pub fn text(&self, v: &[u8]) -> String {
        let enc = bilingua_model::encoding_rs::Encoding::for_label(self.charset.as_bytes()).unwrap_or(bilingua_model::encoding_rs::UTF_8);
        let (s, _, bad) = enc.decode(v);
        assert!(!bad, "stored string is not valid {}", self.charset);
        s.into_owned()
    }

    /// Looks a message up by its original text in the catalog's own charset.
    pub fn raw(&self, key: &str) -> Option<String> {
        let enc = bilingua_model::encoding_rs::Encoding::for_label(self.charset.as_bytes()).unwrap_or(bilingua_model::encoding_rs::UTF_8);
        let (k, _, _) = enc.encode(key);
        self.messages.get(&k.into_owned()).map(|v| self.text(v))
    }

    pub fn has(&self, msgid: &str) -> bool {
        self.messages.contains_key(msgid.as_bytes())
    }

    pub fn gettext(&self, msgid: &str) -> String {
        match self.messages.get(msgid.as_bytes()) {
            Some(v) => self.text(v).split('\0').next().unwrap().to_string(),
            None => msgid.to_string(),
        }
    }

    pub fn pgettext(&self, ctx: &str, msgid: &str) -> String {
        match self.messages.get(format!("{ctx}\u{4}{msgid}").as_bytes()) {
            Some(v) => self.text(v).split('\0').next().unwrap().to_string(),
            None => msgid.to_string(),
        }
    }

    pub fn plural_index(&self, n: i64) -> usize {
        let forms = self.info("plural-forms").unwrap_or_else(|| "nplurals=2; plural=(n != 1);".into());
        let expr = forms.split("plural=").nth(1).expect("plural=").trim().trim_end_matches(';');
        eval_plural(expr, n) as usize
    }

    pub fn ngettext(&self, msgid: &str, plural: &str, n: i64) -> String {
        let key = format!("{msgid}\0{plural}");
        if let Some(v) = self.messages.get(key.as_bytes()) {
            let slots: Vec<String> = self.text(v).split('\0').map(str::to_string).collect();
            if let Some(s) = slots.get(self.plural_index(n)) {
                return s.clone();
            }
        }
        if n == 1 { msgid.to_string() } else { plural.to_string() }
    }
}

/// Evaluates a gettext plural expression (the C operators gettext allows) for `n`.
pub fn eval_plural(expr: &str, n: i64) -> i64 {
    let toks = tokenize(expr);
    let mut p = Parser { toks, at: 0, n };
    let v = p.ternary();
    assert_eq!(p.at, p.toks.len(), "trailing tokens in {expr:?}");
    v
}

fn tokenize(s: &str) -> Vec<String> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
        } else if c.is_ascii_digit() {
            let j = b[i..].iter().position(|x| !x.is_ascii_digit()).map_or(b.len(), |p| i + p);
            out.push(s[i..j].to_string());
            i = j;
        } else if c == b'n' {
            out.push("n".into());
            i += 1;
        } else {
            let two = s.get(i..i + 2).unwrap_or("");
            if ["==", "!=", "<=", ">=", "&&", "||"].contains(&two) {
                out.push(two.to_string());
                i += 2;
            } else {
                out.push((c as char).to_string());
                i += 1;
            }
        }
    }
    out
}

struct Parser {
    toks: Vec<String>,
    at: usize,
    n: i64,
}

impl Parser {
    fn peek(&self) -> &str {
        self.toks.get(self.at).map_or("", String::as_str)
    }
    fn eat(&mut self, t: &str) -> bool {
        if self.peek() == t {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn ternary(&mut self) -> i64 {
        let c = self.or();
        if self.eat("?") {
            let a = self.ternary();
            assert!(self.eat(":"));
            let b = self.ternary();
            return if c != 0 { a } else { b };
        }
        c
    }
    fn or(&mut self) -> i64 {
        let mut v = self.and();
        while self.eat("||") {
            let r = self.and();
            v = i64::from(v != 0 || r != 0);
        }
        v
    }
    fn and(&mut self) -> i64 {
        let mut v = self.eq();
        while self.eat("&&") {
            let r = self.eq();
            v = i64::from(v != 0 && r != 0);
        }
        v
    }
    fn eq(&mut self) -> i64 {
        let mut v = self.rel();
        loop {
            if self.eat("==") {
                v = i64::from(v == self.rel());
            } else if self.eat("!=") {
                v = i64::from(v != self.rel());
            } else {
                return v;
            }
        }
    }
    fn rel(&mut self) -> i64 {
        let mut v = self.add();
        loop {
            if self.eat("<=") {
                v = i64::from(v <= self.add());
            } else if self.eat(">=") {
                v = i64::from(v >= self.add());
            } else if self.eat("<") {
                v = i64::from(v < self.add());
            } else if self.eat(">") {
                v = i64::from(v > self.add());
            } else {
                return v;
            }
        }
    }
    fn add(&mut self) -> i64 {
        let mut v = self.mul();
        loop {
            if self.eat("+") {
                v += self.mul();
            } else if self.eat("-") {
                v -= self.mul();
            } else {
                return v;
            }
        }
    }
    fn mul(&mut self) -> i64 {
        let mut v = self.unary();
        loop {
            if self.eat("*") {
                v *= self.unary();
            } else if self.eat("/") {
                v /= self.unary();
            } else if self.eat("%") {
                v %= self.unary();
            } else {
                return v;
            }
        }
    }
    fn unary(&mut self) -> i64 {
        if self.eat("!") {
            return i64::from(self.unary() == 0);
        }
        if self.eat("(") {
            let v = self.ternary();
            assert!(self.eat(")"));
            return v;
        }
        let t = self.peek().to_string();
        self.at += 1;
        if t == "n" {
            self.n
        } else {
            t.parse().unwrap_or_else(|_| panic!("bad token {t:?}"))
        }
    }
}

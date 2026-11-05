//! The file layer: header, cross-reference tables and streams, object streams, the repair scan, decryption and
//! the page tree. It follows PDFium's `CPDF_Parser` order of attempts: the cross references from `startxref`,
//! then (when they fail or the catalog does not load) a scan of the whole file for objects and trailers.
//!
//! Hostile files: a `/Prev` chain that loops is followed once; an object that refers to itself while it is being
//! read is null; object streams may not nest; every decoded byte is charged to one [`Budget`].

use crate::crypt::{CryptError, Handler};
use crate::decode::{self, Budget};
use crate::obj::{self, atoui, is_ws, Dict, Lexer, Obj, Parsed, Stream, Tok};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// PDFium's `CPDF_Parser::kMaxObjectNumber`.
const MAX_OBJECT_NUMBER: u32 = 24 * 1024 * 1024;
const MAX_PAGE_LEVEL: usize = 1024;
const MAX_READ_DEPTH: usize = 64;
/// PDFium's `CPDF_Document::kPageMaxNum`.
const PAGE_MAX_NUM: i64 = 0xFFFFF;

/// A decoded object stream: its data, the (number, offset) pairs of its header, and `/First`.
type ObjStmIndex = Rc<(Vec<u8>, Vec<(u32, usize)>, usize)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Entry {
    Offset(usize),
    InStream(u32, u32),
}

/// Why a file does not open (the PDFium error LocOS reports).
#[derive(Debug, PartialEq, Eq)]
pub enum OpenError {
    /// "Data format error": not a PDF, truncated before the trailer, no catalog, no pages.
    Format(&'static str),
    /// "Incorrect password error".
    Password,
    /// "Unsupported security scheme".
    Security,
    /// The decoded streams went over the byte budget.
    Budget,
}

pub struct Doc<'a> {
    pub data: &'a [u8],
    xref: HashMap<u32, Entry>,
    pub trailer: Dict,
    cache: RefCell<HashMap<u32, Obj>>,
    reading: RefCell<HashSet<u32>>,
    objstm: RefCell<HashMap<u32, ObjStmIndex>>,
    crypt: Option<Handler>,
    encrypt_num: u32,
    pub budget: RefCell<Budget>,
    /// Raw stream bytes still allowed (see [`Doc::charge_raw`]): the decoded budget plus eight times the file.
    raw_left: std::cell::Cell<u64>,
    /// Work left for fonts and glyph programs (see `fontprog::Work`); at zero the extraction is a limit error.
    pub work: crate::fontprog::Work,
}

/// Units of work a document may spend on font loads and charstring operations (about a few seconds).
pub const WORK_LIMIT: u64 = 1 << 30;

impl<'a> Doc<'a> {
    pub fn open(file: &'a [u8], budget: u64) -> Result<Doc<'a>, OpenError> {
        let head = file.len().min(1024);
        let off = find(&file[..head], b"%PDF", 0).ok_or(OpenError::Format("no PDF header"))?;
        let data = &file[off..];
        if data.len() < 9 {
            return Err(OpenError::Format("too short"));
        }
        let mut doc = Doc {
            data,
            xref: HashMap::new(),
            trailer: Dict::default(),
            cache: RefCell::new(HashMap::new()),
            reading: RefCell::new(HashSet::new()),
            objstm: RefCell::new(HashMap::new()),
            crypt: None,
            encrypt_num: 0,
            budget: RefCell::new(Budget::new(budget)),
            raw_left: std::cell::Cell::new(budget.saturating_add((data.len() as u64).saturating_mul(8))),
            work: Rc::new(std::cell::Cell::new(WORK_LIMIT)),
        };
        let start = doc.start_xref();
        let mut rebuilt = false;
        if start < 9 || !doc.load_xrefs(start) {
            if !doc.rebuild() {
                return Err(OpenError::Format("no cross reference"));
            }
            rebuilt = true;
        }
        doc.set_crypt()?;
        if !doc.root_ok() {
            if rebuilt {
                return Err(OpenError::Format("no catalog"));
            }
            doc.crypt = None;
            doc.cache.borrow_mut().clear();
            doc.objstm.borrow_mut().clear();
            if !doc.rebuild() {
                return Err(OpenError::Format("no cross reference"));
            }
            doc.set_crypt()?;
            if doc.root().is_none() {
                return Err(OpenError::Format("no catalog"));
            }
        }
        if doc.budget.borrow().exceeded {
            return Err(OpenError::Budget);
        }
        Ok(doc)
    }

    fn root(&self) -> Option<Rc<Dict>> {
        match self.trailer.get(b"Root").map(|o| self.resolve(o)) {
            Some(Obj::Dict(d)) => Some(d),
            _ => None,
        }
    }

    fn root_ok(&self) -> bool {
        self.root().is_some() && self.page_count() > 0
    }

    fn set_crypt(&mut self) -> Result<(), OpenError> {
        self.crypt = None;
        let enc_ref = self.trailer.get(b"Encrypt").cloned();
        let Some(enc_obj) = enc_ref else { return Ok(()) };
        if let Obj::Ref(n, _) = enc_obj {
            self.encrypt_num = n;
        }
        let enc = match self.resolve(&enc_obj) {
            Obj::Dict(d) => d,
            _ => return Ok(()),
        };
        let id = match self.trailer.get(b"ID").map(|o| self.resolve(o)) {
            Some(Obj::Array(a)) => a.first().and_then(|o| o.as_str()).map(|s| s.to_vec()).unwrap_or_default(),
            _ => Vec::new(),
        };
        let r = |o: &Obj| self.resolve(o);
        match Handler::open(&enc, &id, &r) {
            Ok(h) => {
                self.crypt = Some(h);
                // Objects read before the handler existed were not decrypted.
                self.cache.borrow_mut().clear();
                self.objstm.borrow_mut().clear();
                Ok(())
            }
            Err(CryptError::Password) => Err(OpenError::Password),
            Err(CryptError::Unsupported) => Err(OpenError::Security),
        }
    }

    // ------------------------------------------------------------ cross references

    fn start_xref(&self) -> usize {
        let d = self.data;
        let from = d.len().saturating_sub(4096 + 9);
        let Some(p) = rfind(&d[from..], b"startxref").map(|p| p + from) else { return 0 };
        let mut lx = Lexer::new(d);
        lx.pos = p + 9;
        match lx.next() {
            Tok::Word(w, true) => {
                let v = atoui(w) as usize;
                if v >= d.len() {
                    0
                } else {
                    v
                }
            }
            _ => 0,
        }
    }

    fn load_xrefs(&mut self, start: usize) -> bool {
        let mut seen = HashSet::new();
        let mut pos = start;
        let mut first = true;
        let mut xref: HashMap<u32, Entry> = HashMap::new();
        loop {
            if !seen.insert(pos) {
                break;
            }
            let Some((entries, trailer, stm)) = self.read_xref_section(pos) else { return false };
            for (n, e) in entries {
                xref.entry(n).or_insert(e);
            }
            if let Some(Obj::Int(s)) = trailer.get(b"XRefStm") {
                let s = *s as usize;
                if s > 0 && s < self.data.len() && seen.insert(s) {
                    if let Some((entries, _, _)) = self.read_xref_section(s) {
                        for (n, e) in entries {
                            xref.entry(n).or_insert(e);
                        }
                    }
                }
            }
            if first {
                self.trailer = trailer.clone();
                first = false;
            }
            let _ = stm;
            match trailer.get(b"Prev").and_then(|o| o.int()) {
                Some(p) if p > 0 && (p as usize) < self.data.len() => pos = p as usize,
                Some(p) if p < 0 => return false,
                _ => break,
            }
        }
        if !self.trailer.has(b"Root") {
            return false;
        }
        self.xref = xref;
        true
    }

    /// One classic table (with its trailer) or one cross-reference stream at `pos`.
    #[allow(clippy::type_complexity)]
    fn read_xref_section(&self, pos: usize) -> Option<(Vec<(u32, Entry)>, Dict, bool)> {
        let d = self.data;
        let mut lx = Lexer::new(d);
        lx.pos = pos;
        lx.skip_ws();
        if d[lx.pos..].starts_with(b"xref") {
            lx.pos += 4;
            let mut entries = Vec::new();
            loop {
                match lx.next() {
                    Tok::Word(w, true) => {
                        let start = atoui(w);
                        let Tok::Word(c, true) = lx.next() else { return None };
                        let count = atoui(c);
                        if start > MAX_OBJECT_NUMBER || count > MAX_OBJECT_NUMBER {
                            return None;
                        }
                        for i in 0..count {
                            let Tok::Word(o, true) = lx.next() else { return None };
                            let Tok::Word(_g, true) = lx.next() else { return None };
                            let Tok::Word(t, _) = lx.next() else { return None };
                            let n = start + i;
                            if n > MAX_OBJECT_NUMBER {
                                return None;
                            }
                            if t == b"n" {
                                let off = atoui(o) as usize;
                                if off > 0 && off < d.len() {
                                    entries.push((n, Entry::Offset(off)));
                                } else {
                                    entries.push((n, Entry::Offset(usize::MAX)));
                                }
                            } else if t != b"f" {
                                return None;
                            } else {
                                entries.push((n, Entry::Offset(usize::MAX)));
                            }
                        }
                    }
                    Tok::Word(b"trailer", _) => {
                        let Some(Parsed::Obj(Obj::Dict(t))) = obj::parse_obj(&mut lx, 0, true) else { return None };
                        return Some((entries, (*t).clone(), false));
                    }
                    _ => return None,
                }
            }
        }
        // A cross-reference stream.
        let (num, o) = self.read_indirect_at(pos)?;
        let Obj::Stream(s) = o else { return None };
        if s.dict.get(b"Type").and_then(|t| t.as_name()) != Some(b"XRef") {
            return None;
        }
        let size = s.dict.get(b"Size").and_then(|o| o.int()).unwrap_or(0);
        if !(0..=MAX_OBJECT_NUMBER as i64 + 1).contains(&size) {
            return None;
        }
        let w: Vec<usize> = s.dict.get(b"W")?.as_array()?.iter().map(|x| x.int().unwrap_or(0).clamp(0, 8) as usize).collect();
        if w.len() < 3 {
            return None;
        }
        let index: Vec<(u32, u32)> = match s.dict.get(b"Index").and_then(|o| o.as_array()) {
            Some(a) if a.len() >= 2 => a.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0].int().unwrap_or(0).max(0) as u32, c[1].int().unwrap_or(0).max(0) as u32)).collect(),
            _ => vec![(0, size as u32)],
        };
        let r = |o: &Obj| self.resolve(o);
        let raw = decode::decode(&s.dict, &s.raw, &r, &mut self.budget.borrow_mut()).unwrap_or_default();
        let row: usize = w.iter().sum();
        let mut entries = Vec::new();
        let mut at = 0;
        let field = |b: &[u8], len: usize| -> u64 { b.iter().take(len).fold(0u64, |a, x| (a << 8) | *x as u64) };
        for (start, count) in index {
            for i in 0..count {
                if row == 0 || at + row > raw.len() {
                    break;
                }
                let r = &raw[at..at + row];
                at += row;
                let n = start.saturating_add(i);
                if n > MAX_OBJECT_NUMBER {
                    break;
                }
                let t = if w[0] == 0 { 1 } else { field(r, w[0]) };
                let f2 = field(&r[w[0]..], w[1]);
                let f3 = field(&r[w[0] + w[1]..], w[2]);
                match t {
                    0 => entries.push((n, Entry::Offset(usize::MAX))),
                    1 => entries.push((n, Entry::Offset(if f2 > 0 && (f2 as usize) < self.data.len() { f2 as usize } else { usize::MAX }))),
                    2 => entries.push((n, Entry::InStream(f2.min(u32::MAX as u64) as u32, f3.min(u32::MAX as u64) as u32))),
                    _ => {}
                }
            }
        }
        let _ = num;
        Some((entries, s.dict.clone(), true))
    }

    /// PDFium's `RebuildCrossRef`: every `N G obj` in the file (the last one wins), the objects inside object
    /// streams found on the way, and the trailers (later keys win).
    fn rebuild(&mut self) -> bool {
        let d = self.data;
        let mut lx = Lexer::new(d);
        let mut nums: Vec<(u32, usize)> = Vec::new();
        let mut xref: HashMap<u32, Entry> = HashMap::new();
        let mut trailer = Dict::default();
        let mut have_trailer = false;
        loop {
            let at = {
                lx.skip_ws();
                lx.pos
            };
            match lx.next() {
                Tok::Eof => break,
                Tok::Word(w, true) => {
                    nums.push((atoui(w), at));
                    if nums.len() > 2 {
                        nums.remove(0);
                    }
                    continue;
                }
                Tok::StrStart => {
                    lx.read_string();
                }
                Tok::HexStart => {
                    lx.read_hex();
                }
                Tok::Word(b"trailer", _) => {
                    if let Some(Parsed::Obj(Obj::Dict(t))) = obj::parse_obj(&mut lx, 0, true) {
                        for (k, v) in t.items().iter() {
                            trailer.insert(k, v.clone());
                        }
                        have_trailer = true;
                    }
                }
                Tok::Word(b"obj", _) if nums.len() == 2 => {
                    let (num, pos) = nums[0];
                    if num > 0 && num <= MAX_OBJECT_NUMBER {
                        if let Some((_, o, end)) = self.read_indirect_full(pos) {
                            lx.pos = end.max(lx.pos);
                            if let Obj::Stream(s) = &o {
                                if s.dict.get(b"Type").and_then(|t| t.as_name()) == Some(b"XRef") {
                                    for (k, v) in s.dict.items().iter() {
                                        if !matches!(k.as_slice(), b"Type" | b"W" | b"Index" | b"Length" | b"Filter" | b"DecodeParms" | b"Prev") {
                                            trailer.insert(k, v.clone());
                                        }
                                    }
                                    have_trailer = true;
                                }
                                if s.dict.get(b"Type").and_then(|t| t.as_name()) == Some(b"ObjStm") {
                                    if let Some(idx) = self.objstm_index(num, s) {
                                        for (i, (n, _)) in idx.1.iter().enumerate() {
                                            xref.entry(*n).or_insert(Entry::InStream(num, i as u32));
                                        }
                                    }
                                }
                            }
                        }
                        xref.insert(num, Entry::Offset(pos));
                    }
                }
                _ => {}
            }
            nums.clear();
        }
        // An object found at its offset wins over the same number inside an object stream.
        if !have_trailer || xref.is_empty() {
            return false;
        }
        self.trailer = trailer;
        self.xref = xref;
        self.cache.borrow_mut().clear();
        true
    }

    // ------------------------------------------------------------ objects

    pub fn resolve(&self, o: &Obj) -> Obj {
        match o {
            Obj::Ref(n, _) => self.get(*n),
            _ => o.clone(),
        }
    }

    pub fn get(&self, num: u32) -> Obj {
        if let Some(o) = self.cache.borrow().get(&num) {
            return o.clone();
        }
        // A chain of objects each needed to read the next (a /Length that is a reference, an object stream) is
        // bounded, and an object that needs itself is null.
        if self.reading.borrow().len() > MAX_READ_DEPTH || !self.reading.borrow_mut().insert(num) {
            return Obj::Null;
        }
        let o = match self.xref.get(&num).copied() {
            Some(Entry::Offset(off)) if off != usize::MAX => match self.read_indirect_at(off) {
                Some((n, o)) if n == num => o,
                _ => Obj::Null,
            },
            Some(Entry::InStream(stm, idx)) => self.read_from_objstm(stm, idx, num),
            _ => Obj::Null,
        };
        self.reading.borrow_mut().remove(&num);
        self.cache.borrow_mut().insert(num, o.clone());
        o
    }

    fn read_indirect_at(&self, pos: usize) -> Option<(u32, Obj)> {
        self.read_indirect_full(pos).map(|(n, o, _)| (n, o))
    }

    /// `N G obj <object> [stream ... endstream]`; returns the number, the object and the end position.
    fn read_indirect_full(&self, pos: usize) -> Option<(u32, Obj, usize)> {
        let d = self.data;
        if pos >= d.len() {
            return None;
        }
        let mut lx = Lexer::new(d);
        lx.pos = pos;
        let Tok::Word(n, true) = lx.next() else { return None };
        let Tok::Word(g, true) = lx.next() else { return None };
        let Tok::Word(b"obj", _) = lx.next() else { return None };
        let num = atoui(n);
        let gen = atoui(g).min(u16::MAX as u32) as u16;
        let parsed = obj::parse_obj(&mut lx, 0, true)?;
        let o = match parsed {
            Parsed::Obj(o) => self.decrypt_obj(o, num, gen),
            Parsed::StreamDict(dict) => {
                let dict = self.decrypt_dict(dict, num, gen);
                let (raw, end) = self.read_stream_data(&dict, lx.pos, num);
                lx.pos = end;
                Obj::Stream(Rc::new(Stream { dict, raw, num, gen }))
            }
        };
        Some((num, o, lx.pos))
    }

    /// The data after the `stream` keyword: `/Length` bytes when they end at `endstream`, else up to the next
    /// `endstream` (or `endobj`) with the end-of-line before it dropped.
    fn read_stream_data(&self, dict: &Dict, after_kw: usize, num: u32) -> (Vec<u8>, usize) {
        let d = self.data;
        let mut start = after_kw;
        if d.get(start) == Some(&b'\r') {
            start += 1;
        }
        if d.get(start) == Some(&b'\n') {
            start += 1;
        }
        let len = match dict.get(b"Length") {
            Some(Obj::Ref(n, _)) if *n != num => self.get(*n).int(),
            Some(o) => o.int(),
            None => None,
        };
        if let Some(len) = len {
            if len >= 0 {
                let end = start.saturating_add(len as usize);
                if end <= d.len() {
                    let mut p = end;
                    while p < d.len() && is_ws(d[p]) {
                        p += 1;
                    }
                    if d[p..].starts_with(b"endstream") {
                        return (self.copy_raw(&d[start..end]), p + 9);
                    }
                }
            }
        }
        // `endobj` only counts before the first `endstream`: looking for it past that point would scan the rest
        // of the file for every such stream.
        let es = find(d, b"endstream", start);
        let eo = find(&d[..es.unwrap_or(d.len())], b"endobj", start);
        let stop = match (es, eo) {
            (Some(a), Some(b)) => a.min(b),
            (Some(a), None) | (None, Some(a)) => a,
            (None, None) => d.len(),
        };
        let mut end = stop;
        if end > start && d[end - 1] == b'\n' {
            end -= 1;
        }
        if end > start && d[end - 1] == b'\r' {
            end -= 1;
        }
        let next = if es == Some(stop) { stop + 9 } else { stop };
        (self.copy_raw(&d[start..end.max(start)]), next)
    }

    /// Charges raw stream bytes read, copied or decrypted to the raw budget. A file can make every one of many
    /// streams run to its end (no `endstream`), or use one large stream many times; without this, the copies and
    /// scans grow with the square of the file. Over the budget the extraction ends with the stream limit error.
    fn charge_raw(&self, n: usize) -> bool {
        let left = self.raw_left.get();
        if n as u64 > left {
            self.raw_left.set(0);
            self.budget.borrow_mut().exceeded = true;
            return false;
        }
        self.raw_left.set(left - n as u64);
        true
    }

    fn copy_raw(&self, b: &[u8]) -> Vec<u8> {
        if self.charge_raw(b.len()) {
            b.to_vec()
        } else {
            Vec::new()
        }
    }

    fn objstm_index(&self, num: u32, s: &Stream) -> Option<ObjStmIndex> {
        if let Some(c) = self.objstm.borrow().get(&num) {
            return Some(c.clone());
        }
        let n = s.dict.get(b"N").and_then(|o| o.int()).filter(|n| *n >= 0)? as usize;
        let first = s.dict.get(b"First").and_then(|o| o.int()).filter(|n| *n >= 0)? as usize;
        let data = self.stream_data(s)?;
        if first > data.len() {
            return None;
        }
        let mut lx = Lexer::new(&data[..first]);
        let mut idx = Vec::new();
        for _ in 0..n.min(MAX_OBJECT_NUMBER as usize) {
            let Tok::Word(a, true) = lx.next() else { break };
            let Tok::Word(b, true) = lx.next() else { break };
            let on = atoui(a);
            if on == 0 || on > MAX_OBJECT_NUMBER {
                continue;
            }
            idx.push((on, atoui(b) as usize));
        }
        let rc = Rc::new((data, idx, first));
        self.objstm.borrow_mut().insert(num, rc.clone());
        Some(rc)
    }

    fn read_from_objstm(&self, stm: u32, idx: u32, num: u32) -> Obj {
        if stm == num {
            return Obj::Null;
        }
        // An object stream may not live in an object stream.
        if let Some(Entry::InStream(..)) = self.xref.get(&stm) {
            return Obj::Null;
        }
        let Obj::Stream(s) = self.get(stm) else { return Obj::Null };
        if s.dict.get(b"Type").and_then(|t| t.as_name()) != Some(b"ObjStm") {
            return Obj::Null;
        }
        let Some(ix) = self.objstm_index(stm, &s) else { return Obj::Null };
        let (data, list, first) = (&ix.0, &ix.1, ix.2);
        let Some(&(on, off)) = list.get(idx as usize) else { return Obj::Null };
        if on != num {
            return Obj::Null;
        }
        let at = first.saturating_add(off);
        if at >= data.len() {
            return Obj::Null;
        }
        let mut lx = Lexer::new(data);
        lx.pos = at;
        match obj::parse_obj(&mut lx, 0, true) {
            Some(Parsed::Obj(o)) => o,
            _ => Obj::Null,
        }
    }

    /// The decrypted, decoded data of a stream. `None` for an image codec or unknown filter.
    pub fn stream_data(&self, s: &Stream) -> Option<Vec<u8>> {
        if !self.charge_raw(s.raw.len()) {
            return None;
        }
        let raw: std::borrow::Cow<[u8]> = match &self.crypt {
            Some(h) if s.num != 0 && s.num != self.encrypt_num && !self.skip_decrypt(&s.dict) => std::borrow::Cow::Owned(h.decrypt(s.num, s.gen, &s.raw)),
            _ => std::borrow::Cow::Borrowed(&s.raw),
        };
        let r = |o: &Obj| self.resolve(o);
        decode::decode(&s.dict, &raw, &r, &mut self.budget.borrow_mut())
    }

    fn skip_decrypt(&self, d: &Dict) -> bool {
        let t = d.get(b"Type").and_then(|t| t.as_name());
        if t == Some(b"XRef") {
            return true;
        }
        if t == Some(b"Metadata") && self.crypt.as_ref().is_some_and(|h| !h.encrypt_metadata) {
            return true;
        }
        false
    }

    fn decrypt_obj(&self, o: Obj, num: u32, gen: u16) -> Obj {
        let Some(h) = &self.crypt else { return o };
        if num == self.encrypt_num {
            return o;
        }
        match o {
            Obj::Str(s) => Obj::Str(Rc::new(h.decrypt(num, gen, &s))),
            Obj::Array(a) => Obj::Array(Rc::new(a.iter().map(|x| self.decrypt_obj(x.clone(), num, gen)).collect())),
            Obj::Dict(d) => Obj::Dict(Rc::new(self.decrypt_dict((*d).clone(), num, gen))),
            o => o,
        }
    }

    fn decrypt_dict(&self, mut d: Dict, num: u32, gen: u16) -> Dict {
        if self.crypt.is_none() || num == self.encrypt_num {
            return d;
        }
        for v in d.values_mut() {
            *v = self.decrypt_obj(std::mem::replace(v, Obj::Null), num, gen);
        }
        d
    }

    // ------------------------------------------------------------ pages

    pub fn page_count(&self) -> usize {
        let Some(root) = self.root() else { return 0 };
        let Some(Obj::Dict(pages)) = root.get(b"Pages").map(|o| self.resolve(o)) else { return 0 };
        if !pages.has(b"Kids") {
            return 1;
        }
        let c = pages.get(b"Count").map(|o| self.resolve(o)).and_then(|o| o.int()).unwrap_or(0);
        if c > 0 && c < PAGE_MAX_NUM {
            return c as usize;
        }
        let mut seen = HashSet::new();
        self.count_leaves(&pages, &mut seen, 0).unwrap_or(0)
    }

    fn count_leaves(&self, node: &Dict, seen: &mut HashSet<*const Dict>, level: usize) -> Option<usize> {
        if level >= MAX_PAGE_LEVEL {
            return None;
        }
        let c = node.get(b"Count").map(|o| self.resolve(o)).and_then(|o| o.int()).unwrap_or(0);
        if c > 0 && c < PAGE_MAX_NUM && level > 0 {
            return Some(c as usize);
        }
        let Some(Obj::Array(kids)) = node.get(b"Kids").map(|o| self.resolve(o)) else { return Some(0) };
        let mut n = 0usize;
        for k in kids.iter() {
            let Obj::Dict(kd) = self.resolve(k) else { continue };
            if !seen.insert(Rc::as_ptr(&kd)) {
                continue;
            }
            if is_branch(&kd) {
                n += self.count_leaves(&kd, seen, level + 1)?;
            } else {
                n += 1;
            }
            if n as i64 >= PAGE_MAX_NUM {
                return None;
            }
        }
        Some(n)
    }

    /// The page dictionaries in order (`None` for a slot whose object is not a dictionary), with the inherited
    /// attributes (`Resources`, `MediaBox`, `CropBox`, `Rotate`) of the page tree.
    pub fn pages(&self, want: usize) -> Vec<Option<Page>> {
        let mut out: Vec<Option<Page>> = Vec::new();
        let Some(root) = self.root() else { return out };
        let Some(Obj::Dict(pages)) = root.get(b"Pages").map(|o| self.resolve(o)) else { return out };
        if !pages.has(b"Kids") {
            out.push(Some(self.page_of(&pages, &[])));
            return out;
        }
        let mut stack: Vec<Rc<Dict>> = vec![pages.clone()];
        let mut seen: HashSet<*const Dict> = HashSet::new();
        seen.insert(Rc::as_ptr(&pages));
        self.walk(&pages, &mut stack, &mut out, want, 0, &mut seen);
        out
    }

    /// A branch is walked once: a page tree whose branches share children (a DAG, not a tree) would otherwise be
    /// walked once per path, exponentially many times.
    fn walk(&self, node: &Rc<Dict>, stack: &mut Vec<Rc<Dict>>, out: &mut Vec<Option<Page>>, want: usize, level: usize, seen: &mut HashSet<*const Dict>) {
        if level >= MAX_PAGE_LEVEL || out.len() >= want {
            return;
        }
        let Some(Obj::Array(kids)) = node.get(b"Kids").map(|o| self.resolve(o)) else { return };
        for k in kids.iter() {
            if out.len() >= want {
                return;
            }
            let Obj::Dict(kd) = self.resolve(k) else {
                out.push(None);
                continue;
            };
            if Rc::ptr_eq(&kd, node) || stack.iter().any(|s| Rc::ptr_eq(s, &kd)) {
                continue;
            }
            if kd.has(b"Kids") {
                if !seen.insert(Rc::as_ptr(&kd)) {
                    continue;
                }
                stack.push(kd.clone());
                self.walk(&kd, stack, out, want, level + 1, seen);
                stack.pop();
            } else {
                out.push(Some(self.page_of(&kd, stack)));
            }
        }
    }

    fn page_of(&self, page: &Rc<Dict>, ancestors: &[Rc<Dict>]) -> Page {
        let attr = |k: &[u8]| -> Option<Obj> {
            if let Some(v) = page.get(k) {
                return Some(self.resolve(v));
            }
            ancestors.iter().rev().find_map(|a| a.get(k).map(|v| self.resolve(v)))
        };
        let rect = |o: Option<Obj>| -> Option<[f64; 4]> {
            let o = o?;
            let a = o.as_array()?;
            if a.len() < 4 {
                return None;
            }
            let v: Vec<f64> = a.iter().take(4).map(|x| self.resolve(x).num().unwrap_or(0.0)).collect();
            Some([v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])])
        };
        let resources = match attr(b"Resources") {
            Some(Obj::Dict(d)) => Some(d),
            _ => None,
        };
        Page {
            dict: page.clone(),
            resources,
            media: rect(attr(b"MediaBox")),
            crop: rect(attr(b"CropBox")),
            rotate: attr(b"Rotate").and_then(|o| o.int()).unwrap_or(0),
        }
    }

    /// The page's content streams, decoded and joined with a space (PDFium's `CPDF_ContentParser`).
    pub fn page_content(&self, page: &Page) -> Vec<u8> {
        match page.dict.get(b"Contents").map(|o| self.resolve(o)) {
            Some(Obj::Stream(s)) => self.stream_data(&s).unwrap_or_default(),
            Some(Obj::Array(a)) => {
                let mut out = Vec::new();
                for x in a.iter() {
                    if let Obj::Stream(s) = self.resolve(x) {
                        if let Some(d) = self.stream_data(&s) {
                            out.extend_from_slice(&d);
                            out.push(b' ');
                        }
                    }
                }
                out
            }
            _ => Vec::new(),
        }
    }

    /// `/Direction /R2L` in the viewer preferences (PDFium makes the text page right to left).
    pub fn rtl(&self) -> bool {
        let Some(root) = self.root() else { return false };
        match root.get(b"ViewerPreferences").map(|o| self.resolve(o)) {
            Some(Obj::Dict(v)) => v.get(b"Direction").map(|o| self.resolve(o)).and_then(|o| o.as_name().map(|n| n == b"R2L")).unwrap_or(false),
            _ => false,
        }
    }
}

fn is_branch(d: &Dict) -> bool {
    match d.get(b"Type").and_then(|t| t.as_name()) {
        Some(b"Pages") => true,
        Some(b"Page") => false,
        _ => d.has(b"Kids"),
    }
}

pub struct Page {
    pub dict: Rc<Dict>,
    pub resources: Option<Rc<Dict>>,
    pub media: Option<[f64; 4]>,
    pub crop: Option<[f64; 4]>,
    pub rotate: i64,
}

impl Page {
    /// PDFium's page box: the crop box inside the media box (Letter when there is no usable media box).
    pub fn bbox(&self) -> [f64; 4] {
        let mut media = self.media.unwrap_or([0.0, 0.0, 612.0, 792.0]);
        if media[2] - media[0] <= 0.0 || media[3] - media[1] <= 0.0 {
            media = [0.0, 0.0, 612.0, 792.0];
        }
        match self.crop {
            Some(c) if c[2] > c[0] && c[3] > c[1] => {
                let r = [c[0].max(media[0]), c[1].max(media[1]), c[2].min(media[2]), c[3].min(media[3])];
                if r[2] < r[0] || r[3] < r[1] {
                    [0.0, 0.0, 0.0, 0.0]
                } else {
                    r
                }
            }
            _ => media,
        }
    }

    pub fn rotation(&self) -> i64 {
        let r = (self.rotate / 90) % 4;
        if r < 0 {
            r + 4
        } else {
            r
        }
    }

    /// Width and height as PDFium reports them (swapped for a quarter turn).
    pub fn size(&self) -> (f64, f64) {
        let b = self.bbox();
        let (w, h) = (b[2] - b[0], b[3] - b[1]);
        if self.rotation() % 2 == 1 {
            (h, w)
        } else {
            (w, h)
        }
    }
}

pub fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from >= hay.len() || needle.is_empty() {
        return None;
    }
    let first = needle[0];
    let mut i = from;
    while i + needle.len() <= hay.len() {
        match hay[i..].iter().position(|b| *b == first) {
            None => return None,
            Some(p) => {
                i += p;
                if i + needle.len() <= hay.len() && &hay[i..i + needle.len()] == needle {
                    return Some(i);
                }
                i += 1;
            }
        }
    }
    None
}

fn rfind(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if hay.len() < needle.len() {
        return None;
    }
    (0..=hay.len() - needle.len()).rev().find(|&i| &hay[i..i + needle.len()] == needle)
}

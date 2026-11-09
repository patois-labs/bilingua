//! Binary property lists (`bplist00`): a reader into the same [`Tree`] as XML and a writer for the whole tree.
//! As in Localization OS a binary file is rebuilt from the tree when a translation changes it, so it is
//! semantically the same file (every non-string value kept), not the same bytes; an untouched file is returned as
//! it came.

use crate::tree::*;
use crate::xml::{perr, too_deep, too_many, MAX_DEPTH};
use bilingua_model::*;
use std::collections::HashMap;

struct Reader<'a> {
    raw: &'a [u8],
    fmt: &'static str,
    offset_size: usize,
    ref_size: usize,
    num: usize,
    table: usize,
    max_nodes: usize,
    max_units: usize,
    /// Bytes of strings and data still allowed to be copied out of the file (a shared string counts every use).
    text_budget: usize,
    tree: Tree,
    /// Containers and UIDs already read, by object number: (tree node, nodes it stands for, height, text budget it
    /// spent). plistlib hands back the same Python object for an object number it read before, so a container or UID
    /// the file references twice is one value that appears twice; the tree shares the node the same way. A string is
    /// not shared: a translation lands on one place of the tree, and a repeated string is another string.
    shared: HashMap<usize, (usize, usize, usize, usize)>,
    /// Object numbers being read now (a reference back to one of them is a cycle).
    busy: std::collections::HashSet<usize>,
    /// Nodes the tree would hold if every reference were expanded (the node limit counts these).
    expanded: usize,
    /// The deepest level reached so far.
    deepest: usize,
}

fn be(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0u64, |a, b| (a << 8) | *b as u64)
}

impl<'a> Reader<'a> {
    fn bad(&self, why: &str) -> Error {
        perr(self.fmt, format!("not a readable binary property list ({why})"))
    }

    fn slice(&self, at: usize, len: usize) -> Result<&'a [u8], Error> {
        at.checked_add(len).and_then(|e| self.raw.get(at..e)).ok_or_else(|| self.bad("an object runs past the end of the file"))
    }

    fn offset_of(&self, i: usize) -> Result<usize, Error> {
        let at = self.table + i * self.offset_size;
        let off = be(self.slice(at, self.offset_size)?) as usize;
        if off < 8 || off >= self.table {
            return Err(self.bad("an object offset is outside the file body"));
        }
        Ok(off)
    }

    fn r#ref(&self, at: usize) -> Result<usize, Error> {
        let r = be(self.slice(at, self.ref_size)?) as usize;
        if r >= self.num {
            return Err(self.bad("a reference points past the object table"));
        }
        Ok(r)
    }

    /// Marker byte, count and the offset of the data that follows.
    fn header(&self, off: usize) -> Result<(u8, usize, usize), Error> {
        let m = self.slice(off, 1)?[0];
        let (hi, lo) = (m >> 4, m & 0xF);
        if !matches!(hi, 0x4 | 0x5 | 0x6 | 0xA | 0xD) {
            return Ok((hi, lo as usize, off + 1));
        }
        if lo != 0xF {
            return Ok((hi, lo as usize, off + 1));
        }
        let im = self.slice(off + 1, 1)?[0];
        if im >> 4 != 1 || (im & 0xF) > 3 {
            return Err(self.bad("a length is not an integer"));
        }
        let size = 1usize << (im & 0xF);
        let n = be(self.slice(off + 2, size)?);
        let n = usize::try_from(n).map_err(|_| self.bad("a length is too large"))?;
        Ok((hi, n, off + 2 + size))
    }

    fn spend(&mut self, n: usize) -> Result<(), Error> {
        self.text_budget = self.text_budget.checked_sub(n).ok_or_else(|| too_many(self.max_units))?;
        Ok(())
    }

    fn string_at(&mut self, i: usize) -> Result<String, Error> {
        let off = self.offset_of(i)?;
        let (hi, n, data) = self.header(off)?;
        match hi {
            0x5 => {
                let s = self.slice(data, n)?;
                self.spend(n)?;
                if !s.is_ascii() {
                    return Err(self.bad("an ASCII string holds other bytes"));
                }
                Ok(String::from_utf8_lossy(s).into_owned())
            }
            0x6 => {
                let len = n.checked_mul(2).ok_or_else(|| self.bad("a length is too large"))?;
                self.spend(len)?;
                let s = self.slice(data, len)?;
                let units: Vec<u16> = s.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
                String::from_utf16(&units).map_err(|_| self.bad("a string is not valid UTF-16"))
            }
            _ => Err(perr(self.fmt, "property list has a non-string dictionary key")),
        }
    }

    fn object(&mut self, i: usize, depth: usize) -> Result<usize, Error> {
        if depth > MAX_DEPTH {
            return Err(too_deep());
        }
        self.deepest = self.deepest.max(depth);
        if let Some(&(idx, size, height, cost)) = self.shared.get(&i) {
            // read before: the same value again, counted as if it were expanded (a reference bomb stays refused)
            if depth + height > MAX_DEPTH {
                return Err(too_deep());
            }
            if self.expanded.saturating_add(size) > self.max_nodes {
                return Err(too_many(self.max_units));
            }
            self.expanded += size;
            self.deepest = self.deepest.max(depth + height);
            self.spend(cost)?;
            return Ok(idx);
        }
        if self.expanded >= self.max_nodes {
            return Err(too_many(self.max_units));
        }
        self.expanded += 1;
        let off = self.offset_of(i)?;
        let (hi, n, data) = self.header(off)?;
        if !matches!(hi, 0xA | 0xD) {
            // scalars and strings in their own frame: the recursive one stays small (deep nesting in a
            // debug build on a 1 MB main-thread stack)
            let node = self.leaf(i, off, hi, n, data)?;
            let idx = self.tree.nodes.len();
            self.tree.nodes.push(node);
            if hi == 0x8 {
                self.shared.insert(i, (idx, 1, 0, 0));
            }
            return Ok(idx);
        }
        if !self.busy.insert(i) {
            return Err(too_deep());
        }
        let (nodes0, spent0) = (self.expanded, self.text_budget);
        let outer = std::mem::replace(&mut self.deepest, depth);
        let idx = self.container(hi, n, data, depth)?;
        let (size, height) = (self.expanded - nodes0 + 1, self.deepest - depth);
        self.deepest = self.deepest.max(outer);
        self.busy.remove(&i);
        self.shared.insert(i, (idx, size, height, spent0 - self.text_budget));
        Ok(idx)
    }

    fn container(&mut self, hi: u8, n: usize, data: usize, depth: usize) -> Result<usize, Error> {
        let mut node = Node::new(Kind::Other);
        match hi {
            0xA => {
                node.kind = Kind::Array;
                let total = n.checked_mul(self.ref_size).ok_or_else(|| self.bad("a length is too large"))?;
                let refs = self.slice(data, total)?;
                let idx = self.tree.nodes.len();
                self.tree.nodes.push(node);
                for k in 0..n {
                    let r = be(&refs[k * self.ref_size..(k + 1) * self.ref_size]) as usize;
                    if r >= self.num {
                        return Err(self.bad("a reference points past the object table"));
                    }
                    let c = self.object(r, depth + 1)?;
                    self.tree.nodes[idx].kids.push(Kid { key: None, kstart: NO_SPAN, kend: NO_SPAN, node: c });
                }
                Ok(idx)
            }
            0xD => {
                node.kind = Kind::Dict;
                let total = n.checked_mul(self.ref_size * 2).ok_or_else(|| self.bad("a length is too large"))?;
                let refs = self.slice(data, total)?;
                let idx = self.tree.nodes.len();
                self.tree.nodes.push(node);
                // key -> place among the kids, so a dict of many distinct keys reads in linear time
                let mut place: HashMap<String, usize> = HashMap::new();
                for k in 0..n {
                    let kr = self.r#ref(data + k * self.ref_size)?;
                    let key = self.string_at(kr)?;
                    let vr = be(&refs[(n + k) * self.ref_size..(n + k + 1) * self.ref_size]) as usize;
                    if vr >= self.num {
                        return Err(self.bad("a reference points past the object table"));
                    }
                    let c = self.object(vr, depth + 1)?;
                    // a repeated key: the last value wins, in the first key's place (a Python dict)
                    let kids = &mut self.tree.nodes[idx].kids;
                    match place.get(&key) {
                        Some(&p) => kids[p].node = c,
                        None => {
                            place.insert(key.clone(), kids.len());
                            kids.push(Kid { key: Some(key), kstart: NO_SPAN, kend: NO_SPAN, node: c });
                        }
                    }
                }
                Ok(idx)
            }
            _ => Err(self.bad("an unsupported object type")),
        }
    }

    #[inline(never)]
    fn leaf(&mut self, i: usize, off: usize, hi: u8, n: usize, data: usize) -> Result<Node, Error> {
        let marker = self.slice(off, 1)?[0];
        let mut node = Node::new(Kind::Other);
        match hi {
            0x0 => match marker {
                0x00 => node.scalar = Scalar::Null,
                0x08 => node.scalar = Scalar::Bool(false),
                0x09 => node.scalar = Scalar::Bool(true),
                0x0F => node.scalar = Scalar::Data(Vec::new()),
                _ => return Err(self.bad("an unknown object type")),
            },
            0x1 => {
                let size = match n {
                    0..=3 => 1usize << n,
                    4 => 16,
                    _ => return Err(self.bad("an integer is too wide")),
                };
                let b = self.slice(off + 1, size)?;
                node.scalar = Scalar::Int(match size {
                    1 | 2 | 4 => be(b) as i128,
                    8 => i64::from_be_bytes(b.try_into().map_err(|_| self.bad("an integer is cut off"))?) as i128,
                    _ => i128::from_be_bytes(b.try_into().map_err(|_| self.bad("an integer is cut off"))?),
                });
            }
            0x2 => {
                let b = self.slice(off + 1, 1usize << n.min(4))?;
                node.scalar = Scalar::Real(match n {
                    2 => f32::from_be_bytes(b.try_into().map_err(|_| self.bad("a real is cut off"))?) as f64,
                    3 => f64::from_be_bytes(b.try_into().map_err(|_| self.bad("a real is cut off"))?),
                    _ => return Err(self.bad("a real has an unsupported size")),
                });
            }
            0x3 => {
                if n != 3 {
                    return Err(self.bad("a date has an unsupported size"));
                }
                let b = self.slice(off + 1, 8)?;
                node.scalar = Scalar::Date(f64::from_be_bytes(b.try_into().map_err(|_| self.bad("a date is cut off"))?));
            }
            0x4 => {
                self.spend(n)?;
                node.scalar = Scalar::Data(self.slice(data, n)?.to_vec());
            }
            0x5 | 0x6 => {
                node = Node::string(self.string_at(i)?);
            }
            0x8 => {
                let size = n + 1;
                if size > 8 {
                    return Err(self.bad("a UID is too wide"));
                }
                node.scalar = Scalar::Uid(be(self.slice(off + 1, size)?));
            }
            _ => return Err(self.bad("an unsupported object type")),
        }
        Ok(node)
    }
}

/// Reads a `bplist00` file.
pub(crate) fn read(raw: &[u8], fmt: &'static str, lim: &Limits) -> Result<Tree, Error> {
    if !raw.starts_with(b"bplist") {
        return Err(perr(fmt, "not a binary property list"));
    }
    if !raw.starts_with(b"bplist00") {
        return Err(perr(fmt, "unsupported binary property list version (only bplist00)"));
    }
    let bad = |why: &str| perr(fmt, format!("not a readable binary property list ({why})"));
    if raw.len() < 8 + 1 + 32 {
        return Err(bad("the file is too short"));
    }
    let t = &raw[raw.len() - 32..];
    let (offset_size, ref_size) = (t[6] as usize, t[7] as usize);
    let (num, top, table) = (be(&t[8..16]), be(&t[16..24]), be(&t[24..32]));
    let max_nodes = lim.max_units.saturating_mul(40);
    if num > max_nodes as u64 {
        return Err(too_many(lim.max_units));
    }
    if !(1..=8).contains(&offset_size) || !(1..=8).contains(&ref_size) || num == 0 || top >= num {
        return Err(bad("the trailer is wrong"));
    }
    let table_end = (table as u128) + (num as u128) * (offset_size as u128);
    if table < 8 || table_end > (raw.len() - 32) as u128 {
        return Err(bad("the offset table is outside the file"));
    }
    let mut r = Reader {
        raw,
        fmt,
        offset_size,
        ref_size,
        num: num as usize,
        table: table as usize,
        max_nodes,
        max_units: lim.max_units,
        text_budget: raw.len().saturating_mul(8).saturating_add(1 << 20),
        tree: Tree { nodes: Vec::new(), root: 0 },
        shared: HashMap::new(),
        busy: std::collections::HashSet::new(),
        expanded: 0,
        deepest: 0,
    };
    let root = r.object(top as usize, 0)?;
    r.tree.root = root;
    Ok(r.tree)
}

enum Obj {
    Null,
    Bool(bool),
    Int(i128),
    Real(f64),
    Date(f64),
    Data(Vec<u8>),
    Str(String),
    Uid(u64),
    Array(Vec<usize>),
    Dict(Vec<usize>, Vec<usize>),
}

/// Objects already in the table, so an equal scalar is written once (plistlib `_BinaryPlistWriter._flatten`: a string,
/// integer, real, date, data, boolean or null that equals an earlier one reuses its object; a container never does,
/// and neither does a UID).
#[derive(Default)]
struct Shared {
    strs: HashMap<String, usize>,
    ints: HashMap<i128, usize>,
    reals: HashMap<u64, usize>,
    dates: HashMap<i128, usize>,
    /// Date bits for a date plistlib cannot hold (not finite, or beyond the years datetime has).
    odd_dates: HashMap<u64, usize>,
    /// Containers and UIDs by tree node: plistlib keeps those by identity, and a binary file that references one object
    /// twice reads as one Python object.
    nodes: HashMap<usize, usize>,
    data: HashMap<Vec<u8>, usize>,
    bools: [Option<usize>; 2],
    null: Option<usize>,
}

/// The bits an f64 is compared by: -0.0 equals 0.0 (as Python's `==`); a NaN never equals anything.
fn real_key(v: f64) -> Option<u64> {
    if v.is_nan() {
        None
    } else if v == 0.0 {
        Some(0)
    } else {
        Some(v.to_bits())
    }
}

/// The microseconds since 2001-01-01 that plistlib's `datetime(2001, 1, 1) + timedelta(seconds=x)` gives: the whole
/// seconds exactly, the fraction as float microseconds, truncated, and the leftover rounded to the nearest (half to
/// even, as CPython's `timedelta` does). `None` for a value datetime cannot hold.
fn date_micros(x: f64) -> Option<i128> {
    if !x.is_finite() || x.abs() > 3.0e11 {
        return None;
    }
    let whole = x.trunc();
    let frac = x - whole;
    let mut us = (whole as i128) * 1_000_000;
    if frac == 0.0 {
        return Some(us);
    }
    let scaled = 1_000_000f64 * frac;
    let ip = scaled.trunc();
    let leftover = scaled - ip;
    us += ip as i128;
    let mut r = leftover.round();
    if (r - leftover).abs() == 0.5 {
        r = 2.0 * (leftover / 2.0).round() - (us & 1) as f64;
    }
    Some(us + r as i128)
}

/// `timedelta.total_seconds()` of that many microseconds: the integer quotient, correctly rounded.
fn micros_to_seconds(us: i128) -> f64 {
    us as f64 / 1_000_000f64
}

/// Adds `o` to `objs` unless an equal shared scalar is there; returns its object number.
fn intern(objs: &mut Vec<Obj>, sh: &mut Shared, o: Obj) -> usize {
    fn slot<K: std::hash::Hash + Eq>(map: &mut HashMap<K, usize>, key: K, objs: &mut Vec<Obj>, o: Obj) -> usize {
        if let Some(&i) = map.get(&key) {
            return i;
        }
        objs.push(o);
        map.insert(key, objs.len() - 1);
        objs.len() - 1
    }
    match &o {
        Obj::Str(s) => {
            let k = s.clone();
            slot(&mut sh.strs, k, objs, o)
        }
        Obj::Int(v) => {
            let k = *v;
            slot(&mut sh.ints, k, objs, o)
        }
        Obj::Real(v) => match real_key(*v) {
            Some(k) => slot(&mut sh.reals, k, objs, o),
            None => {
                objs.push(o);
                objs.len() - 1
            }
        },
        Obj::Date(v) => match date_micros(*v) {
            // plistlib reads a date as a datetime (microseconds) and compares and writes it that way
            Some(us) => slot(&mut sh.dates, us, objs, Obj::Date(micros_to_seconds(us))),
            None => match real_key(*v) {
                Some(k) => slot(&mut sh.odd_dates, k, objs, o),
                None => {
                    objs.push(o);
                    objs.len() - 1
                }
            },
        },
        Obj::Data(d) => {
            let k = d.clone();
            slot(&mut sh.data, k, objs, o)
        }
        Obj::Bool(b) => {
            let at = usize::from(*b);
            if let Some(i) = sh.bools[at] {
                return i;
            }
            objs.push(o);
            sh.bools[at] = Some(objs.len() - 1);
            objs.len() - 1
        }
        Obj::Null => {
            if let Some(i) = sh.null {
                return i;
            }
            objs.push(o);
            sh.null = Some(objs.len() - 1);
            objs.len() - 1
        }
        _ => {
            objs.push(o);
            objs.len() - 1
        }
    }
}

fn flatten(tree: &Tree, i: usize, objs: &mut Vec<Obj>, sh: &mut Shared) -> usize {
    let n = &tree.nodes[i];
    match n.kind {
        Kind::Str => intern(objs, sh, Obj::Str(n.text.clone())),
        Kind::Other => {
            let o = match &n.scalar {
                Scalar::Bool(b) => Obj::Bool(*b),
                Scalar::Int(v) => Obj::Int(*v),
                Scalar::Real(v) => Obj::Real(*v),
                Scalar::Date(v) => Obj::Date(*v),
                Scalar::Data(d) => Obj::Data(d.clone()),
                Scalar::Uid(u) => Obj::Uid(*u),
                Scalar::Null | Scalar::None => Obj::Null,
            };
            if let Obj::Uid(_) = o {
                if let Some(&at) = sh.nodes.get(&i) {
                    return at;
                }
                let at = intern(objs, sh, o);
                sh.nodes.insert(i, at);
                return at;
            }
            intern(objs, sh, o)
        }
        Kind::Array => {
            if let Some(&o) = sh.nodes.get(&i) {
                return o;
            }
            let me = objs.len();
            sh.nodes.insert(i, me);
            objs.push(Obj::Null);
            let kids: Vec<usize> = n.kids.iter().map(|k| flatten(tree, k.node, objs, sh)).collect();
            objs[me] = Obj::Array(kids);
            me
        }
        Kind::Dict => {
            if let Some(&o) = sh.nodes.get(&i) {
                return o;
            }
            let me = objs.len();
            sh.nodes.insert(i, me);
            objs.push(Obj::Null);
            let keys: Vec<usize> = n.kids.iter().map(|k| intern(objs, sh, Obj::Str(k.key.clone().unwrap_or_default()))).collect();
            let vals: Vec<usize> = n.kids.iter().map(|k| flatten(tree, k.node, objs, sh)).collect();
            objs[me] = Obj::Dict(keys, vals);
            me
        }
    }
}

fn put_uint(out: &mut Vec<u8>, v: u64, size: usize) {
    out.extend_from_slice(&v.to_be_bytes()[8 - size..]);
}

fn size_for(v: u64) -> usize {
    if v < 1 << 8 {
        1
    } else if v < 1 << 16 {
        2
    } else if v < 1 << 32 {
        4
    } else {
        8
    }
}

fn put_int_obj(out: &mut Vec<u8>, v: i128) {
    if (0..1 << 8).contains(&v) {
        out.push(0x10);
        put_uint(out, v as u64, 1);
    } else if (0..1 << 16).contains(&v) {
        out.push(0x11);
        put_uint(out, v as u64, 2);
    } else if (0..1 << 32).contains(&v) {
        out.push(0x12);
        put_uint(out, v as u64, 4);
    } else if (i64::MIN as i128..=i64::MAX as i128).contains(&v) {
        out.push(0x13);
        out.extend_from_slice(&(v as i64).to_be_bytes());
    } else {
        out.push(0x14);
        out.extend_from_slice(&v.to_be_bytes());
    }
}

fn put_head(out: &mut Vec<u8>, hi: u8, n: usize) {
    if n < 15 {
        out.push((hi << 4) | n as u8);
    } else {
        out.push((hi << 4) | 0xF);
        put_int_obj(out, n as i128);
    }
}

/// Writes the whole tree as a `bplist00` file.
pub(crate) fn write(tree: &Tree) -> Vec<u8> {
    let mut objs = Vec::new();
    let mut shared = Shared::default();
    let top = flatten(tree, tree.root, &mut objs, &mut shared);
    let ref_size = size_for(objs.len() as u64);
    let mut out = b"bplist00".to_vec();
    let mut offsets = Vec::with_capacity(objs.len());
    for o in &objs {
        offsets.push(out.len() as u64);
        match o {
            Obj::Null => out.push(0x00),
            Obj::Bool(b) => out.push(if *b { 0x09 } else { 0x08 }),
            Obj::Int(v) => put_int_obj(&mut out, *v),
            Obj::Real(v) => {
                out.push(0x23);
                out.extend_from_slice(&v.to_be_bytes());
            }
            Obj::Date(v) => {
                out.push(0x33);
                out.extend_from_slice(&v.to_be_bytes());
            }
            Obj::Data(d) => {
                put_head(&mut out, 0x4, d.len());
                out.extend_from_slice(d);
            }
            Obj::Str(s) => {
                if s.is_ascii() {
                    put_head(&mut out, 0x5, s.len());
                    out.extend_from_slice(s.as_bytes());
                } else {
                    let units: Vec<u16> = s.encode_utf16().collect();
                    put_head(&mut out, 0x6, units.len());
                    for u in units {
                        out.extend_from_slice(&u.to_be_bytes());
                    }
                }
            }
            Obj::Uid(u) => {
                let size = size_for(*u);
                out.push(0x80 | (size as u8 - 1));
                put_uint(&mut out, *u, size);
            }
            Obj::Array(kids) => {
                put_head(&mut out, 0xA, kids.len());
                for k in kids {
                    put_uint(&mut out, *k as u64, ref_size);
                }
            }
            Obj::Dict(keys, vals) => {
                put_head(&mut out, 0xD, keys.len());
                for k in keys.iter().chain(vals) {
                    put_uint(&mut out, *k as u64, ref_size);
                }
            }
        }
    }
    let table = out.len() as u64;
    let offset_size = size_for(table);
    for o in &offsets {
        put_uint(&mut out, *o, offset_size);
    }
    out.extend_from_slice(&[0; 6]);
    out.push(offset_size as u8);
    out.push(ref_size as u8);
    out.extend_from_slice(&(objs.len() as u64).to_be_bytes());
    out.extend_from_slice(&(top as u64).to_be_bytes());
    out.extend_from_slice(&table.to_be_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(t: &mut Tree, kind: Kind, text: &str, scalar: Scalar) -> usize {
        let mut n = Node::new(kind);
        n.text = text.to_string();
        n.scalar = scalar;
        t.nodes.push(n);
        t.nodes.len() - 1
    }

    fn kid(key: Option<&str>, node: usize) -> Kid {
        Kid { key: key.map(str::to_string), kstart: NO_SPAN, kend: NO_SPAN, node }
    }

    /// plistlib's output for the same value (`plistlib.dumps(v, fmt=FMT_BINARY, sort_keys=False)`): equal strings,
    /// integers, reals, data and booleans are written once, keys and values share objects, references are one byte.
    #[test]
    fn equal_scalars_are_written_once_as_plistlib_does() {
        let mut t = Tree::default();
        let s = |t: &mut Tree, x: &str| leaf(t, Kind::Str, x, Scalar::None);
        let list_items = {
            let a = s(&mut t, "Save");
            let b = s(&mut t, "x");
            let c = leaf(&mut t, Kind::Other, "", Scalar::Bool(true));
            let d = leaf(&mut t, Kind::Other, "", Scalar::Bool(false));
            let e = leaf(&mut t, Kind::Other, "", Scalar::Bool(true));
            let f = leaf(&mut t, Kind::Other, "", Scalar::Int(1));
            let g = leaf(&mut t, Kind::Other, "", Scalar::Int(1));
            let h = leaf(&mut t, Kind::Other, "", Scalar::Real(2.5));
            let i = leaf(&mut t, Kind::Other, "", Scalar::Real(2.5));
            let j = leaf(&mut t, Kind::Other, "", Scalar::Data(vec![1]));
            let k = leaf(&mut t, Kind::Other, "", Scalar::Data(vec![1]));
            [a, b, c, d, e, f, g, h, i, j, k]
        };
        let mut list = Node::new(Kind::Array);
        list.kids = list_items.iter().map(|n| kid(None, *n)).collect();
        t.nodes.push(list);
        let list = t.nodes.len() - 1;
        let v_title = s(&mut t, "Save");
        let v_name = s(&mut t, "Save");
        let v_save = s(&mut t, "x");
        let n1 = leaf(&mut t, Kind::Other, "", Scalar::Int(1));
        let n2 = leaf(&mut t, Kind::Other, "", Scalar::Int(1));
        let t1 = leaf(&mut t, Kind::Other, "", Scalar::Bool(true));
        let t2 = leaf(&mut t, Kind::Other, "", Scalar::Bool(true));
        let mut root = Node::new(Kind::Dict);
        root.kids = vec![
            kid(Some("Title"), v_title),
            kid(Some("Name"), v_name),
            kid(Some("Save"), v_save),
            kid(Some("list"), list),
            kid(Some("n"), n1),
            kid(Some("m"), n2),
            kid(Some("t"), t1),
            kid(Some("u"), t2),
        ];
        t.nodes.push(root);
        t.root = t.nodes.len() - 1;
        let want = "62706c6973743030d801020304050607080303090a0d0d0b0b555469746c65544e616d655453617665546c697374516e516d517451755178ab03090b0c0b0d0d0e0e0f0f09081001234004000000000000410108191f24292e303234363844454648510000000000000101000000000000001000000000000000000000000000000053";
        let got: String = write(&t).iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(got, want);
    }

    fn unhex(h: &str) -> Vec<u8> {
        (0..h.len()).step_by(2).map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap()).collect()
    }

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    fn rewrite(h: &str) -> String {
        let tree = read(&unhex(h), "plist", &Limits::default()).unwrap();
        hex(&write(&tree))
    }

    /// An object the file references twice is one value that appears twice (plistlib hands back the same Python
    /// object for a number it read before, and writes a container or UID once): `{"a": D, "b": D, "c": U, "d": U}`
    /// with `D = {"k": "v"}` and `U` a UID. The expected bytes are `plistlib.dumps(plistlib.loads(raw), fmt=FMT_BINARY,
    /// sort_keys=False)`.
    #[test]
    fn a_container_or_uid_the_file_references_twice_is_written_once() {
        let input = "62706c6973743030d401020304050506065161516251635164d107088003516b51760811131517191c1e200000000000000101000000000000000900000000000000000000000000000022";
        let want = "62706c6973743030d401020304050508085161516251635164d10607516b517680030811131517191c1e200000000000000101000000000000000900000000000000000000000000000022";
        assert_eq!(rewrite(input), want);
    }

    /// plistlib reads a date as a datetime, so dates a microsecond apart are distinct and dates closer than half a
    /// microsecond are one value: `[360000000.5, 360000000.5 + 3e-7, 360000000.500001, 360000000.5]`.
    #[test]
    fn dates_are_shared_by_microsecond_as_plistlib_does() {
        let input = "62706c6973743030a4010203043341b5752a008000003341b5752a008000053341b5752a008000113341b5752a00800000080d161f280000000000000101000000000000000500000000000000000000000000000031";
        let want = "62706c6973743030a4010102013341b5752a008000003341b5752a00800011080d16000000000000010100000000000000030000000000000000000000000000001f";
        assert_eq!(rewrite(input), want);
    }
}

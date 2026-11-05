//! Binary property lists (`bplist00`): a reader into the same [`Tree`] as XML and a writer for the whole tree.
//! As in Localization OS a binary file is rebuilt from the tree when a translation changes it, so it is
//! semantically the same file (every non-string value kept), not the same bytes; an untouched file is returned as
//! it came.

use crate::tree::*;
use crate::xml::{perr, too_deep, too_many, MAX_DEPTH};
use bilingua_model::*;

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
        if self.tree.nodes.len() >= self.max_nodes {
            return Err(too_many(self.max_units));
        }
        let off = self.offset_of(i)?;
        let (hi, n, data) = self.header(off)?;
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
                return Ok(idx);
            }
            0xD => {
                node.kind = Kind::Dict;
                let total = n.checked_mul(self.ref_size * 2).ok_or_else(|| self.bad("a length is too large"))?;
                let refs = self.slice(data, total)?;
                let idx = self.tree.nodes.len();
                self.tree.nodes.push(node);
                for k in 0..n {
                    let kr = self.r#ref(data + k * self.ref_size)?;
                    let key = self.string_at(kr)?;
                    let vr = be(&refs[(n + k) * self.ref_size..(n + k + 1) * self.ref_size]) as usize;
                    if vr >= self.num {
                        return Err(self.bad("a reference points past the object table"));
                    }
                    let c = self.object(vr, depth + 1)?;
                    // a repeated key: the last value wins, in the first key's place (a Python dict)
                    match self.tree.nodes[idx].kids.iter().position(|kid| kid.key.as_deref() == Some(key.as_str())) {
                        Some(p) => self.tree.nodes[idx].kids[p].node = c,
                        None => self.tree.nodes[idx].kids.push(Kid { key: Some(key), kstart: NO_SPAN, kend: NO_SPAN, node: c }),
                    }
                }
                return Ok(idx);
            }
            _ => return Err(self.bad("an unsupported object type")),
        }
        let idx = self.tree.nodes.len();
        self.tree.nodes.push(node);
        Ok(idx)
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

fn flatten(tree: &Tree, i: usize, objs: &mut Vec<Obj>) -> usize {
    let n = &tree.nodes[i];
    let me = objs.len();
    match n.kind {
        Kind::Str => objs.push(Obj::Str(n.text.clone())),
        Kind::Other => objs.push(match &n.scalar {
            Scalar::Bool(b) => Obj::Bool(*b),
            Scalar::Int(v) => Obj::Int(*v),
            Scalar::Real(v) => Obj::Real(*v),
            Scalar::Date(v) => Obj::Date(*v),
            Scalar::Data(d) => Obj::Data(d.clone()),
            Scalar::Uid(u) => Obj::Uid(*u),
            Scalar::Null | Scalar::None => Obj::Null,
        }),
        Kind::Array => {
            objs.push(Obj::Null);
            let kids: Vec<usize> = n.kids.iter().map(|k| flatten(tree, k.node, objs)).collect();
            objs[me] = Obj::Array(kids);
        }
        Kind::Dict => {
            objs.push(Obj::Null);
            let keys: Vec<usize> = n
                .kids
                .iter()
                .map(|k| {
                    objs.push(Obj::Str(k.key.clone().unwrap_or_default()));
                    objs.len() - 1
                })
                .collect();
            let vals: Vec<usize> = n.kids.iter().map(|k| flatten(tree, k.node, objs)).collect();
            objs[me] = Obj::Dict(keys, vals);
        }
    }
    me
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
    flatten(tree, tree.root, &mut objs);
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
    out.extend_from_slice(&0u64.to_be_bytes());
    out.extend_from_slice(&table.to_be_bytes());
    out
}

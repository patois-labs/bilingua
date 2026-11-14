//! Embedded font programs, read only for what the text layer needs: glyph boxes (PDFium measures text objects
//! with them), glyph ids for char codes, CFF glyph names and built-in encodings. TrueType (`glyf`, `loca`, `cmap`)
//! and CFF (Type 2 charstrings run for their control box, name-keyed and CID-keyed) are read; Type 1 programs are
//! not (their boxes stay approximate).
//!
//! Every read is bounds-checked; a damaged table gives no box, never a panic. Charstring execution is capped in
//! operations and subroutine depth.

use crate::tables;
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

/// Work left for the whole document (shared with `Doc`): charstring operations are charged to it, so a font whose
/// glyphs each loop through subroutines up to the per-glyph cap cannot multiply that cap by every glyph of every
/// font. At zero no more glyphs run (no box) and the extraction ends with a limit error.
pub type Work = Rc<Cell<u64>>;

pub struct Program {
    kind: Kind,
    /// Units per em (1000 for CFF unless the font matrix says otherwise).
    upem: u32,
    work: Work,
}

enum Kind {
    TrueType(TrueType),
    Cff(Cff),
}

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*b.get(o)?, *b.get(o + 1)?]))
}
fn i16_at(b: &[u8], o: usize) -> Option<i16> {
    u16_at(b, o).map(|v| v as i16)
}
fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_be_bytes([*b.get(o)?, *b.get(o + 1)?, *b.get(o + 2)?, *b.get(o + 3)?]))
}

/// PDFium's `NormalizeFontMetric`: to 1000 units, rounded half up.
fn norm(v: i64, upem: u32) -> i32 {
    if upem == 0 {
        return v.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
    }
    let s = (v as f64 * 1000.0 + (upem / 2) as f64) / upem as f64;
    s.clamp(i32::MIN as f64, i32::MAX as f64) as i32
}

impl Program {
    pub fn parse(data: &[u8], work: &Work) -> Option<Program> {
        let work = work.clone();
        if data.len() >= 12 && (data.starts_with(&[0, 1, 0, 0]) || data.starts_with(b"true") || data.starts_with(b"OTTO")) {
            let tt = TrueType::parse(data)?;
            if let Some(cff) = &tt.cff {
                let c = Cff::parse(cff)?;
                let upem = c.upem;
                return Some(Program { kind: Kind::Cff(c), upem, work });
            }
            let upem = tt.upem;
            return Some(Program { kind: Kind::TrueType(tt), upem, work });
        }
        if data.len() >= 4 && data[0] == 1 && data[1] == 0 {
            let c = Cff::parse(data)?;
            let upem = c.upem;
            return Some(Program { kind: Kind::Cff(c), upem, work });
        }
        None
    }

    /// The glyph box in 1000 units as [left, bottom, right, top]. With `inflate` the top is raised by a 64th, as
    /// PDFium's `CFX_Face::GetCharBBox` (used for CID fonts) does; simple fonts use the plain box. `None` when the
    /// glyph does not load.
    pub fn glyph_bbox(&self, gid: u32, inflate: bool) -> Option<[i32; 4]> {
        let (x0, y0, x1, y1) = match &self.kind {
            Kind::TrueType(t) => t.bbox(gid)?,
            Kind::Cff(c) => c.run(gid, &self.work)?.0,
        };
        let left = norm(x0 as i64, self.upem);
        let top = norm(y1 as i64, self.upem);
        let right = norm(x1 as i64, self.upem);
        let bottom = norm(y0 as i64, self.upem);
        Some([left, bottom, right, if inflate { top + top / 64 } else { top }])
    }

    /// The advance width in 1000 units (TrueType `hmtx`, CFF charstring width).
    pub fn advance(&self, gid: u32) -> Option<i32> {
        match &self.kind {
            Kind::TrueType(t) => t.advance(gid).map(|a| norm(a as i64, self.upem)),
            Kind::Cff(c) => c.run(gid, &self.work).map(|r| norm(r.1 as i64, self.upem)),
        }
    }


    /// The glyph of a CID: a CID-keyed CFF maps through its charset, everything else is identity.
    pub fn gid_for_cid(&self, cid: u32) -> u32 {
        match &self.kind {
            Kind::Cff(c) if c.cid => c.by_sid.get(&cid).copied().unwrap_or(0),
            _ => cid,
        }
    }

    /// A glyph by name (CFF charsets, TrueType `post` is not read).
    pub fn gid_for_name(&self, name: &str) -> Option<u32> {
        match &self.kind {
            Kind::Cff(c) if !c.cid => c.by_name.get(name).copied(),
            _ => None,
        }
    }

    /// The glyph names of a name-keyed CFF's built-in encoding, by code.
    pub fn builtin_names(&self) -> Option<Vec<Option<String>>> {
        match &self.kind {
            Kind::Cff(c) if !c.cid => Some(c.encoding_names()),
            _ => None,
        }
    }

    /// A TrueType `cmap` subtable lookup: (platform, encoding) and a code.
    pub fn cmap(&self, platform: u16, encoding: u16, code: u32) -> Option<u32> {
        match &self.kind {
            Kind::TrueType(t) => t.cmap_lookup(platform, encoding, code),
            _ => None,
        }
    }

    pub fn has_cmap(&self, platform: u16, encoding: u16) -> bool {
        match &self.kind {
            Kind::TrueType(t) => t.cmaps.iter().any(|(p, e, _)| *p == platform && *e == encoding),
            _ => false,
        }
    }
}

// ---------------------------------------------------------------- TrueType

struct TrueType {
    data: Vec<u8>,
    upem: u32,
    loca: Vec<u32>,
    glyf: (usize, usize),
    hmtx: (usize, usize),
    num_h: usize,
    cmaps: Vec<(u16, u16, usize)>,
    cff: Option<Vec<u8>>,
}

impl TrueType {
    fn parse(d: &[u8]) -> Option<TrueType> {
        let n = u16_at(d, 4)? as usize;
        let mut tables: HashMap<[u8; 4], (usize, usize)> = HashMap::new();
        for i in 0..n.min(512) {
            let r = 12 + 16 * i;
            let tag = [*d.get(r)?, *d.get(r + 1)?, *d.get(r + 2)?, *d.get(r + 3)?];
            let off = u32_at(d, r + 8)? as usize;
            let len = u32_at(d, r + 12)? as usize;
            if off <= d.len() {
                tables.insert(tag, (off, len.min(d.len() - off)));
            }
        }
        if let Some(&(o, l)) = tables.get(b"CFF ") {
            return Some(TrueType { data: vec![], upem: 1000, loca: vec![], glyf: (0, 0), hmtx: (0, 0), num_h: 0, cmaps: vec![], cff: Some(d[o..o + l].to_vec()) });
        }
        let &(ho, _) = tables.get(b"head")?;
        let upem = u16_at(d, ho + 18)? as u32;
        let long = i16_at(d, ho + 50)? != 0;
        let num_glyphs = tables.get(b"maxp").and_then(|&(o, _)| u16_at(d, o + 4)).unwrap_or(0) as usize;
        let mut loca = Vec::new();
        if let Some(&(lo, ll)) = tables.get(b"loca") {
            let count = (num_glyphs + 1).min(if long { ll / 4 } else { ll / 2 });
            for i in 0..count {
                let v = if long { u32_at(d, lo + 4 * i)? } else { u16_at(d, lo + 2 * i)? as u32 * 2 };
                loca.push(v);
            }
        }
        let glyf = tables.get(b"glyf").copied().unwrap_or((0, 0));
        let hmtx = tables.get(b"hmtx").copied().unwrap_or((0, 0));
        let num_h = tables.get(b"hhea").and_then(|&(o, _)| u16_at(d, o + 34)).unwrap_or(0) as usize;
        let mut cmaps = Vec::new();
        if let Some(&(co, _)) = tables.get(b"cmap") {
            let nt = u16_at(d, co + 2).unwrap_or(0) as usize;
            for i in 0..nt.min(64) {
                let r = co + 4 + 8 * i;
                if let (Some(p), Some(e), Some(off)) = (u16_at(d, r), u16_at(d, r + 2), u32_at(d, r + 4)) {
                    cmaps.push((p, e, co + off as usize));
                }
            }
        }
        Some(TrueType { data: d.to_vec(), upem, loca, glyf, hmtx, num_h, cmaps, cff: None })
    }

    fn bbox(&self, gid: u32) -> Option<(i32, i32, i32, i32)> {
        let g = gid as usize;
        let (a, b) = (*self.loca.get(g)? as usize, *self.loca.get(g + 1)? as usize);
        if b <= a {
            return Some((0, 0, 0, 0));
        }
        if b - a < 10 || a + 10 > self.glyf.1 {
            return None;
        }
        let o = self.glyf.0 + a;
        let d = &self.data;
        Some((i16_at(d, o + 2)? as i32, i16_at(d, o + 4)? as i32, i16_at(d, o + 6)? as i32, i16_at(d, o + 8)? as i32))
    }

    fn advance(&self, gid: u32) -> Option<u16> {
        if self.num_h == 0 {
            return None;
        }
        let i = (gid as usize).min(self.num_h - 1);
        if 4 * i + 2 > self.hmtx.1 {
            return None;
        }
        u16_at(&self.data, self.hmtx.0 + 4 * i)
    }

    fn cmap_lookup(&self, platform: u16, encoding: u16, code: u32) -> Option<u32> {
        let &(_, _, off) = self.cmaps.iter().find(|(p, e, _)| *p == platform && *e == encoding)?;
        let d = &self.data;
        let fmt = u16_at(d, off)?;
        let g = match fmt {
            0 => {
                if code > 255 {
                    return None;
                }
                *d.get(off + 6 + code as usize)? as u32
            }
            4 => {
                if code > 0xFFFF {
                    return None;
                }
                let segx2 = u16_at(d, off + 6)? as usize;
                let ends = off + 14;
                let starts = ends + segx2 + 2;
                let deltas = starts + segx2;
                let ranges = deltas + segx2;
                // The first segment whose end is not below the code (ends ascend), found by bisection: a table
                // of 32 767 segments is looked up for every code of every font.
                let segs = segx2 / 2;
                let (mut lo, mut hi) = (0usize, segs);
                while lo < hi {
                    let mid = (lo + hi) / 2;
                    if (u16_at(d, ends + 2 * mid)? as u32) < code {
                        lo = mid + 1;
                    } else {
                        hi = mid;
                    }
                }
                let mut found = None;
                if lo < segs {
                    let s = lo;
                    let start = u16_at(d, starts + 2 * s)? as u32;
                    if code < start {
                        return None;
                    }
                    let delta = u16_at(d, deltas + 2 * s)?;
                    let ro = u16_at(d, ranges + 2 * s)? as usize;
                    let gid = if ro == 0 {
                        (code as u16).wrapping_add(delta)
                    } else {
                        let at = ranges + 2 * s + ro + 2 * (code - start) as usize;
                        let v = u16_at(d, at)?;
                        if v == 0 {
                            0
                        } else {
                            v.wrapping_add(delta)
                        }
                    };
                    found = Some(gid as u32);
                }
                found?
            }
            6 => {
                let first = u16_at(d, off + 6)? as u32;
                let count = u16_at(d, off + 8)? as u32;
                if code < first || code >= first + count {
                    return None;
                }
                u16_at(d, off + 10 + 2 * (code - first) as usize)? as u32
            }
            12 => {
                // Groups ascend by start code: the last group starting at or below the code, by bisection.
                let n = (u32_at(d, off + 12)? as usize).min(1 << 20);
                let (mut lo, mut hi) = (0usize, n);
                while lo < hi {
                    let mid = (lo + hi) / 2;
                    if u32_at(d, off + 16 + 12 * mid)? <= code {
                        lo = mid + 1;
                    } else {
                        hi = mid;
                    }
                }
                if lo == 0 {
                    return None;
                }
                let r = off + 16 + 12 * (lo - 1);
                let (a, b, g) = (u32_at(d, r)?, u32_at(d, r + 4)?, u32_at(d, r + 8)?);
                if code < a || code > b {
                    return None;
                }
                g.wrapping_add(code - a)
            }
            _ => return None,
        };
        if g == 0 {
            None
        } else {
            Some(g)
        }
    }
}

// ---------------------------------------------------------------- CFF

/// A Private DICT: local subroutines, default width, nominal width.
type Private = (Vec<(usize, usize)>, f64, f64);

struct Cff {
    data: Vec<u8>,
    upem: u32,
    charstrings: Vec<(usize, usize)>,
    gsubrs: Vec<(usize, usize)>,
    /// Local subrs and (default width, nominal width) per font dict; one entry for a name-keyed font.
    privs: Vec<Private>,
    fd_select: Vec<u8>,
    cid: bool,
    /// Name-keyed: glyph name to gid; CID-keyed: CID to gid.
    by_name: HashMap<String, u32>,
    by_sid: HashMap<u32, u32>,
    names: Vec<String>,
    encoding: Vec<Option<u32>>,
}

fn index(d: &[u8], at: usize) -> Option<(Vec<(usize, usize)>, usize)> {
    let count = u16_at(d, at)? as usize;
    if count == 0 {
        return Some((vec![], at + 2));
    }
    let osz = *d.get(at + 2)? as usize;
    if !(1..=4).contains(&osz) {
        return None;
    }
    let read = |i: usize| -> Option<usize> {
        let mut v = 0usize;
        for k in 0..osz {
            v = (v << 8) | *d.get(at + 3 + i * osz + k)? as usize;
        }
        Some(v)
    };
    let base = at + 3 + (count + 1) * osz - 1;
    let mut out = Vec::with_capacity(count.min(65536));
    for i in 0..count {
        let a = read(i)?;
        let b = read(i + 1)?;
        if a == 0 || b < a || base + b > d.len() {
            return None;
        }
        out.push((base + a, b - a));
    }
    let end = base + read(count)?;
    Some((out, end))
}

/// A CFF DICT: operator (12 x as 1200 + x) to operands.
fn dict(d: &[u8]) -> HashMap<u16, Vec<f64>> {
    let mut out = HashMap::new();
    let mut ops: Vec<f64> = Vec::new();
    let mut i = 0;
    while i < d.len() {
        let b0 = d[i];
        match b0 {
            0..=21 => {
                let op = if b0 == 12 {
                    i += 1;
                    1200 + *d.get(i).unwrap_or(&0) as u16
                } else {
                    b0 as u16
                };
                out.insert(op, std::mem::take(&mut ops));
                i += 1;
            }
            28 => {
                ops.push(i16_at(d, i + 1).unwrap_or(0) as f64);
                i += 3;
            }
            29 => {
                ops.push(u32_at(d, i + 1).unwrap_or(0) as i32 as f64);
                i += 5;
            }
            30 => {
                // real number: nibbles
                let mut s = String::new();
                i += 1;
                'outer: while i < d.len() {
                    for nib in [d[i] >> 4, d[i] & 15] {
                        match nib {
                            0..=9 => s.push((b'0' + nib) as char),
                            0xa => s.push('.'),
                            0xb => s.push('E'),
                            0xc => s.push_str("E-"),
                            0xe => s.push('-'),
                            0xf => {
                                i += 1;
                                break 'outer;
                            }
                            _ => {}
                        }
                    }
                    i += 1;
                }
                ops.push(s.parse::<f64>().unwrap_or(0.0));
            }
            32..=246 => {
                ops.push(b0 as f64 - 139.0);
                i += 1;
            }
            247..=250 => {
                ops.push((b0 as f64 - 247.0) * 256.0 + *d.get(i + 1).unwrap_or(&0) as f64 + 108.0);
                i += 2;
            }
            251..=254 => {
                ops.push(-(b0 as f64 - 251.0) * 256.0 - *d.get(i + 1).unwrap_or(&0) as f64 - 108.0);
                i += 2;
            }
            _ => i += 1,
        }
        if ops.len() > 48 {
            ops.clear();
        }
    }
    out
}

fn first(d: &HashMap<u16, Vec<f64>>, op: u16) -> Option<f64> {
    d.get(&op).and_then(|v| v.first().copied())
}

impl Cff {
    fn parse(d: &[u8]) -> Option<Cff> {
        let hdr = *d.get(2)? as usize;
        let (_names, after_names) = index(d, hdr)?;
        let (tops, after_tops) = index(d, after_names)?;
        let (strings, after_strings) = index(d, after_tops)?;
        let (gsubrs, _) = index(d, after_strings).unwrap_or((vec![], 0));
        let &(to, tl) = tops.first()?;
        let top = dict(&d[to..to + tl]);
        let cs_off = first(&top, 17)? as usize;
        let (charstrings, _) = index(d, cs_off)?;
        let n = charstrings.len();
        let cid = top.contains_key(&1230);
        let upem = match top.get(&1207) {
            Some(m) if !m.is_empty() && m[0] > 0.0 => (1.0 / m[0]).round() as u32,
            _ => 1000,
        };
        let sid_name = |sid: u32| -> String {
            if (sid as usize) < tables::CFF_STD_STRINGS.len() {
                tables::CFF_STD_STRINGS[sid as usize].to_string()
            } else {
                strings.get(sid as usize - tables::CFF_STD_STRINGS.len()).map(|&(o, l)| String::from_utf8_lossy(&d[o..o + l]).into_owned()).unwrap_or_default()
            }
        };
        // charset: gid -> sid (or cid).
        let mut sids = vec![0u32; n];
        let cs = first(&top, 15).unwrap_or(0.0) as usize;
        if cs > 2 {
            let f = *d.get(cs)?;
            let mut gid = 1;
            let mut p = cs + 1;
            match f {
                0 => {
                    while gid < n {
                        sids[gid] = u16_at(d, p)? as u32;
                        p += 2;
                        gid += 1;
                    }
                }
                1 | 2 => {
                    while gid < n {
                        let s = u16_at(d, p)? as u32;
                        let left = if f == 1 { *d.get(p + 2)? as u32 } else { u16_at(d, p + 2)? as u32 };
                        p += if f == 1 { 3 } else { 4 };
                        for k in 0..=left {
                            if gid >= n {
                                break;
                            }
                            sids[gid] = s + k;
                            gid += 1;
                        }
                    }
                }
                _ => {}
            }
        } else if cs == 0 {
            for (g, s) in sids.iter_mut().enumerate() {
                *s = if g < 229 { g as u32 } else { 0 };
            }
        }
        let mut by_name = HashMap::new();
        let mut by_sid = HashMap::new();
        let mut names = vec![String::new(); n];
        for (g, &s) in sids.iter().enumerate() {
            if cid {
                by_sid.entry(s).or_insert(g as u32);
            } else {
                let name = sid_name(s);
                by_name.entry(name.clone()).or_insert(g as u32);
                names[g] = name;
            }
        }
        // Private dicts.
        let read_priv = |pd: &HashMap<u16, Vec<f64>>| -> (Vec<(usize, usize)>, f64, f64) {
            let Some(p) = pd.get(&18) else { return (vec![], 0.0, 0.0) };
            if p.len() < 2 {
                return (vec![], 0.0, 0.0);
            }
            let (size, off) = (p[0] as usize, p[1] as usize);
            if off.checked_add(size).is_none_or(|e| e > d.len()) {
                return (vec![], 0.0, 0.0);
            }
            let pv = dict(&d[off..off + size]);
            let subrs = first(&pv, 19).and_then(|s| index(d, off + s as usize)).map(|x| x.0).unwrap_or_default();
            (subrs, first(&pv, 20).unwrap_or(0.0), first(&pv, 21).unwrap_or(0.0))
        };
        let mut privs = Vec::new();
        let mut fd_select = Vec::new();
        if cid {
            if let Some(fa) = first(&top, 1236) {
                if let Some((fds, _)) = index(d, fa as usize) {
                    for &(o, l) in fds.iter().take(256) {
                        privs.push(read_priv(&dict(&d[o..o + l])));
                    }
                }
            }
            if let Some(fs) = first(&top, 1237) {
                let fs = fs as usize;
                fd_select = vec![0u8; n];
                match d.get(fs) {
                    Some(0) => {
                        for (g, slot) in fd_select.iter_mut().enumerate() {
                            *slot = *d.get(fs + 1 + g).unwrap_or(&0);
                        }
                    }
                    Some(3) => {
                        // Ranges come in ascending order; a range that starts before the previous one ended only
                        // fills what is new, so 65 535 overlapping ranges cannot cost 65 535 times the glyph count.
                        let nr = u16_at(d, fs + 1).unwrap_or(0) as usize;
                        let mut done = 0usize;
                        for r in 0..nr {
                            let a = (u16_at(d, fs + 3 + 3 * r).unwrap_or(0) as usize).max(done);
                            let fd = *d.get(fs + 5 + 3 * r).unwrap_or(&0);
                            let b = (u16_at(d, fs + 6 + 3 * r).unwrap_or(0) as usize).min(n);
                            if b > a {
                                for slot in &mut fd_select[a..b] {
                                    *slot = fd;
                                }
                                done = b;
                            }
                        }
                    }
                    _ => {}
                }
            }
        } else {
            privs.push(read_priv(&top));
        }
        // Encoding (name-keyed): code -> gid.
        let mut encoding = vec![None; 256];
        if !cid {
            let eo = first(&top, 16).unwrap_or(0.0) as usize;
            if eo > 1 {
                let f = d.get(eo).copied().unwrap_or(0);
                let mut p = eo + 1;
                match f & 0x7f {
                    0 => {
                        let nc = *d.get(p).unwrap_or(&0) as usize;
                        p += 1;
                        for g in 1..=nc {
                            if let Some(&c) = d.get(p) {
                                encoding[c as usize] = Some(g as u32);
                            }
                            p += 1;
                        }
                    }
                    1 => {
                        let nr = *d.get(p).unwrap_or(&0) as usize;
                        p += 1;
                        let mut g = 1u32;
                        for _ in 0..nr {
                            let c = *d.get(p).unwrap_or(&0) as usize;
                            let left = *d.get(p + 1).unwrap_or(&0) as usize;
                            p += 2;
                            for k in 0..=left {
                                if c + k < 256 {
                                    encoding[c + k] = Some(g);
                                }
                                g += 1;
                            }
                        }
                    }
                    _ => {}
                }
                if f & 0x80 != 0 {
                    let ns = *d.get(p).unwrap_or(&0) as usize;
                    p += 1;
                    for _ in 0..ns {
                        let c = *d.get(p).unwrap_or(&0) as usize;
                        let sid = u16_at(d, p + 1).unwrap_or(0) as u32;
                        p += 3;
                        if let Some(&g) = by_name.get(&sid_name(sid)) {
                            encoding[c] = Some(g);
                        }
                    }
                }
            } else if eo == 0 {
                for (c, slot) in encoding.iter_mut().enumerate() {
                    if c >= tables::STANDARD_NAMES_FIRST {
                        if let Some(Some(nm)) = tables::STANDARD_NAMES.get(c - tables::STANDARD_NAMES_FIRST) {
                            *slot = by_name.get(*nm).copied();
                        }
                    }
                }
            }
        }
        Some(Cff { data: d.to_vec(), upem, charstrings, gsubrs, privs, fd_select, cid, by_name, by_sid, names, encoding })
    }

    fn encoding_names(&self) -> Vec<Option<String>> {
        self.encoding.iter().map(|g| g.and_then(|g| self.names.get(g as usize).cloned())).collect()
    }

    fn priv_of(&self, gid: u32) -> Option<&Private> {
        let fd = if self.cid { *self.fd_select.get(gid as usize).unwrap_or(&0) as usize } else { 0 };
        self.privs.get(fd).or(self.privs.first())
    }

    /// Runs a charstring: (control box, advance width). `None` when the document's work is spent.
    fn run(&self, gid: u32, work: &Work) -> Option<((i32, i32, i32, i32), f64)> {
        let &(o, l) = self.charstrings.get(gid as usize)?;
        if work.get() == 0 {
            return None;
        }
        let (subrs, default_w, nominal_w) = self.priv_of(gid).cloned().unwrap_or((vec![], 0.0, 0.0));
        let mut st = Cs { x: 0.0, y: 0.0, bbox: None, stack: Vec::new(), stems: 0, width: None, ops: 0, done: false };
        st.exec(&self.data, (o, l), &subrs, &self.gsubrs, 0);
        work.set(work.get().saturating_sub(st.ops as u64));
        let w = match st.width {
            Some(w) if !w.is_nan() => w + nominal_w,
            _ => default_w,
        };
        let b = st.bbox.map(|(a, b, c, d)| (a.floor() as i32, b.floor() as i32, c.ceil() as i32, d.ceil() as i32)).unwrap_or((0, 0, 0, 0));
        Some((b, w))
    }
}

struct Cs {
    x: f64,
    y: f64,
    bbox: Option<(f64, f64, f64, f64)>,
    stack: Vec<f64>,
    stems: usize,
    width: Option<f64>,
    ops: usize,
    done: bool,
}

fn bias(n: usize) -> i64 {
    if n < 1240 {
        107
    } else if n < 33900 {
        1131
    } else {
        32768
    }
}

impl Cs {
    fn pt(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
        let (x, y) = (self.x, self.y);
        self.bbox = Some(match self.bbox {
            None => (x, y, x, y),
            Some((a, b, c, d)) => (a.min(x), b.min(y), c.max(x), d.max(y)),
        });
    }
    fn mv(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
    }
    /// Stem operators: an odd operand count means the first one is the width.
    fn take_width(&mut self, _even: bool) {
        if self.width.is_none() {
            if self.stack.len() % 2 == 1 {
                self.width = Some(self.stack.remove(0));
            } else {
                self.width = Some(f64::NAN);
            }
        }
    }

    fn exec(&mut self, d: &[u8], (o, l): (usize, usize), subrs: &[(usize, usize)], gsubrs: &[(usize, usize)], depth: usize) {
        if depth > 10 || o + l > d.len() {
            return;
        }
        let cs = &d[o..o + l];
        let mut i = 0;
        while i < cs.len() && !self.done {
            self.ops += 1;
            if self.ops > 100_000 || self.stack.len() > 96 {
                self.done = true;
                return;
            }
            let b0 = cs[i];
            i += 1;
            match b0 {
                28 => {
                    if i + 1 < cs.len() {
                        self.stack.push(i16::from_be_bytes([cs[i], cs[i + 1]]) as f64);
                    }
                    i += 2;
                }
                32..=246 => self.stack.push(b0 as f64 - 139.0),
                247..=250 => {
                    self.stack.push((b0 as f64 - 247.0) * 256.0 + *cs.get(i).unwrap_or(&0) as f64 + 108.0);
                    i += 1;
                }
                251..=254 => {
                    self.stack.push(-(b0 as f64 - 251.0) * 256.0 - *cs.get(i).unwrap_or(&0) as f64 - 108.0);
                    i += 1;
                }
                255 => {
                    if i + 3 < cs.len() {
                        self.stack.push(i32::from_be_bytes([cs[i], cs[i + 1], cs[i + 2], cs[i + 3]]) as f64 / 65536.0);
                    }
                    i += 4;
                }
                1 | 3 | 18 | 23 => {
                    self.take_width(true);
                    self.stems += self.stack.len() / 2;
                    self.stack.clear();
                }
                19 | 20 => {
                    self.take_width(true);
                    self.stems += self.stack.len() / 2;
                    self.stack.clear();
                    i += self.stems.div_ceil(8);
                }
                21 => {
                    self.take_width_n(2);
                    let n = self.stack.len();
                    if n >= 2 {
                        let (dx, dy) = (self.stack[n - 2], self.stack[n - 1]);
                        self.mv(dx, dy);
                    }
                    self.stack.clear();
                }
                22 => {
                    self.take_width_n(1);
                    if let Some(&dx) = self.stack.last() {
                        self.mv(dx, 0.0);
                    }
                    self.stack.clear();
                }
                4 => {
                    self.take_width_n(1);
                    if let Some(&dy) = self.stack.last() {
                        self.mv(0.0, dy);
                    }
                    self.stack.clear();
                }
                5 => {
                    let s = std::mem::take(&mut self.stack);
                    for c in s.chunks(2) {
                        if c.len() == 2 {
                            self.pt(c[0], c[1]);
                        }
                    }
                }
                6 | 7 => {
                    let s = std::mem::take(&mut self.stack);
                    let mut horiz = b0 == 6;
                    for v in s {
                        if horiz {
                            self.pt(v, 0.0);
                        } else {
                            self.pt(0.0, v);
                        }
                        horiz = !horiz;
                    }
                }
                8 => {
                    let s = std::mem::take(&mut self.stack);
                    for c in s.chunks(6) {
                        if c.len() == 6 {
                            self.curve(c[0], c[1], c[2], c[3], c[4], c[5]);
                        }
                    }
                }
                24 => {
                    let s = std::mem::take(&mut self.stack);
                    let n = s.len();
                    let mut k = 0;
                    while k + 6 <= n.saturating_sub(2) {
                        self.curve(s[k], s[k + 1], s[k + 2], s[k + 3], s[k + 4], s[k + 5]);
                        k += 6;
                    }
                    if k + 2 <= n {
                        self.pt(s[k], s[k + 1]);
                    }
                }
                25 => {
                    let s = std::mem::take(&mut self.stack);
                    let n = s.len();
                    let mut k = 0;
                    while k + 2 <= n.saturating_sub(6) {
                        self.pt(s[k], s[k + 1]);
                        k += 2;
                    }
                    if k + 6 <= n {
                        self.curve(s[k], s[k + 1], s[k + 2], s[k + 3], s[k + 4], s[k + 5]);
                    }
                }
                26 | 27 => {
                    let mut s = std::mem::take(&mut self.stack);
                    let mut extra = 0.0;
                    if s.len() % 4 == 1 {
                        extra = s.remove(0);
                    }
                    for c in s.chunks(4) {
                        if c.len() < 4 {
                            break;
                        }
                        if b0 == 26 {
                            self.curve(extra, c[0], c[1], c[2], 0.0, c[3]);
                        } else {
                            self.curve(c[0], extra, c[1], c[2], c[3], 0.0);
                        }
                        extra = 0.0;
                    }
                }
                30 | 31 => {
                    let s = std::mem::take(&mut self.stack);
                    let mut horiz = b0 == 31;
                    let mut k = 0;
                    while k + 4 <= s.len() {
                        let last = if k + 5 == s.len() { s[k + 4] } else { 0.0 };
                        if horiz {
                            self.curve(s[k], 0.0, s[k + 1], s[k + 2], last, s[k + 3]);
                        } else {
                            self.curve(0.0, s[k], s[k + 1], s[k + 2], s[k + 3], last);
                        }
                        horiz = !horiz;
                        k += 4;
                    }
                }
                10 | 29 => {
                    let Some(n) = self.stack.pop() else { return };
                    let list = if b0 == 10 { subrs } else { gsubrs };
                    let idx = n as i64 + bias(list.len());
                    if idx >= 0 && (idx as usize) < list.len() {
                        let r = list[idx as usize];
                        self.exec(d, r, subrs, gsubrs, depth + 1);
                    }
                }
                11 => return,
                14 => {
                    self.take_width_n(0);
                    self.done = true;
                    return;
                }
                12 => {
                    let b1 = *cs.get(i).unwrap_or(&0);
                    i += 1;
                    let s = std::mem::take(&mut self.stack);
                    match b1 {
                        35 if s.len() >= 12 => {
                            self.curve(s[0], s[1], s[2], s[3], s[4], s[5]);
                            self.curve(s[6], s[7], s[8], s[9], s[10], s[11]);
                        }
                        34 if s.len() >= 7 => {
                            self.curve(s[0], 0.0, s[1], s[2], s[3], 0.0);
                            self.curve(s[4], 0.0, s[5], -s[2], s[6], 0.0);
                        }
                        36 if s.len() >= 9 => {
                            self.curve(s[0], s[1], s[2], s[3], s[4], 0.0);
                            self.curve(s[5], 0.0, s[6], s[7], s[8], -(s[1] + s[3] + s[7]));
                        }
                        37 if s.len() >= 11 => {
                            let dx = s[0] + s[2] + s[4] + s[6] + s[8];
                            let dy = s[1] + s[3] + s[5] + s[7] + s[9];
                            let (lx, ly) = if dx.abs() > dy.abs() { (s[10], -dy) } else { (-dx, s[10]) };
                            self.curve(s[0], s[1], s[2], s[3], s[4], s[5]);
                            self.curve(s[6], s[7], s[8], s[9], lx, ly);
                        }
                        _ => {}
                    }
                }
                _ => self.stack.clear(),
            }
        }
    }

    fn take_width_n(&mut self, args: usize) {
        if self.width.is_none() {
            if self.stack.len() > args {
                self.width = Some(self.stack.remove(0));
            } else {
                self.width = Some(f64::NAN);
            }
        }
    }

    fn curve(&mut self, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) {
        self.pt(a, b);
        self.pt(c, d);
        self.pt(e, f);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostile_programs_do_not_panic() {
        for n in 0..64usize {
            let mut v = vec![0u8; n];
            if n > 4 {
                v[0] = 1;
                v[1] = 0;
                v[2] = 4;
            }
            let w: Work = Rc::new(Cell::new(1 << 30));
            let _ = Program::parse(&v, &w);
            let mut t = vec![0, 1, 0, 0];
            t.extend(std::iter::repeat_n(0xffu8, n));
            if let Some(p) = Program::parse(&t, &w) {
                let _ = p.glyph_bbox(3, true);
            }
        }
    }
}

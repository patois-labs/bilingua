//! Stream filters: Flate and LZW (with PNG and TIFF predictors), ASCII85, ASCIIHex, RunLength. Image codecs are
//! not decoded (a text layer never needs them); a stream that uses one decodes to nothing.
//!
//! Every decoder writes into a [`Budget`]: the bytes all streams of one document may unpack to. A decompression
//! bomb runs the budget out and the extraction stops with a typed limit error instead of filling memory.

use crate::obj::{Dict, Obj};

/// The bytes every stream of a document may unpack to, shared by all decoders of one extraction.
pub struct Budget {
    pub left: u64,
    pub exceeded: bool,
}

impl Budget {
    pub fn new(n: u64) -> Self {
        Budget { left: n, exceeded: false }
    }
    fn take(&mut self, n: usize) -> bool {
        if (n as u64) > self.left {
            self.left = 0;
            self.exceeded = true;
            return false;
        }
        self.left -= n as u64;
        true
    }
}

/// Decodes `data` through the stream's `/Filter` chain. `None` when a filter is unknown or an image codec (the
/// stream has no text); the bytes decoded so far are kept for a filter that fails part way, as PDFium does.
pub fn decode(dict: &Dict, data: &[u8], resolve: &dyn Fn(&Obj) -> Obj, budget: &mut Budget) -> Option<Vec<u8>> {
    let filters = match dict.get(b"Filter").map(resolve) {
        None | Some(Obj::Null) => return keep(data.to_vec(), budget),
        Some(Obj::Name(n)) => vec![n.to_vec()],
        Some(Obj::Array(a)) => {
            let mut v = Vec::new();
            for f in a.iter() {
                match resolve(f) {
                    Obj::Name(n) => v.push(n.to_vec()),
                    _ => return None,
                }
            }
            v
        }
        Some(_) => return None,
    };
    let params: Vec<Option<Dict>> = match dict.get(b"DecodeParms").or_else(|| dict.get(b"DP")).map(resolve) {
        Some(Obj::Dict(d)) => vec![Some((*d).clone())],
        Some(Obj::Array(a)) => a.iter().map(|p| resolve(p).as_dict().cloned()).collect(),
        _ => vec![],
    };
    let mut cur = data.to_vec();
    for (i, f) in filters.iter().enumerate() {
        let p = params.get(i).cloned().flatten();
        let p = p.as_ref();
        cur = match f.as_slice() {
            b"FlateDecode" | b"Fl" => predict(inflate(&cur, budget), p, resolve, budget)?,
            b"LZWDecode" | b"LZW" => {
                let early = p.and_then(|d| d.get(b"EarlyChange")).map(resolve).and_then(|o| o.int()).unwrap_or(1) != 0;
                predict(lzw(&cur, early, budget), p, resolve, budget)?
            }
            b"ASCII85Decode" | b"A85" => keep(a85(&cur), budget)?,
            b"ASCIIHexDecode" | b"AHx" => keep(ahx(&cur), budget)?,
            b"RunLengthDecode" | b"RL" => keep(rle(&cur), budget)?,
            b"Crypt" => cur,
            _ => return None,
        };
        if budget.exceeded {
            return None;
        }
    }
    Some(cur)
}

fn keep(v: Vec<u8>, budget: &mut Budget) -> Option<Vec<u8>> {
    if budget.take(v.len()) {
        Some(v)
    } else {
        None
    }
}

/// zlib inflate, stopping at the end of the data or the first error with what was decoded so far.
pub fn inflate(data: &[u8], budget: &mut Budget) -> Vec<u8> {
    let mut d = flate2::Decompress::new(true);
    let mut out: Vec<u8> = Vec::with_capacity((data.len() * 4).min(1 << 20));
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        let before_in = d.total_in() as usize;
        let before_out = d.total_out();
        let res = d.decompress(&data[before_in.min(data.len())..], &mut chunk, flate2::FlushDecompress::None);
        let produced = (d.total_out() - before_out) as usize;
        if produced > 0 {
            if !budget.take(produced) {
                return out;
            }
            out.extend_from_slice(&chunk[..produced]);
        }
        match res {
            Ok(flate2::Status::StreamEnd) => break,
            Ok(_) => {
                if produced == 0 && d.total_in() as usize == before_in {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    out
}

fn lzw(data: &[u8], early: bool, budget: &mut Budget) -> Vec<u8> {
    let mut out = Vec::new();
    let mut table: Vec<(u16, u8)> = Vec::with_capacity(4096); // (prefix code, last byte) for codes >= 258
    let mut width = 9u32;
    let (mut bits, mut nbits) = (0u32, 0u32);
    let mut prev: Option<u16> = None;
    let mut i = 0;
    let mut scratch = Vec::new();
    let expand = |code: u16, table: &Vec<(u16, u8)>, scratch: &mut Vec<u8>| -> Option<u8> {
        scratch.clear();
        let mut c = code;
        let mut guard = 0;
        loop {
            if c < 256 {
                scratch.push(c as u8);
                break;
            }
            let (p, b) = *table.get((c - 258) as usize)?;
            scratch.push(b);
            c = p;
            guard += 1;
            if guard > 4096 {
                return None;
            }
        }
        scratch.reverse();
        scratch.first().copied()
    };
    loop {
        while nbits < width {
            let Some(&b) = data.get(i) else { return out };
            i += 1;
            bits = (bits << 8) | b as u32;
            nbits += 8;
        }
        let code = ((bits >> (nbits - width)) & ((1 << width) - 1)) as u16;
        nbits -= width;
        if code == 256 {
            table.clear();
            width = 9;
            prev = None;
            continue;
        }
        if code == 257 {
            break;
        }
        let next_code = 258 + table.len() as u16;
        match prev {
            None => {
                if code >= 256 {
                    break;
                }
                if !budget.take(1) {
                    return out;
                }
                out.push(code as u8);
            }
            Some(p) => {
                let first = if code < next_code {
                    match expand(code, &table, &mut scratch) {
                        Some(f) => f,
                        None => break,
                    }
                } else if code == next_code {
                    let Some(f) = expand(p, &table, &mut scratch) else { break };
                    scratch.push(f);
                    f
                } else {
                    break;
                };
                if !budget.take(scratch.len()) {
                    return out;
                }
                out.extend_from_slice(&scratch);
                if table.len() < 4096 - 258 {
                    table.push((p, first));
                }
            }
        }
        prev = Some(code);
        let n = 258 + table.len() as u32 + if early { 1 } else { 0 };
        width = if n >= 2048 { 12 } else if n >= 1024 { 11 } else if n >= 512 { 10 } else { 9 };
    }
    out
}

fn predict(data: Vec<u8>, p: Option<&Dict>, resolve: &dyn Fn(&Obj) -> Obj, budget: &mut Budget) -> Option<Vec<u8>> {
    let Some(p) = p else { return Some(data) };
    let get = |k: &[u8], d: i64| p.get(k).map(resolve).and_then(|o| o.int()).unwrap_or(d);
    let predictor = get(b"Predictor", 1);
    let colors = get(b"Colors", 1);
    let bpc = get(b"BitsPerComponent", 8);
    let columns = get(b"Columns", 1);
    if colors < 0 || bpc < 0 || columns < 0 || columns.saturating_mul(colors).saturating_mul(bpc) > i32::MAX as i64 - 7 {
        return None;
    }
    if predictor < 2 {
        return Some(data);
    }
    let bpp = ((colors * bpc + 7) / 8).max(1) as usize;
    let row = ((columns * colors * bpc + 7) / 8) as usize;
    if row == 0 {
        return Some(data);
    }
    if predictor == 2 {
        if bpc != 8 {
            return Some(data);
        }
        let mut out = data;
        for r in out.chunks_mut(row) {
            for i in bpp..r.len() {
                r[i] = r[i].wrapping_add(r[i - bpp]);
            }
        }
        return Some(out);
    }
    // PNG predictors: every row starts with its filter type byte.
    let mut out = Vec::with_capacity(data.len());
    let mut prior = vec![0u8; row];
    for chunk in data.chunks(row + 1) {
        let t = chunk[0];
        let mut cur = chunk[1..].to_vec();
        cur.resize(row, 0);
        for i in 0..row {
            let a = if i >= bpp { cur[i - bpp] } else { 0 };
            let b = prior[i];
            let c = if i >= bpp { prior[i - bpp] } else { 0 };
            let v = cur[i];
            cur[i] = match t {
                1 => v.wrapping_add(a),
                2 => v.wrapping_add(b),
                3 => v.wrapping_add(((a as u16 + b as u16) / 2) as u8),
                4 => {
                    let pa = (b as i16 - c as i16).abs();
                    let pb = (a as i16 - c as i16).abs();
                    let pc = (a as i16 + b as i16 - 2 * c as i16).abs();
                    v.wrapping_add(if pa <= pb && pa <= pc { a } else if pb <= pc { b } else { c })
                }
                _ => v,
            };
        }
        let take = (chunk.len() - 1).min(row);
        out.extend_from_slice(&cur[..take]);
        prior = cur;
    }
    let _ = budget;
    Some(out)
}

fn a85(src: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len() * 4 / 5 + 4);
    let (mut state, mut res) = (0usize, 0u32);
    let mut pos = 0;
    while pos < src.len() {
        let ch = src[pos];
        pos += 1;
        if ch == b'\r' || ch == b'\n' || ch == b' ' || ch == b'\t' {
            continue;
        }
        if ch == b'z' {
            out.extend_from_slice(&[0, 0, 0, 0]);
            state = 0;
            res = 0;
            continue;
        }
        if !(b'!'..=b'u').contains(&ch) {
            break;
        }
        res = res.wrapping_mul(85).wrapping_add((ch - 33) as u32);
        if state < 4 {
            state += 1;
            continue;
        }
        out.extend_from_slice(&res.to_be_bytes());
        state = 0;
        res = 0;
    }
    if state > 0 {
        for _ in state..5 {
            res = res.wrapping_mul(85).wrapping_add(84);
        }
        out.extend_from_slice(&res.to_be_bytes()[..state - 1]);
    }
    out
}

fn ahx(src: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len() / 2 + 1);
    let mut hi: Option<u8> = None;
    for &ch in src {
        if ch == b'>' {
            break;
        }
        if let Some(d) = (ch as char).to_digit(16) {
            match hi.take() {
                None => hi = Some(d as u8),
                Some(h) => out.push(h * 16 + d as u8),
            }
        }
    }
    if let Some(h) = hi {
        out.push(h * 16);
    }
    out
}

fn rle(src: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < src.len() {
        let n = src[i];
        if n == 128 {
            break;
        }
        if n < 128 {
            let len = n as usize + 1;
            let end = (i + 1 + len).min(src.len());
            out.extend_from_slice(&src[(i + 1).min(src.len())..end]);
            if end - (i + 1).min(end) < len {
                out.resize(out.len() + len - (end - (i + 1).min(end)), 0);
            }
            i += len + 1;
        } else {
            let b = src.get(i + 1).copied().unwrap_or(0);
            out.extend(std::iter::repeat_n(b, 257 - n as usize));
            i += 2;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(o: &Obj) -> Obj {
        o.clone()
    }

    #[test]
    fn filters_decode() {
        assert_eq!(a85(b"87cURD]i,\"Ebo80~>"), b"Hello World!");
        assert_eq!(ahx(b"48 65 6c6c6f>"), b"Hello");
        assert_eq!(ahx(b"414"), b"A@");
        assert_eq!(rle(&[2, b'a', b'b', b'c', 254, b'x', 128]), b"abcxxx");
        let z = {
            use std::io::Write;
            let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            e.write_all(&vec![b'q'; 10_000]).unwrap();
            e.finish().unwrap()
        };
        let mut b = Budget::new(1 << 20);
        assert_eq!(inflate(&z, &mut b).len(), 10_000);
        let mut small = Budget::new(100);
        let _ = inflate(&z, &mut small);
        assert!(small.exceeded);
        let d = Dict::default();
        assert_eq!(decode(&d, b"abc", &none, &mut Budget::new(10)).unwrap(), b"abc");
    }

    #[test]
    fn lzw_spec_example() {
        // The example from the PDF reference: 45 45 45 45 45 65 45 45 45 66.
        let enc = [0x80, 0x0B, 0x60, 0x50, 0x22, 0x0C, 0x0C, 0x85, 0x01];
        assert_eq!(lzw(&enc, true, &mut Budget::new(1000)), vec![45, 45, 45, 45, 45, 65, 45, 45, 45, 66]);
    }
}

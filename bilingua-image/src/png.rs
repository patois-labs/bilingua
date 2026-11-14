//! PNG structure: chunk walk with checksums, header limits, pixel-data check, and the text chunks (`tEXt`, `zTXt`,
//! `iTXt`) where Localization OS keeps its region sidecar. Pixels are never decoded or changed; the pixel data is
//! only inflated to prove it is a complete zlib stream of the size the header promises.

use crate::refuse;
use bilingua_model::Error;
use flate2::{Decompress, FlushDecompress, Status};

pub const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
/// Pillow's `MAX_TEXT_CHUNK` and `MAX_TEXT_MEMORY`.
const MAX_TEXT_CHUNK: usize = 1024 * 1024;
const MAX_TEXT_MEMORY: usize = 64 * MAX_TEXT_CHUNK;
pub const MAX_PIXEL_DIM: u32 = 8000;
pub const MAX_TOTAL_PIXELS: u64 = 25_000_000;

#[derive(Debug, Clone)]
pub struct Chunk {
    pub ty: [u8; 4],
    /// Offset of the length field.
    pub start: usize,
    /// Offset just after the data.
    pub data_end: usize,
    /// Offset just after the CRC.
    pub end: usize,
}

impl Chunk {
    pub fn data<'a>(&self, b: &'a [u8]) -> &'a [u8] {
        &b[self.start + 8..self.data_end]
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct Png {
    pub width: u32,
    pub height: u32,
    pub chunks: Vec<Chunk>,
}

fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

/// Walks and checks a PNG. `verify_pixels` inflates the image data.
pub fn walk(b: &[u8], verify_pixels: bool) -> Result<Png, Error> {
    if !b.starts_with(SIGNATURE) {
        return Err(refuse("not a decodable image: the content is not a PNG"));
    }
    let mut pos = 8usize;
    let mut chunks: Vec<Chunk> = Vec::new();
    let (mut w, mut h) = (0u32, 0u32);
    let mut ihdr: Option<[u8; 13]> = None;
    let mut seen_end = false;
    while pos < b.len() && !seen_end {
        // Pillow stops quietly at a file cut off after the image data has started, as long as the pixels decode
        let seen_idat = chunks.iter().any(|c| &c.ty == b"IDAT");
        if b.len() - pos < 12 {
            if seen_idat {
                break;
            }
            return Err(refuse("not a decodable image: the PNG is truncated"));
        }
        let len = be32(&b[pos..]) as usize;
        if len > 0x7FFF_FFFF || pos + 12 + len > b.len() {
            if seen_idat || (&b[pos + 4..pos + 8] == b"IDAT" && len <= 0x7FFF_FFFF) {
                if &b[pos + 4..pos + 8] == b"IDAT" && len <= 0x7FFF_FFFF && !chunks.is_empty() {
                    chunks.push(Chunk { ty: *b"IDAT", start: pos, data_end: b.len(), end: b.len() });
                }
                break;
            }
            return Err(refuse("not a decodable image: a PNG chunk runs past the end of the file"));
        }
        let ty = [b[pos + 4], b[pos + 5], b[pos + 6], b[pos + 7]];
        if !ty.iter().all(u8::is_ascii_alphabetic) {
            return Err(refuse("not a decodable image: broken PNG chunk type"));
        }
        let end = pos + 12 + len;
        let crc = be32(&b[end - 4..]);
        if crc32fast::hash(&b[pos + 4..end - 4]) != crc {
            return Err(refuse(&format!("not a decodable image: broken PNG file (bad checksum in {})", String::from_utf8_lossy(&ty))));
        }
        if chunks.is_empty() {
            if &ty != b"IHDR" || len != 13 {
                return Err(refuse("not a decodable image: the PNG does not start with its header chunk"));
            }
            let mut a = [0u8; 13];
            a.copy_from_slice(&b[pos + 8..pos + 21]);
            w = be32(&a[0..]);
            h = be32(&a[4..]);
            ihdr = Some(a);
        }
        if &ty == b"IEND" {
            seen_end = true;
        }
        chunks.push(Chunk { ty, start: pos, data_end: end - 4, end });
        pos = end;
    }
    let Some(ihdr) = ihdr else { return Err(refuse("not a decodable image: empty PNG")) };
    if !seen_end && !chunks.iter().any(|c| &c.ty == b"IDAT") {
        return Err(refuse("not a decodable image: the PNG has no image data (truncated)"));
    }
    if w == 0 || h == 0 {
        return Err(refuse("not a decodable image: the PNG has no pixels"));
    }
    if w > MAX_PIXEL_DIM || h > MAX_PIXEL_DIM {
        return Err(refuse(&format!("image dimensions {w}x{h} exceed the {MAX_PIXEL_DIM}px guard")));
    }
    if u64::from(w) * u64::from(h) > MAX_TOTAL_PIXELS {
        return Err(refuse(&format!(
            "image is {:.0} megapixels ({w}x{h}); the total-pixel guard allows {} megapixels",
            (u64::from(w) * u64::from(h)) as f64 / 1e6,
            MAX_TOTAL_PIXELS / 1_000_000
        )));
    }
    let (depth, color, comp, filt, interlace) = (ihdr[8], ihdr[9], ihdr[10], ihdr[11], ihdr[12]);
    let channels = match (color, depth) {
        (0, 1 | 2 | 4 | 8 | 16) => 1,
        (2, 8 | 16) => 3,
        (3, 1 | 2 | 4 | 8) => 1,
        (4, 8 | 16) => 2,
        (6, 8 | 16) => 4,
        _ => return Err(refuse("not a decodable image: unsupported PNG color type or bit depth")),
    };
    if comp != 0 || filt != 0 || interlace > 1 {
        return Err(refuse("not a decodable image: unsupported PNG header"));
    }
    // animation, as Pillow decides it: the frame count of the first valid acTL (a second one cancels it); an image
    // data chunk with no frame control chunk before it is a default image that counts as one more frame
    let mut n_frames: Option<u64> = None;
    let mut seq_seen = false;
    for c in &chunks {
        match &c.ty {
            b"acTL" => {
                let d = c.data(b);
                if d.len() < 8 {
                    return Err(refuse("not a decodable image: APNG contains truncated acTL chunk"));
                }
                let n = u64::from(be32(d));
                if n_frames.is_some() {
                    n_frames = None;
                } else if n != 0 && n <= 0x8000_0000 {
                    n_frames = Some(n);
                }
            }
            b"fcTL" => {
                let d = c.data(b);
                if d.len() < 26 {
                    return Err(refuse("not a decodable image: APNG contains truncated fcTL chunk"));
                }
                if !seq_seen && be32(d) != 0 {
                    return Err(refuse("not a decodable image: APNG contains frame sequence errors"));
                }
                seq_seen = true;
            }
            b"IDAT" => break,
            _ => {}
        }
    }
    if let Some(n) = n_frames {
        let total = if seq_seen { n } else { n + 1 };
        if total > 1 {
            return Err(crate::refused_frames("This PNG is animated. Image translation draws on a single frame and would flatten it, so it was not processed. Export the frame you need as a single-frame PNG and upload that."));
        }
    }
    if verify_pixels {
        let expected = raw_size(w as u64, h as u64, u64::from(depth) * channels, interlace == 1);
        check_pixels(b, &chunks, expected)?;
    }
    Ok(Png { width: w, height: h, chunks })
}

fn raw_size(w: u64, h: u64, bits_per_pixel: u64, interlaced: bool) -> u64 {
    let rows = |w: u64, h: u64| if w == 0 || h == 0 { 0 } else { h * (1 + (w * bits_per_pixel).div_ceil(8)) };
    if !interlaced {
        return rows(w, h);
    }
    const PASSES: [(u64, u64, u64, u64); 7] = [(0, 0, 8, 8), (4, 0, 8, 8), (0, 4, 4, 8), (2, 0, 4, 4), (0, 2, 2, 4), (1, 0, 2, 2), (0, 1, 1, 2)];
    PASSES.iter().map(|&(x0, y0, dx, dy)| rows(w.saturating_sub(x0).div_ceil(dx), h.saturating_sub(y0).div_ceil(dy))).sum()
}

fn check_pixels(b: &[u8], chunks: &[Chunk], expected: u64) -> Result<(), Error> {
    let mut d = Decompress::new(true);
    let mut buf = vec![0u8; 64 * 1024];
    let mut produced = 0u64;
    let mut ended = false;
    let mut any = false;
    'outer: for c in chunks.iter().filter(|c| &c.ty == b"IDAT") {
        any = true;
        let mut input = c.data(b);
        while !input.is_empty() {
            let (ti, to) = (d.total_in(), d.total_out());
            let st = d.decompress(input, &mut buf, FlushDecompress::None).map_err(|_| refuse("not a decodable image: the PNG pixel data is corrupt"))?;
            input = &input[(d.total_in() - ti) as usize..];
            produced += d.total_out() - to;
            if st == Status::StreamEnd {
                ended = true;
                break 'outer;
            }
            if produced >= expected {
                break 'outer;
            }
            if d.total_in() == ti && d.total_out() == to {
                break;
            }
        }
    }
    if !any {
        return Err(refuse("not a decodable image: the PNG has no image data"));
    }
    let _ = ended;
    if produced < expected {
        return Err(refuse("not a decodable image: the PNG pixel data is truncated"));
    }
    Ok(())
}

fn inflate_limited(data: &[u8]) -> Result<Option<Vec<u8>>, Error> {
    // Err = too large (refuse), Ok(None) = zlib error (Pillow keeps nothing)
    let mut d = Decompress::new(true);
    let mut out: Vec<u8> = Vec::new();
    let mut buf = vec![0u8; 16 * 1024];
    let mut input = data;
    loop {
        let (ti, to) = (d.total_in(), d.total_out());
        let st = match d.decompress(input, &mut buf, FlushDecompress::None) {
            Ok(s) => s,
            Err(_) => return Ok(None),
        };
        input = &input[(d.total_in() - ti) as usize..];
        let n = (d.total_out() - to) as usize;
        out.extend_from_slice(&buf[..n]);
        if out.len() > MAX_TEXT_CHUNK {
            return Err(refuse("not a decodable image: decompressed PNG text is too large"));
        }
        if st == Status::StreamEnd {
            return Ok(Some(out));
        }
        if d.total_in() == ti && d.total_out() == to {
            return Ok(Some(out));
        }
    }
}

/// A text chunk as Pillow reads it.
#[derive(Debug, Clone)]
pub struct Text {
    /// Index into `Png::chunks`.
    pub chunk: usize,
    pub key: String,
    pub value: String,
    /// For an `iTXt`: the language and translated keyword, kept when the chunk is rewritten.
    pub itxt: Option<(Vec<u8>, Vec<u8>)>,
}

fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| c as char).collect()
}

/// Every text chunk in file order, with Pillow's rules (broken ones skipped, a bomb or an unknown compression
/// refused). The effective value of a key is the last one.
pub fn texts(b: &[u8], png: &Png) -> Result<Vec<Text>, Error> {
    let mut out = Vec::new();
    let mut memory = 0usize;
    let mut bump = |n: usize| -> Result<(), Error> {
        memory += n;
        if memory > MAX_TEXT_MEMORY {
            Err(refuse("not a decodable image: too much memory used in PNG text chunks"))
        } else {
            Ok(())
        }
    };
    for (ci, c) in png.chunks.iter().enumerate() {
        let d = c.data(b);
        match &c.ty {
            b"tEXt" => {
                let (k, v) = match d.iter().position(|&x| x == 0) {
                    Some(p) => (&d[..p], &d[p + 1..]),
                    None => (d, &d[..0]),
                };
                if !k.is_empty() {
                    let v = latin1(v);
                    bump(v.chars().count())?;
                    out.push(Text { chunk: ci, key: latin1(k), value: v, itxt: None });
                }
            }
            b"zTXt" => {
                let (k, v) = match d.iter().position(|&x| x == 0) {
                    Some(p) => (&d[..p], &d[p + 1..]),
                    None => (d, &d[..0]),
                };
                let method = v.first().copied().unwrap_or(0);
                if method != 0 {
                    return Err(refuse(&format!("not a decodable image: unknown compression method {method} in zTXt chunk")));
                }
                let plain = if v.is_empty() { Some(Vec::new()) } else { inflate_limited(&v[1..])? }.unwrap_or_default();
                if !k.is_empty() {
                    let s = latin1(&plain);
                    bump(s.chars().count())?;
                    out.push(Text { chunk: ci, key: latin1(k), value: s, itxt: None });
                }
            }
            b"iTXt" => {
                let Some(p) = d.iter().position(|&x| x == 0) else { continue };
                let (k, r) = (&d[..p], &d[p + 1..]);
                if r.len() < 2 {
                    continue;
                }
                let (cf, cm, r) = (r[0], r[1], &r[2..]);
                let mut parts = r.splitn(3, |&x| x == 0);
                let (Some(lang), Some(tk), Some(v)) = (parts.next(), parts.next(), parts.next()) else { continue };
                let v: Vec<u8> = if cf != 0 {
                    if cm != 0 {
                        continue;
                    }
                    match inflate_limited(v)? {
                        Some(x) => x,
                        None => continue,
                    }
                } else {
                    v.to_vec()
                };
                let (Ok(_), Ok(_), Ok(vs)) = (std::str::from_utf8(lang), std::str::from_utf8(tk), std::str::from_utf8(&v)) else { continue };
                bump(vs.chars().count())?;
                out.push(Text { chunk: ci, key: latin1(k), value: vs.to_string(), itxt: Some((lang.to_vec(), tk.to_vec())) });
            }
            _ => {}
        }
    }
    Ok(out)
}

/// The effective text for a key: the last chunk that holds it.
pub fn last<'a>(t: &'a [Text], key: &str) -> Option<&'a Text> {
    t.iter().rev().find(|x| x.key == key)
}

fn chunk_bytes(ty: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(data.len() + 12);
    v.extend_from_slice(&(data.len() as u32).to_be_bytes());
    v.extend_from_slice(ty);
    v.extend_from_slice(data);
    let crc = crc32fast::hash(&v[4..]);
    v.extend_from_slice(&crc.to_be_bytes());
    v
}

/// The file with the text chunk `t` replaced by one holding `value`: `tEXt` stays `tEXt` and `zTXt` becomes `tEXt`
/// (both Latin-1, so `value` must have only characters up to U+00FF), `iTXt` stays an uncompressed `iTXt`.
pub fn replace_text(b: &[u8], png: &Png, t: &Text, value: &str) -> Result<Vec<u8>, Error> {
    let c = &png.chunks[t.chunk];
    let key: Vec<u8> = t.key.chars().map(|ch| ch as u32 as u8).collect();
    let new = match &t.itxt {
        None => {
            let mut data = key;
            data.push(0);
            for ch in value.chars() {
                if ch as u32 > 0xFF {
                    return Err(Error::Write("the image text layer holds a character a tEXt chunk cannot".into()));
                }
                data.push(ch as u32 as u8);
            }
            chunk_bytes(b"tEXt", &data)
        }
        Some((lang, tk)) => {
            let mut data = key;
            data.extend_from_slice(&[0, 0, 0]);
            data.extend_from_slice(lang);
            data.push(0);
            data.extend_from_slice(tk);
            data.push(0);
            data.extend_from_slice(value.as_bytes());
            chunk_bytes(b"iTXt", &data)
        }
    };
    let mut out = Vec::with_capacity(b.len() + new.len());
    out.extend_from_slice(&b[..c.start]);
    out.extend_from_slice(&new);
    out.extend_from_slice(&b[c.end..]);
    Ok(out)
}

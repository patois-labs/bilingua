//! Telling the file kinds apart by their bytes (never the extension) and reading the headers of the raster kinds
//! that carry no text layer: size limits, single frame. Nothing here decodes pixels.

use crate::png::{MAX_PIXEL_DIM, MAX_TOTAL_PIXELS};
use crate::{refuse, refused_frames};
use bilingua_model::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Svg,
    Png,
    Jpeg,
    Tiff,
    Bmp,
    Gif,
    Webp,
}

/// LocOS `sniff_kind`: raster magic numbers first, then an SVG look in the first 4 KB. This is the loose LocOS
/// rule (any `<?xml` file counts as SVG there); [`is_svg_strict`] is what the facade should sniff with.
pub fn sniff(d: &[u8]) -> Option<Kind> {
    if d.is_empty() {
        return None;
    }
    if d.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some(Kind::Png);
    }
    if d.starts_with(b"\xff\xd8") {
        return Some(Kind::Jpeg);
    }
    if d.starts_with(b"II*\0") || d.starts_with(b"MM\0*") || d.starts_with(b"II+\0") || d.starts_with(b"MM\0+") {
        return Some(Kind::Tiff);
    }
    if d.starts_with(b"BM") && d.len() >= 26 {
        return Some(Kind::Bmp);
    }
    if d.starts_with(b"GIF87a") || d.starts_with(b"GIF89a") {
        return Some(Kind::Gif);
    }
    if d.len() >= 12 && &d[..4] == b"RIFF" && &d[8..12] == b"WEBP" {
        return Some(Kind::Webp);
    }
    let head = &d[..d.len().min(4096)];
    let lead = head.iter().position(|&c| !matches!(c, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)).unwrap_or(head.len());
    let head = &head[lead..];
    if head.starts_with(b"<?xml") || head.starts_with(b"<svg") || head[..head.len().min(2048)].windows(4).any(|w| w == b"<svg") {
        return Some(Kind::Svg);
    }
    None
}

fn text_head(d: &[u8]) -> Vec<u8> {
    let head = &d[..d.len().min(8192)];
    if head.starts_with(&[0xFF, 0xFE]) || head.starts_with(&[0xFE, 0xFF]) {
        let le = head[0] == 0xFF;
        return head[2..].chunks_exact(2).map(|p| if le { if p[1] == 0 { p[0] } else { b'?' } } else if p[0] == 0 { p[1] } else { b'?' }).collect();
    }
    head.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(head).to_vec()
}

fn svg_start_at(h: &[u8], i: usize) -> bool {
    h.len() > i + 4 && h[i..i + 4].eq_ignore_ascii_case(b"<svg") && matches!(h[i + 4], b' ' | b'\t' | b'\n' | b'\r' | b'>' | b'/')
}

/// A tight SVG test for format detection: the root element (after an XML declaration, comments, processing
/// instructions and a DOCTYPE) is `<svg`, in UTF-8 or UTF-16.
pub fn is_svg_strict(d: &[u8]) -> bool {
    let h = text_head(d);
    let mut i = 0usize;
    loop {
        while i < h.len() && h[i].is_ascii_whitespace() {
            i += 1;
        }
        let rest = &h[i.min(h.len())..];
        let skip = |end: &[u8]| rest.windows(end.len()).position(|w| w == end).map(|p| p + end.len());
        if rest.starts_with(b"<?") {
            match skip(b"?>") {
                Some(n) => i += n,
                None => return false,
            }
        } else if rest.starts_with(b"<!--") {
            match skip(b"-->") {
                Some(n) => i += n,
                None => return false,
            }
        } else if rest.len() > 9 && rest[..9].eq_ignore_ascii_case(b"<!DOCTYPE") {
            let mut depth = 0usize;
            let mut j = 9;
            while j < rest.len() && !(rest[j] == b'>' && depth == 0) {
                match rest[j] {
                    b'[' => depth += 1,
                    b']' => depth = depth.saturating_sub(1),
                    _ => {}
                }
                j += 1;
            }
            if j >= rest.len() {
                return false;
            }
            i += j + 1;
        } else {
            return svg_start_at(&h, i);
        }
    }
}

/// The looser test the SVG filter extracts with: LocOS's rule, or an `<svg` start tag anywhere in the first 8 KB.
pub fn looks_like_svg(d: &[u8]) -> bool {
    if sniff(d) == Some(Kind::Svg) {
        return true;
    }
    if sniff(d).is_some() {
        return false;
    }
    let h = text_head(d);
    (0..h.len()).any(|i| svg_start_at(&h, i))
}

fn check_dims(w: u64, h: u64) -> Result<(), Error> {
    if w == 0 || h == 0 {
        return Err(refuse("not a decodable image: the image has no pixels"));
    }
    if w > u64::from(MAX_PIXEL_DIM) || h > u64::from(MAX_PIXEL_DIM) {
        return Err(refuse(&format!("image dimensions {w}x{h} exceed the {MAX_PIXEL_DIM}px guard")));
    }
    if w * h > MAX_TOTAL_PIXELS {
        return Err(refuse(&format!(
            "image is {:.0} megapixels ({w}x{h}); the total-pixel guard allows {} megapixels",
            (w * h) as f64 / 1e6,
            MAX_TOTAL_PIXELS / 1_000_000
        )));
    }
    Ok(())
}

fn le16(b: &[u8], p: usize) -> Option<u64> {
    Some(u64::from(u16::from_le_bytes([*b.get(p)?, *b.get(p + 1)?])))
}

fn le32(b: &[u8], p: usize) -> Option<u64> {
    Some(u64::from(u32::from_le_bytes([*b.get(p)?, *b.get(p + 1)?, *b.get(p + 2)?, *b.get(p + 3)?])))
}

fn truncated() -> Error {
    refuse("not a decodable image: the file is truncated or its header is broken")
}

/// Checks the header of a non-PNG raster: size limits and one frame.
pub fn check_raster(d: &[u8], kind: Kind) -> Result<(), Error> {
    match kind {
        Kind::Jpeg => jpeg(d),
        Kind::Gif => gif(d),
        Kind::Bmp => bmp(d),
        Kind::Webp => webp(d),
        Kind::Tiff => tiff(d),
        Kind::Png | Kind::Svg => Ok(()),
    }
}

fn jpeg(d: &[u8]) -> Result<(), Error> {
    let mut p = 2usize;
    while p + 4 <= d.len() {
        if d[p] != 0xFF {
            p += 1;
            continue;
        }
        let m = d[p + 1];
        if m == 0xFF {
            p += 1;
            continue;
        }
        if m == 0xD8 || m == 0x01 || (0xD0..=0xD7).contains(&m) || m == 0 {
            p += 2;
            continue;
        }
        if m == 0xD9 || m == 0xDA {
            break;
        }
        let len = u64::from(u16::from_be_bytes([d[p + 2], d[p + 3]])) as usize;
        if (0xC0..=0xCF).contains(&m) && !matches!(m, 0xC4 | 0xC8 | 0xCC) {
            if p + 9 > d.len() {
                return Err(truncated());
            }
            let h = u64::from(u16::from_be_bytes([d[p + 5], d[p + 6]]));
            let w = u64::from(u16::from_be_bytes([d[p + 7], d[p + 8]]));
            return check_dims(w, h);
        }
        p += 2 + len.max(2);
    }
    Err(truncated())
}

fn gif(d: &[u8]) -> Result<(), Error> {
    let (w, h) = (le16(d, 6).ok_or_else(truncated)?, le16(d, 8).ok_or_else(truncated)?);
    check_dims(w, h)?;
    let flags = *d.get(10).ok_or_else(truncated)?;
    let mut p = 13usize;
    if flags & 0x80 != 0 {
        p += 3 * (1usize << ((flags & 7) + 1));
    }
    let mut frames = 0u32;
    let skip_blocks = |mut p: usize| -> Result<usize, Error> {
        loop {
            let n = *d.get(p).ok_or_else(truncated)? as usize;
            p += 1 + n;
            if n == 0 {
                return Ok(p);
            }
        }
    };
    loop {
        match *d.get(p).ok_or_else(truncated)? {
            0x3B => break,
            0x21 => p = skip_blocks(p + 2)?,
            0x2C => {
                frames += 1;
                if frames > 1 {
                    return Err(refused_frames("This GIF is animated. Image translation draws on a single frame and would flatten it, so it was not processed. Export the frame you need as a single-frame PNG (or a still GIF, WebP or TIFF) and upload that."));
                }
                let f = *d.get(p + 9).ok_or_else(truncated)?;
                p += 10;
                if f & 0x80 != 0 {
                    p += 3 * (1usize << ((f & 7) + 1));
                }
                p = skip_blocks(p + 1)?;
            }
            _ => return Err(truncated()),
        }
    }
    if frames == 0 {
        return Err(truncated());
    }
    Ok(())
}

fn bmp(d: &[u8]) -> Result<(), Error> {
    let size = le32(d, 14).ok_or_else(truncated)?;
    let (w, h) = if size == 12 {
        (le16(d, 18).ok_or_else(truncated)?, le16(d, 20).ok_or_else(truncated)?)
    } else if size >= 40 {
        let w = le32(d, 18).ok_or_else(truncated)? as u32 as i32;
        let h = le32(d, 22).ok_or_else(truncated)? as u32 as i32;
        (u64::from(w.unsigned_abs()), u64::from(h.unsigned_abs()))
    } else {
        return Err(refuse("not a decodable image: unsupported BMP header"));
    };
    check_dims(w, h)
}

fn webp(d: &[u8]) -> Result<(), Error> {
    let ty = d.get(12..16).ok_or_else(truncated)?;
    let c = 20usize;
    match ty {
        b"VP8 " => {
            if d.get(c + 3..c + 6) != Some(&[0x9d, 0x01, 0x2a]) {
                return Err(truncated());
            }
            check_dims(le16(d, c + 6).ok_or_else(truncated)? & 0x3FFF, le16(d, c + 8).ok_or_else(truncated)? & 0x3FFF)
        }
        b"VP8L" => {
            if d.get(c) != Some(&0x2f) {
                return Err(truncated());
            }
            let bits = le32(d, c + 1).ok_or_else(truncated)?;
            check_dims((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1)
        }
        b"VP8X" => {
            let flags = *d.get(c).ok_or_else(truncated)?;
            let w = 1 + (le16(d, c + 4).ok_or_else(truncated)? | (u64::from(*d.get(c + 6).ok_or_else(truncated)?) << 16));
            let h = 1 + (le16(d, c + 7).ok_or_else(truncated)? | (u64::from(*d.get(c + 9).ok_or_else(truncated)?) << 16));
            check_dims(w, h)?;
            if flags & 0x02 != 0 {
                return Err(refused_frames("This WEBP is animated. Image translation draws on a single frame and would flatten it, so it was not processed. Export the frame you need as a single-frame PNG (or a still GIF, WebP or TIFF) and upload that."));
            }
            Ok(())
        }
        _ => Err(truncated()),
    }
}

fn tiff(d: &[u8]) -> Result<(), Error> {
    if d.len() < 8 || d[2] == b'+' || d[3] == b'+' {
        return Err(refuse("not a decodable image: unsupported TIFF (BigTIFF)"));
    }
    let le = d[0] == b'I';
    let u16at = |p: usize| -> Option<u64> {
        let b = [*d.get(p)?, *d.get(p + 1)?];
        Some(u64::from(if le { u16::from_le_bytes(b) } else { u16::from_be_bytes(b) }))
    };
    let u32at = |p: usize| -> Option<u64> {
        let b = [*d.get(p)?, *d.get(p + 1)?, *d.get(p + 2)?, *d.get(p + 3)?];
        Some(u64::from(if le { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) }))
    };
    let ifd = u32at(4).ok_or_else(truncated)? as usize;
    let n = u16at(ifd).ok_or_else(truncated)? as usize;
    let (mut w, mut h) = (0u64, 0u64);
    for i in 0..n {
        let e = ifd + 2 + i * 12;
        let tag = u16at(e).ok_or_else(truncated)?;
        if tag == 256 || tag == 257 {
            let v = if u16at(e + 2).ok_or_else(truncated)? == 3 { u16at(e + 8) } else { u32at(e + 8) }.ok_or_else(truncated)?;
            if tag == 256 {
                w = v;
            } else {
                h = v;
            }
        }
    }
    check_dims(w, h)?;
    let next = u32at(ifd + 2 + n * 12).ok_or_else(truncated)?;
    if next != 0 {
        return Err(refused_frames("This TIFF has more than one page. Image translation draws on a single frame and would flatten it, so it was not processed. Export the page you need as a single-frame PNG (or a still GIF, WebP or TIFF) and upload that."));
    }
    Ok(())
}

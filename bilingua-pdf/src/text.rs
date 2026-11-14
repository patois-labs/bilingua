//! Unicode helpers with PDFium's data: bidi directions, mirroring, the decomposition applied to right-to-left
//! text and ligatures, and PDF text strings (PDFDocEncoding or UTF-16 with a byte order mark).

use crate::tables;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    Neutral,
    Left,
    LeftWeak,
    Right,
}

pub fn dir(u: u16) -> Dir {
    let i = tables::BIDI_RUNS.partition_point(|(start, _)| *start <= u);
    match tables::BIDI_RUNS[i.saturating_sub(1)].1 {
        1 => Dir::Left,
        2 => Dir::LeftWeak,
        3 => Dir::Right,
        _ => Dir::Neutral,
    }
}

pub fn mirror(u: u16) -> u16 {
    match tables::MIRROR.binary_search_by_key(&u, |(a, _)| *a) {
        Ok(i) => tables::MIRROR[i].1,
        Err(_) => u,
    }
}

/// PDFium's `GetUnicodeNormalization`: the replacement sequence, or the char itself.
pub fn normalize(u: u16) -> Vec<u16> {
    match tables::NORMALIZE.binary_search_by_key(&u, |(a, _)| *a) {
        Ok(i) => tables::NORMALIZE[i].1.to_vec(),
        Err(_) => vec![u],
    }
}

/// One bidi segment: start, count, direction.
#[derive(Clone, Copy, Debug)]
pub struct Segment {
    pub start: usize,
    pub count: usize,
    pub dir: Dir,
}

/// PDFium's `CFX_BidiString` with `auto_order`: runs of one direction; the order is reversed when the right to
/// left runs outnumber the left to right ones (or when `force_rtl`).
pub fn bidi(s: &[u16], force_rtl: bool) -> (Vec<Segment>, Dir) {
    let mut out: Vec<Segment> = Vec::new();
    let mut cur = Segment { start: 0, count: 0, dir: Dir::Neutral };
    for &c in s {
        let d = dir(c);
        if d != cur.dir {
            if cur.count > 0 {
                out.push(cur);
            }
            cur = Segment { start: cur.start + cur.count, count: 0, dir: d };
        }
        cur.count += 1;
    }
    if cur.count > 0 {
        out.push(cur);
    }
    let r = out.iter().filter(|s| s.dir == Dir::Right).count();
    let l = out.iter().filter(|s| s.dir == Dir::Left).count();
    let mut overall = Dir::Left;
    if r > l || force_rtl {
        out.reverse();
        overall = Dir::Right;
    }
    (out, overall)
}

/// A PDF text string as UTF-16 units: UTF-16BE or LE with a BOM, UTF-8 with a BOM, else PDFDocEncoding.
pub fn pdf_text_string(b: &[u8]) -> Vec<u16> {
    if b.len() >= 2 && b[0] == 0xFE && b[1] == 0xFF {
        return b[2..].chunks(2).filter(|c| c.len() == 2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
    }
    if b.len() >= 2 && b[0] == 0xFF && b[1] == 0xFE {
        return b[2..].chunks(2).filter(|c| c.len() == 2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    }
    if b.len() >= 3 && b[..3] == [0xEF, 0xBB, 0xBF] {
        return String::from_utf8_lossy(&b[3..]).encode_utf16().collect();
    }
    b.iter().map(|c| tables::PDFDOC_CODES[*c as usize]).collect()
}

/// PDFium's `FXSYS_iswalpha` (ICU alphabetic) on a UTF-16 unit.
pub fn is_alpha(u: u16) -> bool {
    char::from_u32(u as u32).is_some_and(|c| c.is_alphabetic())
}

/// PDFium's `FXSYS_iswalnum`.
pub fn is_alnum(u: u16) -> bool {
    char::from_u32(u as u32).is_some_and(|c| c.is_alphabetic() || c.is_numeric())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directions() {
        assert_eq!(dir(b'a' as u16), Dir::Left);
        assert_eq!(dir(0x0627), Dir::Right);
        assert_eq!(dir(0x05D0), Dir::Right);
        assert_eq!(dir(b'1' as u16), Dir::LeftWeak);
        assert_eq!(dir(b' ' as u16), Dir::Neutral);
        assert_eq!(mirror(b'(' as u16), b')' as u16);
        assert_eq!(normalize(0xFB01), vec![b'f' as u16, b'i' as u16]);
    }
}

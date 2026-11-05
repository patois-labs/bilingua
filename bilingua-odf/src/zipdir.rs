//! A scan of the zip central directory for two members with the same name. The zip reader keeps one entry per
//! name, so a package that lists `content.xml` twice would show this filter one part and another reader the other.

use std::collections::HashSet;

fn u16at(b: &[u8], i: usize) -> Option<usize> {
    Some(u16::from_le_bytes(b.get(i..i + 2)?.try_into().ok()?) as usize)
}
fn u32at(b: &[u8], i: usize) -> Option<usize> {
    Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?) as usize)
}

/// The first member name (as written) that appears twice in the central directory, comparing names the way the
/// package reader does (case-insensitive, `\` as `/`). `None` when there is none or the directory cannot be walked
/// (the reader then refuses the file on its own).
pub fn duplicate_name(b: &[u8]) -> Option<String> {
    let lo = b.len().saturating_sub(22 + 65535);
    let eocd = (lo..=b.len().checked_sub(22)?).rev().find(|i| b[*i..].starts_with(b"PK\x05\x06"))?;
    let count = u16at(b, eocd + 10)?;
    let mut at = u32at(b, eocd + 16)?;
    let mut seen: HashSet<String> = HashSet::new();
    for _ in 0..count {
        if b.get(at..at + 4)? != b"PK\x01\x02" {
            return None;
        }
        let (n, x, c) = (u16at(b, at + 28)?, u16at(b, at + 30)?, u16at(b, at + 32)?);
        let raw = String::from_utf8_lossy(b.get(at + 46..at + 46 + n)?).into_owned();
        let key = raw.replace('\\', "/").trim_start_matches("./").to_ascii_lowercase();
        if !seen.insert(key) {
            return Some(raw);
        }
        at += 46 + n + x + c;
    }
    None
}

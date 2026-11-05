//! A scan of the zip central directory for two members with the same name. The zip reader keeps one entry per
//! name, so a package that lists `visio/pages/page1.xml` twice would show this filter one part and another reader
//! the other. Stop-gap until `bilingua-zip` checks this itself.

use std::collections::HashSet;

// Offsets come from the file, so every sum is checked (usize is 32 bits on wasm32).
fn u16at(b: &[u8], i: usize) -> Option<usize> {
    Some(u16::from_le_bytes(b.get(i..i.checked_add(2)?)?.try_into().ok()?) as usize)
}
fn u32at(b: &[u8], i: usize) -> Option<usize> {
    Some(u32::from_le_bytes(b.get(i..i.checked_add(4)?)?.try_into().ok()?) as usize)
}

/// The first member name (as written) that appears twice in the central directory, comparing names the way the
/// package reader does (case-insensitive, `\` as `/`). `None` when there is none or the directory cannot be walked
/// (the reader then refuses the file on its own).
pub(crate) fn duplicate_name(b: &[u8]) -> Option<String> {
    let last = b.len().checked_sub(22)?;
    let lo = b.len().saturating_sub(22 + 65535);
    let eocd = (lo..=last).rev().find(|i| b[*i..].starts_with(b"PK\x05\x06"))?;
    let count = u16at(b, eocd.checked_add(10)?)?;
    let mut at = u32at(b, eocd.checked_add(16)?)?;
    let mut seen: HashSet<String> = HashSet::new();
    for _ in 0..count {
        if b.get(at..at.checked_add(4)?)? != b"PK\x01\x02" {
            return None;
        }
        let (n, x, c) = (u16at(b, at.checked_add(28)?)?, u16at(b, at.checked_add(30)?)?, u16at(b, at.checked_add(32)?)?);
        let name0 = at.checked_add(46)?;
        let raw = String::from_utf8_lossy(b.get(name0..name0.checked_add(n)?)?).into_owned();
        let key = raw.replace('\\', "/").trim_start_matches("./").to_ascii_lowercase();
        if !seen.insert(key) {
            return Some(raw);
        }
        at = name0.checked_add(n + x + c)?;
    }
    None
}

//! The zip central directory read by hand, before any zip library indexes members by name.

use bilingua_model::{Error, LimitError, LimitKind};
use std::collections::HashSet;

fn refuse(msg: String) -> Error {
    LimitError::new(LimitKind::ZipMalformed, msg).into()
}

/// Refuses a package whose central directory lists one member name twice (names compared without case, with
/// `\` read as `/`, as the reader normalises them). A zip library that indexes by name would show only one of the
/// two copies, so the two readers of one file could disagree about its content.
///
/// A directory that cannot be read here is left to the zip reader, which reports it. Every offset is checked, so a
/// hostile directory cannot overflow `usize` on a 32-bit (WASM) target.
pub fn check_member_names(bytes: &[u8]) -> Result<(), Error> {
    let span = |o: usize, n: usize| o.checked_add(n).and_then(|e| bytes.get(o..e));
    let u16at = |o: usize| span(o, 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize);
    let u32at = |o: usize| span(o, 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let sig = [0x50u8, 0x4b, 5, 6];
    let from = bytes.len().saturating_sub(22 + 65535);
    let Some(eocd) = bytes.get(from..).and_then(|t| t.windows(4).rposition(|w| w == sig)).map(|p| p + from) else {
        return Ok(());
    };
    let (Some(count), Some(mut off)) = (u16at(eocd + 10), u32at(eocd + 16)) else { return Ok(()) };
    let mut seen: HashSet<String> = HashSet::new();
    for _ in 0..count {
        if span(off, 4) != Some(&[0x50, 0x4b, 1, 2][..]) {
            return Ok(());
        }
        let (Some(n), Some(x), Some(c)) = (u16at(off + 28), u16at(off + 30), u16at(off + 32)) else { return Ok(()) };
        let Some(raw) = span(off + 46, n) else { return Ok(()) };
        let name = String::from_utf8_lossy(raw).replace('\\', "/").to_ascii_lowercase();
        if !seen.insert(name.clone()) {
            return Err(refuse(format!("This XLIFF package has duplicate file names inside the archive ({name:?}).")));
        }
        let Some(next) = off.checked_add(46).and_then(|v| v.checked_add(n)).and_then(|v| v.checked_add(x)).and_then(|v| v.checked_add(c)) else {
            return Ok(());
        };
        off = next;
    }
    Ok(())
}

//! The names in the zip's central directory, read straight from the bytes: the zip reader merges two entries of
//! one name, and LocOS refuses an archive that lists the same file twice.

/// True when the central directory lists one name twice. Offsets are checked: on a 32-bit target (wasm) a hostile
/// offset near `u32::MAX` must not overflow.
pub fn has_duplicates(z: &[u8]) -> bool {
    let from = z.len().saturating_sub(22 + 65535);
    let Some(mut i) = z.len().checked_sub(22) else { return false };
    while !z[i..].starts_with(&[0x50, 0x4b, 0x05, 0x06]) {
        if i <= from {
            return false;
        }
        i -= 1;
    }
    let count = u16::from_le_bytes([z[i + 10], z[i + 11]]) as usize;
    let mut at = u32::from_le_bytes([z[i + 16], z[i + 17], z[i + 18], z[i + 19]]) as usize;
    let mut seen: std::collections::HashSet<&[u8]> = std::collections::HashSet::new();
    for _ in 0..count {
        let Some(names_at) = at.checked_add(46) else { return false };
        let Some(h) = z.get(at..names_at) else { return false };
        if !h.starts_with(&[0x50, 0x4b, 0x01, 0x02]) {
            return false;
        }
        let n = u16::from_le_bytes([h[28], h[29]]) as usize;
        let skip = n + u16::from_le_bytes([h[30], h[31]]) as usize + u16::from_le_bytes([h[32], h[33]]) as usize;
        let Some(name) = names_at.checked_add(n).and_then(|e| z.get(names_at..e)) else { return false };
        if !seen.insert(name) {
            return true;
        }
        let Some(next) = names_at.checked_add(skip) else { return false };
        at = next;
    }
    false
}

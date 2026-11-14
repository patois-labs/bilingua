//! Hand-built zip archives for tests: what a safe writer refuses to produce (the same name twice) in the layouts a
//! simple scan of the central directory misreads (a zip64 end record, bytes before the archive). Used by this
//! crate's tests and by the format crates' hostile tests; nothing here is used when reading a file.

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
        }
    }
    !c
}

/// A plain zip of stored members, in order, with the names exactly as given (repeats allowed).
pub fn stored_zip(members: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in members {
        let (crc, size, n) = (crc32(data), data.len() as u32, name.len() as u16);
        let offset = out.len() as u32;
        out.extend([0x50, 0x4b, 3, 4, 20, 0, 0, 0, 0, 0, 0, 0, 0x21, 0x5a]);
        out.extend(crc.to_le_bytes());
        out.extend(size.to_le_bytes());
        out.extend(size.to_le_bytes());
        out.extend(n.to_le_bytes());
        out.extend(0u16.to_le_bytes());
        out.extend(name.as_bytes());
        out.extend(*data);
        central.extend([0x50, 0x4b, 1, 2, 20, 0, 20, 0, 0, 0, 0, 0, 0, 0, 0x21, 0x5a]);
        central.extend(crc.to_le_bytes());
        central.extend(size.to_le_bytes());
        central.extend(size.to_le_bytes());
        central.extend(n.to_le_bytes());
        central.extend([0u8; 12]); // extra, comment, disk, internal and external attributes
        central.extend(offset.to_le_bytes());
        central.extend(name.as_bytes());
    }
    let (cd_off, cd_len, count) = (out.len() as u32, central.len() as u32, members.len() as u16);
    out.extend(central);
    out.extend([0x50, 0x4b, 5, 6, 0, 0, 0, 0]);
    out.extend(count.to_le_bytes());
    out.extend(count.to_le_bytes());
    out.extend(cd_len.to_le_bytes());
    out.extend(cd_off.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out
}

fn u32at(b: &[u8], o: usize) -> u64 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as u64
}

/// `z` (an archive with no comment) with its end record in the zip64 form: a zip64 end record and locator, and the
/// classic record's count 0xFFFF and offset 0xFFFFFFFF.
pub fn as_zip64(z: &[u8]) -> Vec<u8> {
    let eocd = z.len() - 22;
    assert_eq!(&z[eocd..eocd + 4], &[0x50, 0x4b, 5, 6], "an archive without a comment");
    let count = u16::from_le_bytes([z[eocd + 10], z[eocd + 11]]) as u64;
    let (size, off) = (u32at(z, eocd + 12), u32at(z, eocd + 16));
    let mut out = z[..eocd].to_vec();
    let z64 = out.len() as u64;
    out.extend([0x50, 0x4b, 6, 6]);
    out.extend(44u64.to_le_bytes());
    out.extend([45, 0, 45, 0]);
    out.extend(0u32.to_le_bytes());
    out.extend(0u32.to_le_bytes());
    out.extend(count.to_le_bytes());
    out.extend(count.to_le_bytes());
    out.extend(size.to_le_bytes());
    out.extend(off.to_le_bytes());
    out.extend([0x50, 0x4b, 6, 7]);
    out.extend(0u32.to_le_bytes());
    out.extend(z64.to_le_bytes());
    out.extend(1u32.to_le_bytes());
    out.extend([0x50, 0x4b, 5, 6, 0, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0, 0]);
    out
}

/// `z` after 64 bytes of junk that start like a local header (a self-extractor style prefix): the directory offsets
/// no longer point at the directory, and the zip reader finds it anyway.
pub fn with_prefix(z: &[u8]) -> Vec<u8> {
    let mut out = b"PK\x03\x04".to_vec();
    out.extend([0u8; 60]);
    out.extend(z);
    out
}

/// The three layouts of one archive: plain, zip64 end record, bytes before the archive.
pub fn layouts(z: &[u8]) -> [(&'static str, Vec<u8>); 3] {
    [("plain", z.to_vec()), ("zip64", as_zip64(z)), ("prefix", with_prefix(z))]
}

//! Raw bytes in JSON as a base64 string (standard alphabet, padded). Reading also accepts the old form, an
//! array of integers, so skeletons saved before this change still load.

use serde::de::{self, SeqAccess, Visitor};
use serde::{Deserializer, Serializer};
use std::fmt;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub fn encode(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { ALPHABET[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { ALPHABET[n as usize & 63] as char } else { '=' });
    }
    out
}

pub fn decode(s: &str) -> Result<Vec<u8>, String> {
    let b = s.as_bytes();
    if !b.len().is_multiple_of(4) {
        return Err("base64 length is not a multiple of 4".into());
    }
    // A lookup table: a linear search of the alphabet made a 20 MB skeleton take seconds to read.
    const BAD: u8 = 0xFF;
    const TABLE: [u8; 256] = {
        let mut t = [BAD; 256];
        let mut i = 0;
        while i < 64 {
            t[ALPHABET[i] as usize] = i as u8;
            i += 1;
        }
        t
    };
    let val = |c: u8| -> Result<u32, String> {
        match TABLE[c as usize] {
            BAD => Err(format!("bad base64 byte 0x{c:02x}")),
            v => Ok(v as u32),
        }
    };
    let mut out = Vec::with_capacity(b.len() / 4 * 3);
    for (i, q) in b.chunks(4).enumerate() {
        let last = (i + 1) * 4 == b.len();
        let pad = q.iter().rev().take_while(|&&c| c == b'=').count();
        if pad > 2 || (pad > 0 && !last) {
            return Err("misplaced base64 padding".into());
        }
        let mut n = 0u32;
        for &c in &q[..4 - pad] {
            n = (n << 6) | val(c)?;
        }
        n <<= 6 * pad as u32;
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    Ok(out)
}

pub fn serialize<S: Serializer>(data: &[u8], s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&encode(data))
}

pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
    struct V;
    impl<'de> Visitor<'de> for V {
        type Value = Vec<u8>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a base64 string or an array of bytes")
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<Vec<u8>, E> {
            decode(v).map_err(E::custom)
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<u8>, A::Error> {
            let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(0).min(1 << 20));
            while let Some(b) = seq.next_element::<u8>()? {
                out.push(b);
            }
            Ok(out)
        }
    }
    d.deserialize_any(V)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_vectors() {
        for (raw, enc) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foobar", "Zm9vYmFy")] {
            assert_eq!(encode(raw.as_bytes()), enc);
            assert_eq!(decode(enc).unwrap(), raw.as_bytes());
        }
    }

    #[test]
    fn all_bytes_round_trip() {
        let all: Vec<u8> = (0..=255).collect();
        for n in [0, 1, 2, 3, 255, 256] {
            assert_eq!(decode(&encode(&all[..n])).unwrap(), &all[..n]);
        }
    }

    #[test]
    fn bad_input_is_an_error() {
        for bad in ["abc", "ab!d", "=abc", "a===", "Zg==Zg=="] {
            assert!(decode(bad).is_err(), "{bad}");
        }
        // Non-ASCII and control bytes are errors, not panics.
        for bad in ["\u{e9}abc", "ab\u{645}", "ab\n=", "\0\0\0\0"] {
            assert!(decode(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn large_input_round_trips_quickly() {
        let data: Vec<u8> = (0..8_000_000u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 13) as u8).collect();
        let enc = encode(&data);
        let t = std::time::Instant::now();
        let back = decode(&enc).unwrap();
        eprintln!("decode of {} base64 chars: {:?}", enc.len(), t.elapsed());
        assert_eq!(back, data);
    }
}

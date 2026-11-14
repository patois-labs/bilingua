//! Content-Transfer-Encodings: quoted-printable and base64, behaving as CPython's `binascii.a2b_qp`,
//! `binascii.b2a_qp` (as `quopri.encodestring` calls it) and `binascii.a2b_base64` / `base64.encodebytes`, so a
//! re-encoded part is the same bytes LocOS writes.

const MAXLINESIZE: usize = 76;

fn hexval(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// `binascii.a2b_qp(raw)`: `=XX` escapes, soft line breaks, a broken `=` kept as it is.
pub fn qp_decode(data: &[u8]) -> Vec<u8> {
    let n = data.len();
    let mut out = Vec::with_capacity(n);
    let mut i = 0;
    while i < n {
        if data[i] != b'=' {
            out.push(data[i]);
            i += 1;
            continue;
        }
        i += 1;
        if i >= n {
            break;
        }
        if data[i] == b'\n' || data[i] == b'\r' {
            if data[i] != b'\n' {
                while i < n && data[i] != b'\n' {
                    i += 1;
                }
            }
            if i < n {
                i += 1;
            }
        } else if data[i] == b'=' {
            out.push(b'=');
            i += 1;
        } else if i + 1 < n && hexval(data[i]).is_some() && hexval(data[i + 1]).is_some() {
            out.push(hexval(data[i]).unwrap() << 4 | hexval(data[i + 1]).unwrap());
            i += 2;
        } else {
            out.push(b'=');
        }
    }
    out
}

fn push_hex(out: &mut Vec<u8>, b: u8) {
    const H: &[u8; 16] = b"0123456789ABCDEF";
    out.push(H[(b >> 4) as usize]);
    out.push(H[(b & 15) as usize]);
}

/// `binascii.b2a_qp(data, quotetabs=False, istext=True, header=False)`.
pub fn qp_encode(data: &[u8]) -> Vec<u8> {
    let n = data.len();
    let crlf = match data.iter().position(|c| *c == b'\n') {
        Some(p) => p > 0 && data[p - 1] == b'\r',
        None => false,
    };
    let mut out: Vec<u8> = Vec::with_capacity(n + n / 8 + 8);
    let (mut i, mut linelen) = (0usize, 0usize);
    let soft = |out: &mut Vec<u8>| {
        out.push(b'=');
        if crlf {
            out.push(b'\r');
        }
        out.push(b'\n');
    };
    while i < n {
        let c = data[i];
        let must = c > 126
            || c == b'='
            || (c == b'.' && linelen == 0 && (i + 1 == n || data[i + 1] == b'\n' || data[i + 1] == b'\r' || data[i + 1] == 0))
            || ((c == b'\t' || c == b' ') && i + 1 == n)
            || (c < 33 && c != b'\r' && c != b'\n' && c != b'\t' && c != b' ');
        if must {
            if linelen + 3 >= MAXLINESIZE {
                soft(&mut out);
                linelen = 0;
            }
            out.push(b'=');
            push_hex(&mut out, c);
            i += 1;
            linelen += 3;
        } else if c == b'\n' || (i + 1 < n && c == b'\r' && data[i + 1] == b'\n') {
            linelen = 0;
            if let Some(&last) = out.last() {
                if last == b' ' || last == b'\t' {
                    let l = out.len();
                    out[l - 1] = b'=';
                    push_hex(&mut out, last);
                }
            }
            if crlf {
                out.push(b'\r');
            }
            out.push(b'\n');
            i += if c == b'\r' { 2 } else { 1 };
        } else {
            if i + 1 != n && data[i + 1] != b'\n' && linelen + 1 >= MAXLINESIZE {
                soft(&mut out);
                linelen = 0;
            }
            out.push(c);
            i += 1;
            linelen += 1;
        }
    }
    out
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn b64val(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// `binascii.a2b_base64(raw)` (not strict): characters outside the alphabet are skipped, padding ends the data,
/// and a truncated last group is an error.
pub fn b64_decode(data: &[u8]) -> Result<Vec<u8>, ()> {
    let mut out = Vec::with_capacity(data.len() / 4 * 3 + 3);
    let (mut quad, mut left, mut pads) = (0u8, 0u32, 0u32);
    for &c in data {
        if c == b'=' {
            if quad >= 2 {
                pads += 1;
                if quad as u32 + pads >= 4 {
                    return Ok(out);
                }
            }
            continue;
        }
        let Some(v) = b64val(c) else { continue };
        pads = 0;
        match quad {
            0 => {
                quad = 1;
                left = v as u32;
            }
            1 => {
                quad = 2;
                out.push(((left << 2) | (v as u32 >> 4)) as u8);
                left = (v & 0x0f) as u32;
            }
            2 => {
                quad = 3;
                out.push(((left << 4) | (v as u32 >> 2)) as u8);
                left = (v & 0x03) as u32;
            }
            _ => {
                quad = 0;
                out.push(((left << 6) | v as u32) as u8);
                left = 0;
            }
        }
    }
    if quad != 0 {
        return Err(());
    }
    Ok(out)
}

/// Plain base64 without line breaks.
pub fn b64_encode(data: &[u8]) -> String {
    let mut s = String::with_capacity(data.len().div_ceil(3) * 4);
    for ch in data.chunks(3) {
        let b = [ch[0], *ch.get(1).unwrap_or(&0), *ch.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        s.push(B64[(n >> 18) as usize & 63] as char);
        s.push(B64[(n >> 12) as usize & 63] as char);
        s.push(if ch.len() > 1 { B64[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if ch.len() > 2 { B64[n as usize & 63] as char } else { '=' });
    }
    s
}

/// `base64.encodebytes`: 76 characters per line, every line (the last too) ends with `\n`.
pub fn b64_encode_lines(data: &[u8]) -> Vec<u8> {
    let flat = b64_encode(data);
    let mut out = Vec::with_capacity(flat.len() + flat.len() / 76 + 2);
    for line in flat.as_bytes().chunks(76) {
        out.extend_from_slice(line);
        out.push(b'\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qp_round_trips_and_wraps() {
        assert_eq!(qp_decode(b"caf=E9 =\r\nbar =3D =ZZ="), b"caf\xe9 bar = =ZZ");
        assert_eq!(qp_encode(b"caf\xc3\xa9 = x\n"), b"caf=C3=A9 =3D x\n");
        let long = vec![b'a'; 200];
        let e = qp_encode(&long);
        assert!(e.split(|c| *c == b'\n').all(|l| l.len() <= 76));
        assert_eq!(qp_decode(&e), long);
        assert_eq!(qp_encode(b"end \n.x\n.\n"), b"end=20\n.x\n=2E\n");
        assert_eq!(qp_encode(b"tail "), b"tail=20");
    }

    #[test]
    fn base64_matches_python() {
        assert_eq!(b64_decode(b"aGVs bG8=\r\n").unwrap(), b"hello");
        assert!(b64_decode(b"aGVsb").is_err());
        assert!(b64_decode(b"aGVsbG8").is_err());
        assert_eq!(b64_decode(b"aGVsbG8=junk").unwrap(), b"hello");
        assert_eq!(b64_encode_lines(&[7u8; 100]).iter().filter(|c| **c == b'\n').count(), 2);
        assert!(b64_encode_lines(b"").is_empty());
        for n in 0..9 {
            let d: Vec<u8> = (0..n as u8).collect();
            assert_eq!(b64_decode(b64_encode(&d).as_bytes()).unwrap(), d);
        }
    }
}

//! The resource-script lexer: strings, numbers, identifiers and single characters. Comments, white space and
//! preprocessor lines are skipped (they stay in the skeleton). Linear in the input; no recursion.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Str,
    Num,
    Id,
    /// Any other single character.
    P,
}

#[derive(Clone, Copy, Debug)]
pub struct Tok {
    pub kind: Kind,
    pub start: usize,
    pub end: usize,
}

fn is_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n' | 0x0c | 0x0b)
}

fn char_len(b: u8) -> usize {
    match b {
        0..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xff => 4,
        _ => 1,
    }
}

/// End of a preprocessor line starting at `pos` (a `#`), with backslash continuations.
fn preproc_end(b: &[u8], pos: usize) -> usize {
    let n = b.len();
    let mut i = pos;
    while i < n {
        if b[i] == b'\\' {
            if i + 1 < n && b[i + 1] == b'\n' {
                i += 2;
                continue;
            }
            if i + 2 < n && b[i + 1] == b'\r' && b[i + 2] == b'\n' {
                i += 3;
                continue;
            }
        }
        if b[i] == b'\n' {
            break;
        }
        i += 1;
    }
    i
}

/// A string literal whose opening quote is at `q`: its end, or `None` when it is not terminated on its line.
fn string_end(b: &[u8], q: usize) -> Option<usize> {
    let n = b.len();
    let mut i = q + 1;
    while i < n {
        match b[i] {
            b'"' => {
                if i + 1 < n && b[i + 1] == b'"' {
                    i += 2;
                } else {
                    return Some(i + 1);
                }
            }
            b'\\' => {
                if i + 1 >= n {
                    return None;
                }
                if b[i + 1] == b'\r' && i + 2 < n && b[i + 2] == b'\n' {
                    i += 3;
                } else {
                    i += 2;
                }
            }
            b'\n' => return None,
            _ => i += 1,
        }
    }
    None
}

fn find_nl(b: &[u8], from: usize) -> usize {
    b[from..].iter().position(|&c| c == b'\n').map_or(b.len(), |p| from + p)
}

pub fn lex(text: &str) -> Vec<Tok> {
    let b = text.as_bytes();
    let n = b.len();
    let mut toks: Vec<Tok> = Vec::with_capacity(n / 6 + 4);
    let mut pos = 0usize;
    let mut line_start = true;
    while pos < n {
        let c = b[pos];
        if c == b'#' && line_start {
            pos = preproc_end(b, pos);
            continue;
        }
        if is_ws(c) {
            let mut e = pos;
            let mut last_nl = None;
            while e < n && is_ws(b[e]) {
                if b[e] == b'\n' {
                    last_nl = Some(e);
                }
                e += 1;
            }
            if let Some(nl) = last_nl {
                line_start = b[nl + 1..e].iter().all(|&x| x == b' ' || x == b'\t');
            }
            pos = e;
            continue;
        }
        line_start = false;
        if c == b'/' && pos + 1 < n && b[pos + 1] == b'/' {
            pos = find_nl(b, pos);
            continue;
        }
        if c == b'/' && pos + 1 < n && b[pos + 1] == b'*' {
            pos = match b[pos + 2..].windows(2).position(|w| w == b"*/") {
                Some(p) => pos + 2 + p + 2,
                None => n,
            };
            continue;
        }
        if c == b'L' && pos + 1 < n && b[pos + 1] == b'"' {
            if let Some(e) = string_end(b, pos + 1) {
                toks.push(Tok { kind: Kind::Str, start: pos, end: e });
                pos = e;
                continue;
            }
        }
        if c == b'"' {
            match string_end(b, pos) {
                Some(e) => {
                    toks.push(Tok { kind: Kind::Str, start: pos, end: e });
                    pos = e;
                }
                // an unterminated string: skip the rest of the line (rc.exe refuses it)
                None => pos = find_nl(b, pos),
            }
            continue;
        }
        if c.is_ascii_digit() {
            let mut e = pos;
            if c == b'0' && pos + 2 < n && (b[pos + 1] == b'x' || b[pos + 1] == b'X') && b[pos + 2].is_ascii_hexdigit() {
                e = pos + 2;
                while e < n && b[e].is_ascii_hexdigit() {
                    e += 1;
                }
            } else {
                while e < n && b[e].is_ascii_digit() {
                    e += 1;
                }
            }
            while e < n && matches!(b[e], b'l' | b'L' | b'u' | b'U') {
                e += 1;
            }
            toks.push(Tok { kind: Kind::Num, start: pos, end: e });
            pos = e;
            continue;
        }
        if c.is_ascii_alphabetic() || c == b'_' {
            let mut e = pos + 1;
            while e < n && (b[e].is_ascii_alphanumeric() || b[e] == b'_') {
                e += 1;
            }
            toks.push(Tok { kind: Kind::Id, start: pos, end: e });
            pos = e;
            continue;
        }
        let e = (pos + char_len(c)).min(n);
        toks.push(Tok { kind: Kind::P, start: pos, end: e });
        pos = e;
    }
    toks
}

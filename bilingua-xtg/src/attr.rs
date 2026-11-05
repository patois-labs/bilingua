//! Character attribute tags as operations (LocOS `_attr_ops`), for the pairing of switch-on and reset tags.

pub type Key = char;
/// The double strikethrough toggle `//`.
pub const DSLASH: Key = '\u{2}';

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OpKind {
    /// `$`: everything back to the style sheet.
    ResetAll,
    /// `P`: plain (every type style off).
    Plain,
    /// `B$`, `f$`, `<@$>`: one attribute back to the style sheet.
    ResetOne,
    /// `B`: a toggle.
    Toggle,
    /// `f"Arial"`, `z12`, `<@Name>`: a value.
    Set,
}

#[derive(Clone, Copy, Debug)]
pub struct Op {
    pub key: Key,
    pub kind: OpKind,
}

pub const TOGGLE_MASK: u32 = (1 << 13) - 1;

pub fn bit(k: Key) -> u32 {
    1 << match k {
        'B' => 0,
        'I' => 1,
        'O' => 2,
        'S' => 3,
        'U' => 4,
        'W' => 5,
        'R' => 6,
        'K' => 7,
        'V' => 8,
        '+' => 9,
        '-' => 10,
        '/' => 11,
        DSLASH => 12,
        'f' => 13,
        'z' => 14,
        'c' => 15,
        's' => 16,
        'h' => 17,
        'y' => 18,
        'k' => 19,
        't' => 20,
        'b' => 21,
        'o' => 22,
        _ => 23, // '@'
    }
}

pub fn is_toggle(k: Key) -> bool {
    bit(k) & TOGGLE_MASK != 0 && k != '@'
}

const VALUE_KEYS: &[u8] = b"fzcshyktbo";

/// The length of a value at `b[i..]`: a quoted string, `$`, or a number.
fn value_len(b: &[u8], i: usize) -> Option<usize> {
    let n = b.len();
    let c = *b.get(i)?;
    if c == b'"' {
        let mut j = i + 1;
        while j < n {
            match b[j] {
                b'\\' => {
                    if j + 1 >= n {
                        break;
                    }
                    j += 1 + crate::lex::utf8_len(b[j + 1]);
                }
                b'"' => return Some(j + 1 - i),
                _ => j += 1,
            }
        }
    }
    if c == b'$' {
        return Some(1);
    }
    let digits = |mut j: usize| {
        let s = j;
        while j < n && b[j].is_ascii_digit() {
            j += 1;
        }
        (j, j > s)
    };
    let mut j = i;
    if b[j] == b'-' {
        j += 1;
    }
    let (e, ok) = digits(j);
    if ok {
        let mut e = e;
        if e < n && b[e] == b'.' {
            let (e2, ok2) = digits(e + 1);
            if ok2 {
                e = e2;
            }
        }
        return Some(e - i);
    }
    if j < n && b[j] == b'.' {
        let (e2, ok2) = digits(j + 1);
        if ok2 {
            return Some(e2 - i);
        }
    }
    None
}

fn style_name_ok(s: &str) -> bool {
    let mut it = s.chars();
    match it.next() {
        Some(c) if !matches!(c, '"' | '<' | '>' | '$' | '\\') => {}
        _ => return false,
    }
    let mut n = 0;
    for c in it {
        if matches!(c, '"' | '<' | '>' | '\\') {
            return false;
        }
        n += 1;
        if n > 255 {
            return false;
        }
    }
    true
}

/// A character attribute tag (`raw` includes the angle brackets) as operations, or `None` for any other tag.
pub fn attr_ops(raw: &str) -> Option<Vec<Op>> {
    let body = raw.get(1..raw.len().checked_sub(1)?)?;
    let b = body.as_bytes();
    if b.is_empty() {
        return None;
    }
    if b[0] == b'@' {
        let rest = &body[1..];
        if rest == "$" || rest == "$p" {
            return Some(vec![Op { key: '@', kind: OpKind::ResetOne }]);
        }
        return style_name_ok(rest).then(|| vec![Op { key: '@', kind: OpKind::Set }]);
    }
    let n = b.len();
    let mut ops = Vec::new();
    let mut i = 0;
    while i < n {
        let ch = b[i];
        if ch == b'$' {
            ops.push(Op { key: '$', kind: OpKind::ResetAll });
            i += 1;
            continue;
        }
        if ch == b'P' {
            ops.push(Op { key: 'P', kind: OpKind::Plain });
            i += 1;
            continue;
        }
        if VALUE_KEYS.contains(&ch) {
            let l = value_len(b, i + 1)?;
            let kind = if &b[i + 1..i + 1 + l] == b"$" { OpKind::ResetOne } else { OpKind::Set };
            ops.push(Op { key: ch as char, kind });
            i += 1 + l;
            continue;
        }
        let (key, l) = if b[i..].starts_with(b"//") { (DSLASH, 2) } else { (ch as char, 1) };
        if ch >= 0x80 || !is_toggle(key) {
            return None;
        }
        i += l;
        if i < n && b[i] == b'$' {
            ops.push(Op { key, kind: OpKind::ResetOne });
            i += 1;
        } else {
            ops.push(Op { key, kind: OpKind::Toggle });
        }
    }
    Some(ops)
}

/// The tags that put `keys` back to the style sheet (last set, first reset).
pub fn reset_tags(keys: &[Key]) -> String {
    let mut s = String::new();
    for &k in keys.iter().rev() {
        s.push('<');
        if k == DSLASH {
            s.push_str("//");
        } else {
            s.push(k);
        }
        s.push_str("$>");
    }
    s
}

/// A short name for the tag chip.
pub fn label(raw: &str) -> &'static str {
    let Some(ops) = attr_ops(raw) else {
        let b = raw.as_bytes();
        return match b.get(1) {
            Some(b'\\') => "special character",
            Some(b'A') => "link",
            Some(b'*') => "paragraph format",
            Some(b'X') => "index",
            _ => "tag",
        };
    };
    if ops.len() != 1 {
        return "format";
    }
    let o = ops[0];
    match (o.key, o.kind) {
        ('$', _) => "reset",
        ('P', _) => "plain",
        ('@', _) => "style sheet",
        ('B', _) => "bold",
        ('I', _) => "italic",
        ('U', _) => "underline",
        ('W', _) => "word underline",
        ('O', _) => "outline",
        ('S', _) => "shadow",
        ('R', _) => "all caps",
        ('K', _) => "small caps",
        ('+', _) => "superscript",
        ('-', _) => "subscript",
        ('V', _) => "superior",
        ('/', _) | (DSLASH, _) => "strikethrough",
        ('f', _) => "font",
        ('z', _) => "size",
        ('c', _) => "colour",
        _ => "format",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(raw: &str) -> Option<Vec<(Key, OpKind)>> {
        attr_ops(raw).map(|v| v.into_iter().map(|o| (o.key, o.kind)).collect())
    }

    #[test]
    fn ops() {
        assert_eq!(keys("<B>"), Some(vec![('B', OpKind::Toggle)]));
        assert_eq!(keys("<B$>"), Some(vec![('B', OpKind::ResetOne)]));
        assert_eq!(keys("<f\"Arial\"z12>"), Some(vec![('f', OpKind::Set), ('z', OpKind::Set)]));
        assert_eq!(keys("<f$>"), Some(vec![('f', OpKind::ResetOne)]));
        assert_eq!(keys("<$>"), Some(vec![('$', OpKind::ResetAll)]));
        assert_eq!(keys("<//>"), Some(vec![(DSLASH, OpKind::Toggle)]));
        assert_eq!(keys("<@Emph>"), Some(vec![('@', OpKind::Set)]));
        assert_eq!(keys("<@$>"), Some(vec![('@', OpKind::ResetOne)]));
        assert!(keys("<\\n>").is_none());
        assert!(keys("<*L>").is_none());
        assert!(keys("<z>").is_none());
        assert_eq!(reset_tags(&['B', 'f']), "<f$><B$>");
    }
}

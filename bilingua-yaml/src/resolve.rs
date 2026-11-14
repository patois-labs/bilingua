//! The YAML 1.1 implicit resolver (the one PyYAML, Psych and Symfony use), reduced to one question: does a plain
//! scalar resolve to a string? `yes`, `off`, numbers, dates, `null`, `~`, `<<` and `=` do not.

fn is_bool(s: &str) -> bool {
    matches!(
        s,
        "yes" | "Yes" | "YES" | "no" | "No" | "NO" | "true" | "True" | "TRUE" | "false" | "False" | "FALSE" | "on" | "On" | "ON" | "off" | "Off" | "OFF"
    )
}

fn is_null(s: &str) -> bool {
    matches!(s, "" | "~" | "null" | "Null" | "NULL")
}

/// Cursor over the bytes of one candidate scalar.
struct C<'a> {
    b: &'a [u8],
    i: usize,
}

impl<'a> C<'a> {
    fn new(s: &'a str) -> C<'a> {
        C { b: s.as_bytes(), i: 0 }
    }
    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }
    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn eat_set(&mut self, set: &[u8]) -> bool {
        match self.peek() {
            Some(c) if set.contains(&c) => {
                self.i += 1;
                true
            }
            _ => false,
        }
    }
    fn run(&mut self, f: impl Fn(u8) -> bool) -> usize {
        let s = self.i;
        while self.peek().is_some_and(&f) {
            self.i += 1;
        }
        self.i - s
    }
    fn end(&self) -> bool {
        self.i == self.b.len()
    }
    fn starts(&mut self, lit: &str) -> bool {
        if self.b[self.i..].starts_with(lit.as_bytes()) {
            self.i += lit.len();
            true
        } else {
            false
        }
    }
}

fn digit(c: u8) -> bool {
    c.is_ascii_digit()
}

/// `[0-5]?[0-9]` groups after a colon, one or more: `:30`, `:5:09`.
fn sexagesimal_tail(c: &mut C) -> bool {
    let mut n = 0;
    loop {
        let save = c.i;
        if !c.eat(b':') {
            break;
        }
        let ok = if c.peek().is_some_and(|x| (b'0'..=b'5').contains(&x)) && c.b.get(c.i + 1).is_some_and(|x| digit(*x)) {
            c.i += 2;
            true
        } else if c.peek().is_some_and(digit) {
            c.i += 1;
            true
        } else {
            false
        };
        if !ok {
            c.i = save;
            break;
        }
        n += 1;
    }
    n > 0
}

fn is_int(s: &str) -> bool {
    let mut c = C::new(s);
    c.eat_set(b"+-");
    let rest = c.i;
    // 0b101_0
    if c.starts("0b") && c.run(|x| matches!(x, b'0' | b'1' | b'_')) > 0 && c.end() {
        return true;
    }
    c.i = rest;
    // 0755
    if c.eat(b'0') && c.run(|x| matches!(x, b'0'..=b'7' | b'_')) > 0 && c.end() {
        return true;
    }
    c.i = rest;
    // 0 or 12_000
    if c.eat(b'0') && c.end() {
        return true;
    }
    c.i = rest;
    if c.peek().is_some_and(|x| (b'1'..=b'9').contains(&x)) {
        c.i += 1;
        c.run(|x| digit(x) || x == b'_');
        if c.end() {
            return true;
        }
        // 190:20:30
        if sexagesimal_tail(&mut c) && c.end() {
            return true;
        }
    }
    c.i = rest;
    // 0xFF
    c.starts("0x") && c.run(|x| x.is_ascii_hexdigit() || x == b'_') > 0 && c.end()
}

fn exponent(c: &mut C) -> bool {
    let save = c.i;
    if c.eat_set(b"eE") && c.eat_set(b"+-") && c.run(digit) > 0 {
        return true;
    }
    c.i = save;
    false
}

fn is_float(s: &str) -> bool {
    let mut c = C::new(s);
    // [-+]?[0-9][0-9_]*\.[0-9_]*([eE][-+][0-9]+)?
    c.eat_set(b"+-");
    let rest = c.i;
    if c.peek().is_some_and(digit) {
        c.i += 1;
        c.run(|x| digit(x) || x == b'_');
        let after_int = c.i;
        if c.eat(b'.') {
            c.run(|x| digit(x) || x == b'_');
            exponent(&mut c);
            if c.end() {
                return true;
            }
        }
        // [-+]?[0-9][0-9_]*(:[0-5]?[0-9])+\.[0-9_]*
        c.i = after_int;
        if sexagesimal_tail(&mut c) && c.eat(b'.') {
            c.run(|x| digit(x) || x == b'_');
            if c.end() {
                return true;
            }
        }
    }
    // [-+]?\.(inf|Inf|INF)
    c.i = rest;
    if c.eat(b'.') && (c.starts("inf") || c.starts("Inf") || c.starts("INF")) && c.end() {
        return true;
    }
    // \.[0-9][0-9_]*([eE][-+][0-9]+)?  (no sign)
    let mut d = C::new(s);
    if d.eat(b'.') && d.peek().is_some_and(digit) {
        d.i += 1;
        d.run(|x| digit(x) || x == b'_');
        exponent(&mut d);
        if d.end() {
            return true;
        }
    }
    // \.(nan|NaN|NAN)
    let mut d = C::new(s);
    d.eat(b'.') && (d.starts("nan") || d.starts("NaN") || d.starts("NAN")) && d.end()
}

fn digits(c: &mut C, n: usize) -> bool {
    for _ in 0..n {
        if !c.peek().is_some_and(digit) {
            return false;
        }
        c.i += 1;
    }
    true
}

fn is_timestamp(s: &str) -> bool {
    let mut c = C::new(s);
    if !(digits(&mut c, 4) && c.eat(b'-')) {
        return false;
    }
    let after_year = c.i;
    // YYYY-MM-DD
    if digits(&mut c, 2) && c.eat(b'-') && digits(&mut c, 2) && c.end() {
        return true;
    }
    c.i = after_year;
    // YYYY-M(M)-D(D) (T| +) H(H):MM:SS(.f*) (zone)?
    let one_or_two = |c: &mut C| {
        if !c.peek().is_some_and(digit) {
            return false;
        }
        c.i += 1;
        if c.peek().is_some_and(digit) {
            c.i += 1;
        }
        true
    };
    if !(one_or_two(&mut c) && c.eat(b'-') && one_or_two(&mut c)) {
        return false;
    }
    if !c.eat_set(b"Tt") && c.run(|x| x == b' ' || x == b'\t') == 0 {
        return false;
    }
    if !(one_or_two(&mut c) && c.eat(b':') && digits(&mut c, 2) && c.eat(b':') && digits(&mut c, 2)) {
        return false;
    }
    if c.eat(b'.') {
        c.run(digit);
    }
    if c.end() {
        return true;
    }
    c.run(|x| x == b' ' || x == b'\t');
    if c.eat(b'Z') {
        return c.end();
    }
    if c.eat_set(b"+-") && one_or_two(&mut c) {
        if c.end() {
            return true;
        }
        return c.eat(b':') && digits(&mut c, 2) && c.end();
    }
    false
}

/// True when a plain (unquoted, untagged) scalar with this text is a string under the YAML 1.1 resolver.
pub fn resolves_str(s: &str) -> bool {
    let Some(&first) = s.as_bytes().first() else { return false }; // empty is null
    if is_bool(s) || is_null(s) {
        return false;
    }
    match first {
        b'<' => s != "<<",
        b'=' => s != "=",
        b'-' | b'+' | b'0'..=b'9' | b'.' => !(is_int(s) || is_float(s) || (first.is_ascii_digit() && is_timestamp(s))),
        b'!' | b'&' | b'*' => false,
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::resolves_str as r;

    #[test]
    fn strings_and_non_strings() {
        for s in ["Hello", "1.0.0", "12abc", "0x", "1:60", "yess", "#ff0000", "v1", "2026-10", "10-10-2026", "1_000a", "-", "+", ".", "..", "e5", "1e5", "0b2"] {
            assert!(r(s), "{s} should be a string");
        }
        for s in [
            "yes", "No", "ON", "off", "true", "~", "null", "", "12", "-12", "+7", "0", "0755", "0b101", "0xFF", "1_000", "190:20:30", "1.5", "-1.5e+3", ".5", ".inf", "-.INF", ".NaN", "1.", "2026-10-10", "2001-1-1 1:02:03", "2001-12-14t21:59:43.10-05:00",
            "2001-12-14 21:59:43.10 -5", "<<", "=",
        ] {
            assert!(!r(s), "{s} should not be a string");
        }
    }
}

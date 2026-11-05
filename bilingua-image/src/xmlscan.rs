//! A lenient XML scan that finds the regions where markup characters are not markup: comments, CDATA sections,
//! processing instructions, the DOCTYPE and quoted attribute values.
//!
//! The LocOS tokenizer ends a tag at the first `>`, so a `>` inside one of those regions cuts the region in two and
//! the second half looks like text. Such a text node is not text for any XML parser; writing a translation over it
//! would break the file. [`Regions::overlaps`] lets the unit finder leave those nodes alone.

pub struct Regions(Vec<(usize, usize)>);

fn find(b: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if from >= b.len() {
        return None;
    }
    b[from..].windows(needle.len()).position(|w| w == needle).map(|p| p + from)
}

impl Regions {
    pub fn scan(text: &str) -> Regions {
        let b = text.as_bytes();
        let mut out = Vec::new();
        let mut i = 0usize;
        while let Some(p) = b[i..].iter().position(|&c| c == b'<').map(|p| p + i) {
            let rest = &b[p..];
            // Only a region that ends is a region; an unterminated one is damage, not markup, and is not guessed at.
            if rest.starts_with(b"<!--") {
                match find(b, p + 4, b"-->") {
                    Some(e) => {
                        out.push((p, e + 3));
                        i = e + 3;
                    }
                    None => i = p + 1,
                }
            } else if rest.starts_with(b"<![CDATA[") {
                match find(b, p + 9, b"]]>") {
                    Some(e) => {
                        out.push((p, e + 3));
                        i = e + 3;
                    }
                    None => i = p + 1,
                }
            } else if rest.starts_with(b"<?") {
                match find(b, p + 2, b"?>") {
                    Some(e) => {
                        out.push((p, e + 2));
                        i = e + 2;
                    }
                    None => i = p + 1,
                }
            } else if rest.len() > 9 && rest[..9].eq_ignore_ascii_case(b"<!DOCTYPE") {
                let mut j = p + 9;
                let mut depth = 0usize;
                while j < b.len() {
                    match b[j] {
                        b'[' => depth += 1,
                        b']' => depth = depth.saturating_sub(1),
                        q @ (b'"' | b'\'') => {
                            // a quote that would swallow a `<` is a stray quote, not a literal
                            if let Some(k) = b[j + 1..].iter().position(|&c| c == q) {
                                if !b[j + 1..j + 1 + k].contains(&b'<') {
                                    j += 1 + k;
                                }
                            }
                        }
                        b'>' if depth == 0 => break,
                        _ => {}
                    }
                    j += 1;
                }
                if j < b.len() {
                    out.push((p, j + 1));
                    i = j + 1;
                } else {
                    i = p + 1; // a DOCTYPE that never ends is damage, not markup
                }
            } else if rest.len() > 1 && (rest[1].is_ascii_alphabetic() || rest[1] == b'_' || rest[1] == b':' || (rest[1] == b'/' && rest.len() > 2)) {
                let mut j = p + 1;
                while j < b.len() {
                    match b[j] {
                        q @ (b'"' | b'\'') => {
                            // XML forbids `<` in an attribute value: a quote that would swallow one is a stray quote
                            if let Some(e) = b[j + 1..].iter().position(|&c| c == q).map(|k| j + 1 + k + 1) {
                                if !b[j + 1..e].contains(&b'<') {
                                    out.push((j, e));
                                    j = e;
                                    continue;
                                }
                            }
                        }
                        b'>' => break,
                        _ => {}
                    }
                    j += 1;
                }
                i = (j + 1).min(b.len());
            } else {
                i = p + 1;
            }
        }
        Regions(out)
    }

    /// True when `[s, e)` shares a byte with a region.
    pub fn overlaps(&self, s: usize, e: usize) -> bool {
        let k = self.0.partition_point(|r| r.1 <= s);
        self.0.get(k).is_some_and(|r| r.0 < e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions() {
        let t = r#"<a t="x>y">z</a><!-- p > q --><![CDATA[ > ]]>"#;
        let r = Regions::scan(t);
        assert!(r.overlaps(t.find("y\"").unwrap(), t.find("y\"").unwrap() + 2));
        assert!(!r.overlaps(t.find('z').unwrap(), t.find('z').unwrap() + 1));
        assert!(r.overlaps(t.find("q -->").unwrap(), t.find("q -->").unwrap() + 5));
    }
}

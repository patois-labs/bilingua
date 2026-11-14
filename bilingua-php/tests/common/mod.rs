#![allow(dead_code)]
use bilingua_php::Php;
use bilingua_model::*;

pub fn opts() -> ExtractOptions {
    ExtractOptions::default()
}

pub fn extract(data: &[u8]) -> Result<Document, Error> {
    Php.extract(data, &opts())
}

pub fn extract_str(src: &str) -> Result<Document, Error> {
    extract(src.as_bytes())
}

/// The unit text the way LocOS shows it: one token per inline code (`x` point, `p` free point).
pub fn locos_text(u: &Unit) -> String {
    bilingua_model::profile::tokens::to_locos_text(&u.source()).0
}

/// The unit texts of a file (translatable units only, as LocOS lists them).
pub fn texts(data: &[u8]) -> Vec<String> {
    extract(data)
        .expect("extract")
        .units
        .iter()
        .filter(|u| u.translate)
        .map(locos_text)
        .collect()
}

pub fn texts_str(src: &str) -> Vec<String> {
    texts(src.as_bytes())
}

/// What the LocOS adapter does with a translated text: a point token (`x` or `p`) names the source code with that
/// number, a token that is not in the source or is a paired token (`g`) is dropped, the rest is text.
pub fn target_from_locos(u: &Unit, text: &str) -> Vec<Inline> {
    let src = u.source();
    let mut out: Vec<Inline> = Vec::new();
    let mut lit = String::new();
    let mut rest = text;
    while let Some(i) = rest.find('\u{27E6}') {
        lit.push_str(&rest[..i]);
        let after = &rest[i + '\u{27E6}'.len_utf8()..];
        let Some(j) = after.find('\u{27E7}') else {
            lit.push('\u{27E6}');
            rest = after;
            continue;
        };
        let inner = &after[..j];
        let body = inner.strip_prefix('/').unwrap_or(inner);
        let well = !body.is_empty()
            && body
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            && body
                .trim_start_matches(|c: char| c.is_ascii_lowercase())
                .chars()
                .all(|c| c.is_ascii_digit())
            && body.chars().any(|c| c.is_ascii_digit());
        if well {
            if matches!(inner.chars().next(), Some('x' | 'p')) {
                let n = &inner[1..];
                let mut tags = src.iter().filter(|i| matches!(i, Inline::Tag { .. }));
                if let Some(t) = n
                    .parse::<usize>()
                    .ok()
                    .and_then(|n| tags.nth(n.wrapping_sub(1)))
                {
                    if !lit.is_empty() {
                        out.push(Inline::Text(std::mem::take(&mut lit)));
                    }
                    out.push(t.clone());
                }
            }
            // unknown or paired tokens are dropped
        } else {
            lit.push_str(&rest[i..i + '\u{27E6}'.len_utf8() + j + '\u{27E7}'.len_utf8()]);
        }
        rest = &after[j + '\u{27E7}'.len_utf8()..];
    }
    lit.push_str(rest);
    if !lit.is_empty() {
        out.push(Inline::Text(lit));
    }
    out
}

/// Sets targets by translatable-unit index (as the LocOS mapping does) from LocOS-style text and merges.
pub fn build(data: &[u8], mapping: &[(usize, &str)]) -> Vec<u8> {
    let mut doc = extract(data).expect("extract");
    let ids: Vec<usize> = doc
        .units
        .iter()
        .enumerate()
        .filter(|(_, u)| u.translate)
        .map(|(i, _)| i)
        .collect();
    for (k, t) in mapping {
        let ui = ids[*k];
        let id = doc.units[ui].segments[0].id.clone();
        let inl = target_from_locos(&doc.units[ui], t);
        doc.set_target(&id, inl).expect("set_target");
    }
    Php.merge(&doc).expect("merge")
}

pub fn build_str(src: &str, mapping: &[(usize, &str)]) -> String {
    String::from_utf8(build(src.as_bytes(), mapping)).unwrap()
}

/// Untouched merge and an echo merge (every unit's own text as target) both give back the original bytes.
pub fn assert_identity(data: &[u8]) {
    let doc = extract(data).unwrap();
    assert_eq!(Php.merge(&doc).unwrap(), data, "untouched merge");
    let t = texts(data);
    let echo: Vec<(usize, &str)> = t.iter().enumerate().map(|(i, s)| (i, s.as_str())).collect();
    assert_eq!(build(data, &echo), data, "echo merge");
}

pub fn refused(src: &str, needle: &str) -> String {
    match extract_str(src) {
        Err(Error::Parse { msg, .. }) | Err(Error::Unsupported(msg)) | Err(Error::Encoding(msg)) | Err(Error::Limit(LimitError { msg, .. })) => {
            assert!(
                msg.contains(needle),
                "{needle:?} not in {msg:?} for {src:?}"
            );
            msg
        }
        Err(e) => panic!("wrong error kind {e:?}"),
        Ok(_) => panic!("not refused: {src:?}"),
    }
}

pub fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

pub fn corpus_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/php")
}

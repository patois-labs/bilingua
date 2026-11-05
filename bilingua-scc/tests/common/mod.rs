#![allow(dead_code)]
use bilingua_model::*;
use bilingua_scc::SccFilter;

pub fn opts() -> ExtractOptions {
    ExtractOptions::default()
}

pub fn extract(data: &[u8]) -> Result<Document, Error> {
    SccFilter.extract(data, &opts())
}

/// A unit's text the way LocOS writes it: inline codes as `⟦xN⟧`.
pub fn locos_text(inl: &[Inline]) -> String {
    let mut s = String::new();
    for i in inl {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { id, .. } => s.push_str(&format!("\u{27e6}x{id}\u{27e7}")),
        }
    }
    s
}

/// The inverse: `⟦xN⟧` become standalone tags again.
pub fn from_locos(text: &str) -> Vec<Inline> {
    let mut out = Vec::new();
    let mut rest = text;
    let mut buf = String::new();
    while let Some(p) = rest.find('\u{27e6}') {
        let tail = &rest[p..];
        let parsed = tail
            .strip_prefix("\u{27e6}x")
            .and_then(|t| t.find('\u{27e7}').map(|q| (&t[..q], q)))
            .filter(|(d, _)| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()));
        match parsed {
            Some((digits, q)) => {
                buf.push_str(&rest[..p]);
                if !buf.is_empty() {
                    out.push(Inline::Text(std::mem::take(&mut buf)));
                }
                out.push(Inline::tag(digits.to_string(), TagKind::Standalone, ""));
                rest = &rest[p + "\u{27e6}x".len() + q + "\u{27e7}".len()..];
            }
            None => {
                buf.push_str(&rest[..p + "\u{27e6}".len()]);
                rest = &rest[p + "\u{27e6}".len()..];
            }
        }
    }
    buf.push_str(rest);
    if !buf.is_empty() {
        out.push(Inline::Text(buf));
    }
    out
}

pub fn units(data: &[u8]) -> Vec<String> {
    extract(data).expect("extract").units.iter().map(|u| locos_text(&u.source())).collect()
}

/// (unit index, text) pairs as LocOS's tests print them.
pub fn indexed(data: &[u8]) -> Vec<(usize, String)> {
    units(data).into_iter().enumerate().collect()
}

/// Sets targets by unit index (LocOS `paragraph_texts`) and merges.
pub fn try_build(data: &[u8], mapping: &[(usize, String)]) -> Result<Vec<u8>, Error> {
    try_build_with(data, mapping, &MergeOptions::default())
}

pub fn try_build_with(data: &[u8], mapping: &[(usize, String)], o: &MergeOptions) -> Result<Vec<u8>, Error> {
    let mut doc = extract(data).expect("extract");
    for (i, t) in mapping {
        let id = doc.units[*i].segments[0].id.clone();
        doc.set_target(&id, from_locos(t)).expect("set_target");
    }
    SccFilter.merge_with(&doc, o)
}

pub fn build(data: &[u8], mapping: &[(usize, &str)]) -> Vec<u8> {
    let m: Vec<(usize, String)> = mapping.iter().map(|(i, s)| (*i, s.to_string())).collect();
    try_build(data, &m).expect("build")
}

pub fn assert_identity(data: &[u8]) {
    let doc = extract(data).unwrap();
    assert_eq!(SccFilter.merge(&doc).unwrap(), data, "untouched merge");
    let echo: Vec<(usize, String)> = units(data).into_iter().enumerate().collect();
    assert_eq!(try_build(data, &echo).unwrap(), data, "echo merge");
}

pub fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

pub fn corpus_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/scc")
}

/// The line of a file that starts with `tc`.
pub fn line_of(data: &[u8], tc: &str) -> String {
    let t = String::from_utf8_lossy(data).trim_start_matches('\u{feff}').to_string();
    t.lines().find(|l| l.starts_with(tc)).unwrap_or_else(|| panic!("no line {tc}")).to_string()
}

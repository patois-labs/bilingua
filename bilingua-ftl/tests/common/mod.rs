#![allow(dead_code)]
use bilingua_ftl::{FtlFilter, Limits};
use bilingua_model::*;

pub fn opts() -> ExtractOptions {
    ExtractOptions::default()
}

pub fn extract(data: &[u8]) -> Result<Document, Error> {
    FtlFilter.extract(data, &opts())
}

pub fn extract_terms(data: &[u8]) -> Result<Document, Error> {
    FtlFilter.extract_with(data, &opts(), &Limits { translate_terms: true, ..Limits::default() })
}

/// Unit text the way Localization OS shows it: tags flattened to the placeable source.
pub fn flat(u: &Unit) -> String {
    u.source()
        .iter()
        .map(|i| match i {
            Inline::Text(t) => t.clone(),
            Inline::Tag { display, .. } => display.clone(),
        })
        .collect()
}

pub fn units(data: &[u8]) -> Vec<String> {
    extract(data).expect("extract").units.iter().map(flat).collect()
}

pub fn styles(data: &[u8]) -> Vec<String> {
    extract(data).expect("extract").units.iter().map(|u| u.context.style.clone().unwrap()).collect()
}

/// Splits translator text into text and tags: a placeable of the unit's source found in the text becomes its tag
/// (the way the LocOS filter reads `{ $name }` in a translation); anything else stays text.
pub fn to_inline(u: &Unit, text: &str) -> Vec<Inline> {
    let mut tags: Vec<(String, String)> = u
        .segments[0]
        .source
        .iter()
        .filter_map(|i| if let Inline::Tag { id, display, .. } = i { Some((id.clone(), display.clone())) } else { None })
        .collect();
    tags.sort_by_key(|t| std::cmp::Reverse(t.1.len()));
    tags.dedup_by(|a, b| a.1 == b.1);
    let mut out = Vec::new();
    let mut rest = text;
    let mut buf = String::new();
    'outer: while !rest.is_empty() {
        for (id, d) in &tags {
            if rest.starts_with(d.as_str()) {
                if !buf.is_empty() {
                    out.push(Inline::text(std::mem::take(&mut buf)));
                }
                out.push(Inline::Tag { id: id.clone(), kind: TagKind::Standalone, display: d.clone(), free: true });
                rest = &rest[d.len()..];
                continue 'outer;
            }
        }
        let c = rest.chars().next().unwrap();
        buf.push(c);
        rest = &rest[c.len_utf8()..];
    }
    if !buf.is_empty() {
        out.push(Inline::text(buf));
    }
    out
}

pub fn build_doc(doc: Document, mapping: &[(usize, &str)]) -> Vec<u8> {
    build_doc_with(doc, mapping, None)
}

pub fn build_doc_with(mut doc: Document, mapping: &[(usize, &str)], locale: Option<&str>) -> Vec<u8> {
    for (i, t) in mapping {
        let id = doc.units[*i].segments[0].id.clone();
        let inl = to_inline(&doc.units[*i], t);
        doc.set_target(&id, inl).expect("set_target");
    }
    match locale {
        Some(l) => FtlFilter.merge_localized(&doc, Some(l)).expect("merge"),
        None => FtlFilter.merge(&doc).expect("merge"),
    }
}

/// Sets targets by unit index (as the LocOS mapping does) and merges.
pub fn build(data: &[u8], mapping: &[(usize, &str)]) -> Vec<u8> {
    build_doc(extract(data).expect("extract"), mapping)
}

pub fn build_locale(data: &[u8], mapping: &[(usize, &str)], locale: &str) -> Vec<u8> {
    build_doc_with(extract(data).expect("extract"), mapping, Some(locale))
}

/// Untouched merge and an echo merge (every unit's own text as target) both give back the original bytes.
pub fn assert_identity(data: &[u8]) {
    let doc = extract(data).unwrap();
    assert_eq!(FtlFilter.merge(&doc).unwrap(), data, "untouched merge");
    let texts = units(data);
    let echo: Vec<(usize, &str)> = texts.iter().enumerate().map(|(i, t)| (i, t.as_str())).collect();
    assert_eq!(build(data, &echo), data, "echo merge");
}

pub fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

pub fn corpus_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/ftl")
}

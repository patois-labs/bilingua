#![allow(dead_code)]
use bilingua_model::*;
use bilingua_serverpages::{Aspx, Erb, Jsp, Phtml, Shtml};

pub fn filter(fmt: &str) -> Box<dyn Filter> {
    match fmt {
        "phtml" => Box::new(Phtml),
        "jsp" => Box::new(Jsp),
        "aspx" => Box::new(Aspx),
        "erb" => Box::new(Erb),
        "shtml" => Box::new(Shtml),
        other => panic!("unknown format {other}"),
    }
}

pub const FORMATS: [&str; 5] = ["phtml", "jsp", "aspx", "erb", "shtml"];

pub fn opts() -> ExtractOptions {
    ExtractOptions::default()
}

pub fn extract(fmt: &str, data: &[u8]) -> Result<Document, Error> {
    filter(fmt).extract(data, &opts())
}

pub fn extract_str(fmt: &str, src: &str) -> Result<Document, Error> {
    extract(fmt, src.as_bytes())
}

/// The unit text the way LocOS shows it: `g` pairs and `x` points numbered by the unit's own token ids.
pub fn locos_text(u: &Unit) -> String {
    let mut s = String::new();
    for i in u.source() {
        match i {
            Inline::Text(t) => s.push_str(&t),
            Inline::Tag { id, kind: TagKind::Open, .. } => s.push_str(&format!("\u{27E6}g{id}\u{27E7}")),
            Inline::Tag { id, kind: TagKind::Close, .. } => s.push_str(&format!("\u{27E6}/g{id}\u{27E7}")),
            Inline::Tag { id, kind: TagKind::Standalone, .. } => s.push_str(&format!("\u{27E6}x{id}\u{27E7}")),
        }
    }
    s
}

pub fn texts(fmt: &str, data: &[u8]) -> Vec<String> {
    extract(fmt, data).expect("extract").units.iter().map(locos_text).collect()
}

pub fn texts_str(fmt: &str, src: &str) -> Vec<String> {
    texts(fmt, src.as_bytes())
}

/// What the LocOS adapter does with a translated text: `x`, `g` and `/g` tokens name the source tag with that
/// number and kind; a token the source does not have is dropped; each source tag is used once.
pub fn target_from_locos(u: &Unit, text: &str) -> Vec<Inline> {
    let src = u.source();
    let has = |id: &str, kind: TagKind| src.iter().any(|i| matches!(i, Inline::Tag { id: i2, kind: k2, .. } if i2 == id && *k2 == kind));
    let tag = |id: &str, kind: TagKind| -> Inline {
        for i in &src {
            if let Inline::Tag { id: i2, kind: k2, .. } = i {
                if i2 == id && *k2 == kind {
                    return i.clone();
                }
            }
        }
        unreachable!()
    };
    let mut out: Vec<Inline> = Vec::new();
    let mut used: Vec<(String, TagKind)> = Vec::new();
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
        let (kind, body) = if let Some(b) = inner.strip_prefix('/') {
            (TagKind::Close, b.trim_start_matches(|c: char| c.is_ascii_lowercase()))
        } else if inner.starts_with('x') || inner.starts_with('p') {
            (TagKind::Standalone, &inner[1..])
        } else {
            (TagKind::Open, inner.trim_start_matches(|c: char| c.is_ascii_lowercase()))
        };
        let well = !body.is_empty() && body.bytes().all(|b| b.is_ascii_digit());
        if well {
            if has(body, kind) && !used.contains(&(body.to_string(), kind)) {
                if !lit.is_empty() {
                    out.push(Inline::Text(std::mem::take(&mut lit)));
                }
                out.push(tag(body, kind));
                used.push((body.to_string(), kind));
            }
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

/// Sets targets by unit index from LocOS-style text and merges.
pub fn build_doc(fmt: &str, data: &[u8], mapping: &[(usize, &str)], target: Option<&str>) -> Result<Vec<u8>, Error> {
    let f = filter(fmt);
    let mut doc = f.extract(data, &opts())?;
    doc.target_lang = target.map(String::from);
    for (k, t) in mapping {
        let id = doc.units[*k].segments[0].id.clone();
        let inl = target_from_locos(&doc.units[*k], t);
        doc.set_target(&id, inl).expect("set_target");
    }
    f.merge(&doc)
}

pub fn build(fmt: &str, data: &[u8], mapping: &[(usize, &str)]) -> Vec<u8> {
    build_doc(fmt, data, mapping, None).expect("merge")
}

pub fn build_str(fmt: &str, src: &str, mapping: &[(usize, &str)]) -> String {
    String::from_utf8(build(fmt, src.as_bytes(), mapping)).unwrap()
}

/// Untouched merge and an echo merge (every unit's own text as target) both give back the original bytes.
pub fn assert_identity(fmt: &str, data: &[u8]) {
    let f = filter(fmt);
    let doc = f.extract(data, &opts()).unwrap();
    assert_eq!(f.merge(&doc).unwrap(), data, "untouched merge");
    let t = texts(fmt, data);
    let echo: Vec<(usize, &str)> = t.iter().enumerate().map(|(i, s)| (i, s.as_str())).collect();
    assert_eq!(build(fmt, data, &echo), data, "echo merge");
}

pub fn refused(fmt: &str, src: &str, needle: &str) -> String {
    match extract_str(fmt, src) {
        Err(Error::Parse { msg, .. }) | Err(Error::Unsupported(msg)) | Err(Error::Encoding(msg)) => {
            assert!(msg.contains(needle), "{needle:?} not in {msg:?} for {src:?}");
            msg
        }
        Err(e) => panic!("wrong error kind {e:?}"),
        Ok(_) => panic!("not refused: {src:?}"),
    }
}

pub fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

pub fn data_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data")
}

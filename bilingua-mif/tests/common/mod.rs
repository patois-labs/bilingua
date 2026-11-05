#![allow(dead_code)]
use bilingua_mif::Mif;
use bilingua_model::*;

pub fn opts() -> ExtractOptions {
    ExtractOptions::default()
}

pub fn extract(data: &[u8]) -> Result<Document, Error> {
    Mif.extract(data, &opts())
}

/// The unit text the way LocOS shows it: one token per inline code (`g` pairs, `x` points).
pub fn locos_text(u: &Unit) -> String {
    bilingua_model::locos::to_locos_text(&u.source()).0
}

/// `(style, LocOS text)` of every unit.
pub fn units(data: &[u8]) -> Vec<(String, String)> {
    extract(data)
        .expect("extract")
        .units
        .iter()
        .map(|u| (u.context.style.clone().unwrap_or_default(), locos_text(u)))
        .collect()
}

pub fn texts(data: &[u8]) -> Vec<String> {
    units(data).into_iter().map(|(_, t)| t).collect()
}

/// What the LocOS adapter does with translated text: a token `⟦xN⟧` / `⟦pN⟧` names point tag N, `⟦gN⟧` and
/// `⟦/gN⟧` the pair N; tokens of another kind (or with no kind letter) are dropped; the rest is text. Tag numbers
/// are NOT checked against the source (the writer drops unknown and repeated ones, as LocOS does).
pub fn target_from_locos(text: &str) -> Vec<Inline> {
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
        let (slash, body) = match inner.strip_prefix('/') {
            Some(b) => (true, b),
            None => (false, inner),
        };
        let letters: String = body.chars().take_while(|c| c.is_ascii_lowercase()).collect();
        let digits = &body[letters.len()..];
        let well = letters.len() <= 1 && !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit());
        if well {
            let kind = match (letters.as_str(), slash) {
                ("x" | "p", false) => Some(TagKind::Standalone),
                ("g", false) => Some(TagKind::Open),
                ("g", true) => Some(TagKind::Close),
                _ => None,
            };
            if let Some(kind) = kind {
                if !lit.is_empty() {
                    out.push(Inline::Text(std::mem::take(&mut lit)));
                }
                out.push(Inline::tag(digits.to_string(), kind, String::new()));
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
pub fn build(data: &[u8], mapping: &[(usize, &str)]) -> Vec<u8> {
    build_for(data, mapping, None)
}

pub fn build_for(data: &[u8], mapping: &[(usize, &str)], target_lang: Option<&str>) -> Vec<u8> {
    let mut doc = extract(data).expect("extract");
    doc.target_lang = target_lang.map(String::from);
    for (k, t) in mapping {
        doc.units[*k].segments[0].target = target_from_locos(t);
    }
    Mif.merge(&doc).expect("merge")
}

pub fn build_str(data: &[u8], mapping: &[(usize, &str)]) -> String {
    String::from_utf8(build(data, mapping)).unwrap()
}

pub fn refused(data: &[u8]) -> Error {
    match extract(data) {
        Err(e) => e,
        Ok(_) => panic!("not refused: {:?}", String::from_utf8_lossy(&data[..data.len().min(80)])),
    }
}

/// Untouched merge and an echo merge (every unit's own text as target) both give back the original bytes.
pub fn assert_identity(data: &[u8]) {
    let doc = extract(data).unwrap();
    assert!(Mif.merge(&doc).unwrap() == data, "untouched merge");
    let t = texts(data);
    let echo: Vec<(usize, &str)> = t.iter().enumerate().map(|(i, s)| (i, s.as_str())).collect();
    assert!(build(data, &echo) == data, "echo merge");
}

pub fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

pub fn corpus_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/mif")
}

pub const SUITE: &[u8] = include_bytes!("../data/locos_suite.mif");

/// SHA-1 (for comparing large LocOS outputs by digest).
pub fn sha1(data: &[u8]) -> String {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[4 * i], chunk[4 * i + 1], chunk[4 * i + 2], chunk[4 * i + 3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut b, mut c, mut d, mut e] = h;
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | (!b & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6u32),
            };
            let t = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(*wi);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = t;
        }
        h = [
            h[0].wrapping_add(a),
            h[1].wrapping_add(b),
            h[2].wrapping_add(c),
            h[3].wrapping_add(d),
            h[4].wrapping_add(e),
        ];
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

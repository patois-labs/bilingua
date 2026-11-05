#![allow(dead_code)]
//! Helpers shared by the test files: a hand builder for GSI and TTI blocks (independent of the crate), and the
//! usual extract/merge shortcuts. The LocOS token form is `\u{27E6}xN\u{27E7}`.

use bilingua_model::*;
use bilingua_stl::Stl;

pub fn gsi(cct: &[u8; 2], lc: &[u8; 2], dsc: u8, mnc: &[u8; 2], tnb: usize) -> Vec<u8> {
    let mut g = vec![b' '; 1024];
    g[0..3].copy_from_slice(b"850");
    g[3..11].copy_from_slice(b"STL25.01");
    g[11] = dsc;
    g[12..14].copy_from_slice(cct);
    g[14..16].copy_from_slice(lc);
    g[16..30].copy_from_slice(b"Demo programme");
    g[224..230].copy_from_slice(b"261010");
    g[230..236].copy_from_slice(b"261010");
    g[236..238].copy_from_slice(b"01");
    g[238..243].copy_from_slice(format!("{tnb:05}").as_bytes());
    g[243..248].copy_from_slice(format!("{tnb:05}").as_bytes());
    g[248..251].copy_from_slice(b"001");
    g[251..253].copy_from_slice(mnc);
    g[253..255].copy_from_slice(b"23");
    g[255] = b'1';
    g[256..264].copy_from_slice(b"10000000");
    g[264..272].copy_from_slice(b"10000100");
    g[272..274].copy_from_slice(b"11");
    g[274..277].copy_from_slice(b"GBR");
    g[448..460].copy_from_slice(b"user area ok");
    g
}

pub fn gsi_default(tnb: usize) -> Vec<u8> {
    gsi(b"00", b"09", b'1', b"40", tnb)
}

pub struct Tti {
    pub sn: u16,
    pub ebn: u8,
    pub cf: u8,
    pub vp: u8,
    pub jc: u8,
    pub tci: [u8; 4],
    pub tco: [u8; 4],
}

impl Default for Tti {
    fn default() -> Self {
        Tti { sn: 1, ebn: 0xFF, cf: 0, vp: 20, jc: 2, tci: [10, 0, 1, 0], tco: [10, 0, 3, 0] }
    }
}

pub fn tti(h: &Tti, tf: &[u8]) -> Vec<u8> {
    assert!(tf.len() <= 112);
    let mut b = vec![0u8];
    b.extend_from_slice(&h.sn.to_le_bytes());
    b.extend_from_slice(&[h.ebn, 0]);
    b.extend_from_slice(&h.tci);
    b.extend_from_slice(&h.tco);
    b.extend_from_slice(&[h.vp, h.jc, h.cf]);
    b.extend_from_slice(tf);
    b.resize(128, 0x8F);
    b
}

pub fn t(sn: u16, tf: &[u8]) -> Vec<u8> {
    tti(&Tti { sn, ..Default::default() }, tf)
}

pub const DH: &[u8] = b"\x0d\x0b\x0b";
pub const END: &[u8] = b"\x0a\x0a";

pub fn cat(parts: &[&[u8]]) -> Vec<u8> {
    parts.concat()
}

pub fn blocks(data: &[u8]) -> Vec<&[u8]> {
    (0..(data.len() - 1024) / 128).map(|i| &data[1024 + i * 128..1024 + (i + 1) * 128]).collect()
}

pub fn opts(target: Option<&str>) -> ExtractOptions {
    ExtractOptions { target_lang: target.map(Into::into), ..Default::default() }
}

pub fn ex(data: &[u8]) -> Document {
    Stl.extract(data, &opts(None)).unwrap()
}

/// The unit text in LocOS form.
pub fn locos_text(u: &Unit) -> String {
    let mut s = String::new();
    for i in u.source() {
        match i {
            Inline::Text(t) => s.push_str(&t),
            Inline::Tag { id, .. } => s.push_str(&format!("\u{27E6}x{id}\u{27E7}")),
        }
    }
    s
}

pub fn texts(doc: &Document) -> Vec<String> {
    doc.units.iter().map(locos_text).collect()
}

/// A LocOS text (with `\u{27E6}xN\u{27E7}` tokens) as inlines. Tokens become tags with id N.
pub fn from_locos(text: &str) -> Vec<Inline> {
    let mut out = Vec::new();
    let mut rest = text;
    let open = '\u{27E6}';
    let close = '\u{27E7}';
    while let Some(p) = rest.find(open) {
        let after = &rest[p + open.len_utf8()..];
        let tok = after.strip_prefix('x').and_then(|a| a.find(close).map(|e| (&a[..e], e)));
        match tok {
            Some((num, e)) if !num.is_empty() && num.bytes().all(|b| b.is_ascii_digit()) => {
                if p > 0 {
                    out.push(Inline::Text(rest[..p].to_string()));
                }
                out.push(Inline::tag(num.to_string(), TagKind::Standalone, "<?>"));
                rest = &after[1 + e + close.len_utf8()..];
            }
            _ => {
                out.push(Inline::Text(rest[..p + open.len_utf8()].to_string()));
                rest = after;
            }
        }
    }
    if !rest.is_empty() {
        out.push(Inline::Text(rest.to_string()));
    }
    merge_text(out)
}

/// Sets the target of unit `k` straight on the segment (no tag checks: hostile maps use unknown tokens).
pub fn set(doc: &mut Document, k: usize, text: &str) {
    let seg = &mut doc.units[k].segments[0];
    seg.target = from_locos(text);
    seg.state = SegState::Translated;
}

pub fn pseudo(doc: &mut Document) {
    for u in doc.units.iter_mut() {
        let s = &mut u.segments[0];
        let mut t = vec![Inline::text("\u{ab}")];
        t.extend(s.source.iter().cloned());
        t.push(Inline::text("\u{bb}"));
        s.target = merge_text(t);
        s.state = SegState::Translated;
    }
}

/// Build the way LocOS does: UTF-8 out, stamp always (but no target language, so nothing is stamped).
pub fn build(data: &[u8], map: &[(usize, String)]) -> Result<Vec<u8>, Error> {
    let mut d = ex(data);
    for (k, t) in map {
        set(&mut d, *k, t);
    }
    Stl.merge_with(&d, &MergeOptions::locos())
}

pub fn build_localized(data: &[u8], map: &[(usize, String)], locale: &str) -> Result<Vec<u8>, Error> {
    let mut d = Stl.extract(data, &opts(Some(locale))).unwrap();
    for (k, t) in map {
        set(&mut d, *k, t);
    }
    Stl.merge_with(&d, &MergeOptions::locos())
}

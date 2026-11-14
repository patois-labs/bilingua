#![allow(dead_code)]
//! Helpers shared by the integration tests.

use bilingua_model::*;
use bilingua_properties::Properties;

pub fn opts() -> ExtractOptions {
    ExtractOptions::new().with_target_lang("de-DE").with_granularity(Granularity::Paragraph)
}

pub fn extract(data: &[u8]) -> Document {
    Properties.extract(data, &opts()).expect("extract")
}

/// A unit's source as the text LocOS shows: tags written as their original text.
pub fn show(_u: &Unit, v: &[Inline]) -> String {
    let mut s = String::new();
    for i in v {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { display, .. } => s.push_str(display),
        }
    }
    s
}

pub fn units(data: &[u8]) -> Vec<String> {
    let d = extract(data);
    d.units.iter().map(|u| show(u, &u.source())).collect()
}

/// Plain text to inlines, turning the source's placeholders (left to right, each once) into its tags.
pub fn inlines_for(u: &Unit, text: &str) -> Vec<Inline> {
    let tags: Vec<Inline> = u.source().into_iter().filter(|i| matches!(i, Inline::Tag { .. })).collect();
    let mut used = vec![false; tags.len()];
    let mut out = Vec::new();
    let mut rest = text;
    'outer: while !rest.is_empty() {
        let mut best: Option<(usize, usize)> = None;
        for (k, t) in tags.iter().enumerate() {
            if used[k] {
                continue;
            }
            if let Inline::Tag { display, .. } = t {
                if let Some(p) = rest.find(display.as_str()) {
                    if best.is_none_or(|(bp, _)| p < bp) {
                        best = Some((p, k));
                    }
                }
            }
        }
        match best {
            Some((p, k)) => {
                if p > 0 {
                    out.push(Inline::text(&rest[..p]));
                }
                used[k] = true;
                let Inline::Tag { display, .. } = &tags[k] else { continue 'outer };
                out.push(tags[k].clone());
                rest = &rest[p + display.len()..];
            }
            None => {
                out.push(Inline::text(rest));
                break;
            }
        }
    }
    merge_text(out)
}

pub fn translate(data: &[u8], mapping: &[(usize, &str)]) -> Vec<u8> {
    let mut d = extract(data);
    for (i, t) in mapping {
        let id = d.units[*i].segments[0].id.clone();
        let inl = inlines_for(&d.units[*i], t);
        d.set_target(&id, inl).expect("set_target");
    }
    Properties.merge(&d).expect("merge")
}

/// An untouched merge and a merge with every unit echoed back must both give the original bytes.
pub fn assert_identity(data: &[u8]) {
    let d = extract(data);
    assert_eq!(Properties.merge(&d).expect("merge"), data, "untouched merge");
    let echo: Vec<(usize, String)> = d.units.iter().enumerate().map(|(i, u)| (i, show(u, &u.source()))).collect();
    let m: Vec<(usize, &str)> = echo.iter().map(|(i, s)| (*i, s.as_str())).collect();
    assert_eq!(translate(data, &m), data, "echo merge");
}

/// Merge with `mapping`, compare to the expected bytes, re-read and check every translated unit.
pub fn assert_round_trip(data: &[u8], mapping: &[(usize, &str)], expected: &[u8]) {
    let out = translate(data, mapping);
    assert_eq!(String::from_utf8_lossy(&out), String::from_utf8_lossy(expected), "rebuilt bytes");
    assert_eq!(out, expected);
    let back = units(&out);
    let before = units(data);
    assert_eq!(back.len(), before.len());
    for (i, t) in mapping {
        assert_eq!(back[*i], t.trim(), "unit {i}");
    }
}

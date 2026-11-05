#![allow(dead_code)]
//! Helpers shared by the integration tests.

use bilingua_model::*;
use bilingua_resx::Resx;

pub fn opts() -> ExtractOptions {
    ExtractOptions { target_lang: Some("de-DE".into()), granularity: Granularity::Paragraph, ..Default::default() }
}

pub fn extract(data: &[u8]) -> Document {
    Resx.extract(data, &opts()).expect("extract")
}

/// A unit's source as the text LocOS shows: tags written as their original text.
pub fn show(v: &[Inline]) -> String {
    v.iter()
        .map(|i| match i {
            Inline::Text(t) => t.clone(),
            Inline::Tag { display, .. } => display.clone(),
        })
        .collect()
}

pub fn units(data: &[u8]) -> Vec<String> {
    extract(data).units.iter().map(|u| show(&u.source())).collect()
}

/// Plain text to inlines, turning the source's placeholders (left to right, each once) into its tags.
pub fn inlines_for(u: &Unit, text: &str) -> Vec<Inline> {
    let tags: Vec<Inline> = u.source().into_iter().filter(|i| matches!(i, Inline::Tag { .. })).collect();
    let mut used = vec![false; tags.len()];
    let mut out = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
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
                let Inline::Tag { display, .. } = &tags[k] else { unreachable!() };
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
    Resx.merge(&d).expect("merge")
}

/// Wraps every unit's source in brackets, as a pseudo translation.
pub fn pseudo(d: &Document) -> Document {
    let mut e = d.clone();
    for u in &d.units {
        if !u.translate {
            continue;
        }
        let id = u.segments[0].id.clone();
        let mut t = vec![Inline::text("[")];
        t.extend(u.source());
        t.push(Inline::text("]"));
        e.set_target(&id, t).expect("set_target");
    }
    e
}

/// An untouched merge and a merge with every unit echoed back must both give the original bytes.
pub fn assert_identity(data: &[u8]) {
    let d = extract(data);
    assert_eq!(Resx.merge(&d).expect("merge"), data, "untouched merge");
    let echo: Vec<(usize, String)> = d.units.iter().enumerate().map(|(i, u)| (i, show(&u.source()))).collect();
    let m: Vec<(usize, &str)> = echo.iter().map(|(i, s)| (*i, s.as_str())).collect();
    assert_eq!(translate(data, &m), data, "echo merge");
}

/// Every byte outside the translated values is identical: put the old inner XML back for the new and get the
/// original.
pub fn only_changed(before: &[u8], after: &[u8], olds: &[&str], news: &[&str]) {
    let mut out = String::from_utf8(after.to_vec()).expect("utf-8");
    for (old, new) in olds.iter().zip(news) {
        assert!(out.contains(new), "{new} not in output");
        out = out.replacen(new, old, 1);
    }
    assert_eq!(out.as_bytes(), before, "bytes outside the translated values changed");
}

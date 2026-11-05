#![allow(dead_code)]
use bilingua_model::*;
use bilingua_plist::{Plist, Stringsdict};

pub mod bp;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum F {
    Plist,
    Sd,
}

impl F {
    pub fn filter(self) -> Box<dyn Filter> {
        match self {
            F::Plist => Box::new(Plist),
            F::Sd => Box::new(Stringsdict),
        }
    }
}

pub fn ex(f: F, b: &[u8]) -> Document {
    f.filter().extract(b, &ExtractOptions::default()).unwrap()
}

pub fn ex_with(f: F, b: &[u8], opts: &ExtractOptions) -> Document {
    f.filter().extract(b, opts).unwrap()
}

pub fn merged(f: F, d: &Document) -> Vec<u8> {
    f.filter().merge(d).unwrap()
}

/// Source text of a unit with each tag shown as its original specifier.
pub fn shown(v: &[Inline]) -> String {
    v.iter()
        .map(|i| match i {
            Inline::Text(t) => t.clone(),
            Inline::Tag { display, .. } => display.clone(),
        })
        .collect()
}

/// The LocOS-style token text of a unit: `⟦xN⟧` for a fixed specifier, `⟦pN⟧` for a positional one.
pub fn tokens(u: &Unit) -> String {
    let mut s = String::new();
    for i in u.source() {
        match i {
            Inline::Text(t) => s.push_str(&t),
            Inline::Tag { id, free, .. } => s.push_str(&format!("\u{27e6}{}{id}\u{27e7}", if free { 'p' } else { 'x' })),
        }
    }
    s
}

pub fn texts(d: &Document) -> Vec<String> {
    d.units.iter().filter(|u| u.translate).map(|u| shown(&u.source())).collect()
}

pub fn unit_tokens(d: &Document) -> Vec<String> {
    d.units.iter().filter(|u| u.translate).map(tokens).collect()
}

pub fn units_of(f: F, b: &[u8]) -> Vec<String> {
    texts(&ex(f, b))
}

/// A target for `s`: specifier text that matches a source tag becomes that tag.
pub fn target_for(u: &Unit, s: &str) -> Vec<Inline> {
    let tags: Vec<(String, String, bool)> = u
        .source()
        .iter()
        .filter_map(|i| match i {
            Inline::Tag { id, display, free, .. } => Some((id.clone(), display.clone(), *free)),
            _ => None,
        })
        .collect();
    let mut used = vec![false; tags.len()];
    let mut out = Vec::new();
    let mut text = String::new();
    let mut rest = s;
    'outer: while !rest.is_empty() {
        if rest.starts_with('%') {
            for (k, (id, display, free)) in tags.iter().enumerate() {
                if !used[k] && rest.starts_with(display.as_str()) {
                    if !text.is_empty() {
                        out.push(Inline::text(std::mem::take(&mut text)));
                    }
                    out.push(if *free { Inline::free_tag(id.clone(), display.clone()) } else { Inline::tag(id.clone(), TagKind::Standalone, display.clone()) });
                    used[k] = true;
                    rest = &rest[display.len()..];
                    continue 'outer;
                }
            }
        }
        let c = rest.chars().next().unwrap();
        text.push(c);
        rest = &rest[c.len_utf8()..];
    }
    if !text.is_empty() {
        out.push(Inline::text(text));
    }
    out
}

/// Sets the target of the nth translatable unit (LocOS numbers units the same way).
pub fn set_nth(d: &mut Document, n: usize, s: &str) {
    let (id, t) = {
        let u = d.units.iter().filter(|u| u.translate).nth(n).unwrap();
        (u.segments[0].id.clone(), target_for(u, s))
    };
    d.set_target(&id, t).unwrap();
}

/// Every unit echoed as its own text: LocOS `build(data, _all(filt, data))`.
pub fn echo(f: F, b: &[u8]) -> Vec<u8> {
    let mut d = ex(f, b);
    for n in 0..texts(&d).len() {
        let s = texts(&d)[n].clone();
        set_nth(&mut d, n, &s);
    }
    merged(f, &d)
}

/// Every unit translated as `prefix + text` (a pseudo translation that keeps the tags).
pub fn prefixed(d: &Document, prefix: &str) -> Document {
    let mut p = d.clone();
    for u in d.units.iter().filter(|u| u.translate) {
        let mut t = vec![Inline::text(prefix)];
        t.extend(u.source());
        p.set_target(&u.segments[0].id, t).unwrap();
    }
    p
}

pub fn parse_err(f: F, b: &[u8]) -> Error {
    match f.filter().extract(b, &ExtractOptions::default()) {
        Err(e) => e,
        Ok(_) => panic!("must be refused: {:?}", String::from_utf8_lossy(&b[..b.len().min(60)])),
    }
}

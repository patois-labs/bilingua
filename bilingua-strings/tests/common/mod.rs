#![allow(dead_code)]
use bilingua_model::*;
use bilingua_strings::Strings;

pub fn opts() -> ExtractOptions {
    ExtractOptions::default()
}

pub fn ex(b: &[u8]) -> Document {
    Strings.extract(b, &opts()).unwrap()
}

pub fn merged(d: &Document) -> Vec<u8> {
    Strings.merge(d).unwrap()
}

/// Source text of a unit with each tag shown as its original placeholder text.
pub fn shown(v: &[Inline]) -> String {
    v.iter()
        .map(|i| match i {
            Inline::Text(t) => t.clone(),
            Inline::Tag { display, .. } => display.clone(),
        })
        .collect()
}

/// What LocOS lists as units: the translatable units' source texts, placeholders in place.
pub fn texts(d: &Document) -> Vec<String> {
    d.units.iter().filter(|u| u.translate).map(|u| shown(&u.source())).collect()
}

pub fn units_of(b: &[u8]) -> Vec<String> {
    texts(&ex(b))
}

/// A target for `s`: placeholder text that matches a source tag becomes that tag.
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

/// Echo every unit as its own text: must give the original bytes back.
pub fn echo(b: &[u8]) -> Vec<u8> {
    let mut d = ex(b);
    for n in 0..texts(&d).len() {
        let s = texts(&d)[n].clone();
        set_nth(&mut d, n, &s);
    }
    merged(&d)
}

/// LocOS `assert_identity`: no translations and every unit echoed both give the original bytes.
pub fn assert_identity(b: &[u8]) {
    assert_eq!(merged(&ex(b)), b, "empty merge must be byte-identical");
    assert_eq!(echo(b), b, "echo merge must be byte-identical");
}

pub fn parse_err(b: &[u8]) -> String {
    match Strings.extract(b, &opts()) {
        Err(e) => e.to_string(),
        Ok(_) => panic!("must be refused: {:?}", String::from_utf8_lossy(&b[..b.len().min(60)])),
    }
}

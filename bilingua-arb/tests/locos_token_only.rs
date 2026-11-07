//! A message of only a literal LocOS token (`⟦x1⟧` in the source text) under the LocOS profile, compared with the
//! LocOS ARB filter (11-06 gate, its `parse` on the same bytes), and a document extracted under one profile merged
//! under the other.

use bilingua_arb::Arb;
use bilingua_model::*;

const T: &str = "\u{27E6}x1\u{27E7}";

/// The unit text as LocOS lists it: `{name}` as plain text, `#` and complex arguments as `⟦xN⟧`.
fn lo(u: &Unit) -> String {
    let mut out = String::new();
    let mut n = 0;
    for i in u.source() {
        match i {
            Inline::Text(t) => out.push_str(&t),
            Inline::Tag { free: true, display, .. } => out.push_str(&display),
            Inline::Tag { .. } => {
                n += 1;
                out.push_str(&format!("\u{27E6}x{n}\u{27E7}"));
            }
        }
    }
    out
}

fn units(src: &str, profile: Option<&str>) -> Vec<String> {
    let o = ExtractOptions::new().with_source_lang("en").with_profile(profile);
    Arb.extract(src.as_bytes(), &o).unwrap().units.iter().map(lo).collect()
}

#[test]
fn token_only_messages_follow_locos_under_the_profile() {
    let msg = |a: &str| format!("{{\"@@locale\": \"en\", \"a\": \"{a}\", \"b\": \"Hi\"}}");
    // (message a, LocOS units of the file); without the profile every message a is a unit of its own
    for (a, locos) in [
        ("{value}".to_string(), vec!["{value}", "Hi"]),
        ("{count, plural, one{# item} other{# items}}".to_string(), vec![&format!("{T} item")[..], &format!("{T} items"), "Hi"]),
        (format!("{{count, plural, one{{{T}}} other{{# items}}}}"), vec![&format!("{T} items")[..], "Hi"]),
        (T.to_string(), vec!["Hi"]),
        (format!("  {T} "), vec!["Hi"]),
        (format!("{T}!?"), vec!["Hi"]),
        (format!("\u{27E6}x2\u{27E7} {T}"), vec!["Hi"]),
        ("\u{27E6}x\u{661}\u{27E7}".to_string(), vec!["Hi"]),
        (format!("{{{T}"), vec!["Hi"]),
        (format!("Hi {T}"), vec![&format!("Hi {T}")[..], "Hi"]),
        ("\u{27E6}x\u{27E7}".to_string(), vec!["\u{27E6}x\u{27E7}", "Hi"]),
    ] {
        let src = msg(&a);
        assert_eq!(units(&src, Some(profile::LOCOS)), locos, "{a}");
        let plain = units(&src, None);
        assert!(plain.len() >= locos.len(), "{a}: {plain:?}");
        if !a.contains("plural") {
            assert_eq!(plain.len(), 2, "{a}: {plain:?}");
        }
    }
}

/// The unit plan follows the extract: a document extracted under the profile merges without it, and one
/// extracted without it merges under it, each translation landing on its own message.
#[test]
fn a_document_merges_under_either_profile() {
    let src = format!("{{\"@@locale\": \"en\", \"a\": \"{T}\", \"p\": \"{{n, plural, one{{{T}}} other{{# items}}}}\", \"b\": \"Hi\"}}");
    for (extract_with, merge_with) in [(Some(profile::LOCOS), None), (None, Some(profile::LOCOS)), (Some(profile::LOCOS), Some(profile::LOCOS)), (None, None)] {
        let o = ExtractOptions::new().with_source_lang("en").with_profile(extract_with);
        let mut doc = Arb.extract(src.as_bytes(), &o).unwrap();
        let ids: Vec<(String, Vec<Inline>, Option<String>)> =
            doc.units.iter().map(|u| (u.segments[0].id.clone(), u.segments[0].source.clone(), u.context.key.clone())).collect();
        for (id, s, key) in ids {
            let mut t = vec![Inline::text(format!("{}:", key.unwrap_or_default()))];
            t.extend(s);
            doc.set_target(&id, t).unwrap();
        }
        let m = MergeOptions::new().with_profile(merge_with);
        let out = String::from_utf8(Arb.merge_with(&doc, &m).unwrap_or_else(|e| panic!("{extract_with:?} -> {merge_with:?}: {e}"))).unwrap();
        assert!(out.contains("\"b\": \"b:Hi\""), "{extract_with:?} -> {merge_with:?}: {out}");
        assert!(out.contains("other{p:# items}"), "{extract_with:?} -> {merge_with:?}: {out}");
        let a_translated = out.contains(&format!("\"a\": \"a:{T}\""));
        // the units of a message of only a token exist only without the profile; the token is text, not a reason for the source
        assert_eq!(a_translated, extract_with.is_none(), "{extract_with:?} -> {merge_with:?}: {out}");
    }
}

/// #814.1: a token in a translation that names no code of the unit is kept as text, under both profiles, and the
/// translation is never replaced by the source. A real argument still maps beside it.
#[test]
fn unknown_token_in_a_translation_is_kept_as_text() {
    let src = "{\"@@locale\": \"en\", \"a\": \"Hello {name}\", \"p\": \"{n, plural, one{# item} other{# items}}\", \"b\": \"Hi\"}";
    for profile in [Some(profile::LOCOS), None] {
        let o = ExtractOptions::new().with_source_lang("en").with_profile(profile);
        let mut doc = Arb.extract(src.as_bytes(), &o).unwrap();
        let ids: Vec<(String, Vec<Inline>, Option<String>)> =
            doc.units.iter().map(|u| (u.segments[0].id.clone(), u.segments[0].source.clone(), u.context.key.clone())).collect();
        for (id, s, key) in ids {
            let mut t = vec![Inline::text(format!("{}: {T} \u{27E6}x9\u{27E7} ", key.unwrap_or_default()))];
            t.extend(s);
            doc.set_target(&id, t).unwrap();
        }
        let out = String::from_utf8(Arb.merge_with(&doc, &MergeOptions::new().with_profile(profile)).unwrap()).unwrap();
        let tok = format!("{T} \u{27E6}x9\u{27E7} ");
        assert!(out.contains(&format!("\"b\": \"b: {tok}Hi\"")), "{profile:?}: {out}");
        assert!(out.contains(&format!("\"a\": \"a: {tok}Hello {{name}}\"")), "{profile:?}: {out}");
        assert!(out.contains(&format!("other{{p: {tok}# items}}")), "{profile:?}: {out}");
    }
}

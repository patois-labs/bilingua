//! Issue 814 item 1: text that looks like a LocOS token (`⟦x2⟧`, `⟦/g1⟧`) is text, in a source and in a
//! translation; only the unit's real codes are tokens when a line is written. The emphasis repair still runs.
use bilingua_mdx::Mdx;
use bilingua_model::*;

fn tags(src: &[Inline]) -> Vec<Inline> {
    src.iter().filter(|i| matches!(i, Inline::Tag { .. })).cloned().collect()
}

fn id_of(i: &Inline) -> String {
    match i {
        Inline::Tag { id, .. } => id.clone(),
        _ => unreachable!(),
    }
}

fn merged(data: &str, target: impl Fn(&[Inline]) -> Vec<Inline>) -> String {
    let mut doc = Mdx.extract(data.as_bytes(), &ExtractOptions::default()).unwrap();
    let seg = doc.units[0].segments[0].clone();
    doc.set_target(&seg.id, target(&seg.source)).unwrap();
    String::from_utf8(Mdx.merge(&doc).unwrap()).unwrap()
}

/// A line whose text holds the tokens its own codes get, an unknown one and the private-use characters.
fn line() -> String {
    let doc = Mdx.extract(b"Press <Kbd>Ctrl</Kbd> and {count} now.\n", &ExtractOptions::default()).unwrap();
    let t = tags(&doc.units[0].segments[0].source);
    let (g, x) = (id_of(&t[0]), id_of(&t[2]));
    format!("Press \u{27e6}g{g}\u{27e7} <Kbd>Ctrl</Kbd> \u{27e6}/g{g}\u{27e7} and {{count}} \u{27e6}x{x}\u{27e7} \u{e000}\u{e001}\u{27e6}9\u{27e7} now.\n")
}

#[test]
fn look_alikes_in_the_source_come_back_byte_for_byte() {
    let src = line();
    let doc = Mdx.extract(src.as_bytes(), &ExtractOptions::default()).unwrap();
    assert_eq!(tags(&doc.units[0].segments[0].source).len(), 3, "{:?}", doc.units[0].segments[0].source);
    assert_eq!(merged(&src, |s| s.to_vec()), src);
}

#[test]
fn look_alikes_in_a_translation_stay_text_when_codes_move() {
    let src = line();
    let out = merged(&src, |s| {
        let t = tags(s);
        let (g, x) = (id_of(&t[0]), id_of(&t[2]));
        vec![
            Inline::text("Und "),
            t[2].clone(),
            Inline::text(format!(" \u{27e6}g{g}\u{27e7}lit\u{27e6}/g{g}\u{27e7} ")),
            t[0].clone(),
            Inline::text("Strg"),
            t[1].clone(),
            Inline::text(format!(" \u{27e6}x{x}\u{27e7} \u{e000}\u{e001}")),
        ]
    });
    let doc = Mdx.extract(src.as_bytes(), &ExtractOptions::default()).unwrap();
    let t = tags(&doc.units[0].segments[0].source);
    let (g, x) = (id_of(&t[0]), id_of(&t[2]));
    assert_eq!(out, format!("Und {{count}} \u{27e6}g{g}\u{27e7}lit\u{27e6}/g{g}\u{27e7} <Kbd>Strg</Kbd> \u{27e6}x{x}\u{27e7} \u{e000}\u{e001}\n"));
    // the build reads back with the same codes
    let back = Mdx.extract(out.as_bytes(), &ExtractOptions::default()).unwrap();
    assert_eq!(tags(&back.units[0].segments[0].source).len(), 3);
}

#[test]
fn emphasis_repair_still_runs_beside_a_look_alike() {
    let out = merged("Choose __Advanced__ options \u{27e6}x9\u{27e7}.\n", |_| vec![Inline::text("__\u{8a73}\u{7d30}__\u{30aa}\u{30d7}\u{30b7}\u{30e7}\u{30f3}\u{27e6}x9\u{27e7}\u{3092}\u{9078}\u{629e}\u{3002}")]);
    assert_eq!(out, "**\u{8a73}\u{7d30}**\u{30aa}\u{30d7}\u{30b7}\u{30e7}\u{30f3}\u{27e6}x9\u{27e7}\u{3092}\u{9078}\u{629e}\u{3002}\n");
}

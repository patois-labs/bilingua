//! Regressions found by running real files from the Internet through the pseudo round trip (hand-written inputs).
mod common;
use common::*;

fn one(src: &str) -> String {
    let t = texts(&mw(), src.as_bytes());
    assert_eq!(t.len(), 1, "{t:?}");
    t[0].clone()
}

#[test]
fn localized_file_namespace_hides_its_options() {
    let t = one("See [[\u{424}\u{430}\u{439}\u{43b}:a.jpg|\u{43c}\u{438}\u{43d}\u{438}|A small cat]] here.\n");
    assert!(t.contains("A small cat"), "{t}");
    assert!(!t.contains("\u{43c}\u{438}\u{43d}\u{438}"), "the option is not text: {t}");
    let t = one("Voir [[Datei:a.png|mini|links|Ein Hund]] hier.\n");
    assert!(t.contains("Ein Hund") && !t.contains("mini") && !t.contains("links"), "{t}");
    assert_identity(&mw(), "See [[Datei:a.png|mini|Ein Hund]] here.\n".as_bytes());
}

#[test]
fn caption_followed_by_options_is_the_caption() {
    let t = one("A [[File:a.png|thumb|A brown dog|upright]] b.\n");
    assert!(t.contains("A brown dog") && !t.contains("upright"), "{t}");
}

#[test]
fn local_pixel_unit_is_an_option() {
    let t = one("x [[\u{645}\u{644}\u{641}:a.png|250\u{628}\u{643}|Some caption]] y\n");
    assert!(t.contains("Some caption") && !t.contains("250"), "{t}");
}

#[test]
fn words_that_only_look_like_options_stay_captions() {
    // local option words are case sensitive, and a year in CJK is no size
    let t = one("x [[File:a.jpg|thumb|Marco]] y\n");
    assert!(t.contains("Marco"), "{t}");
    let t = one("x [[File:a.jpg|thumb|2010\u{5e74}]] y\n");
    assert!(t.contains("2010\u{5e74}"), "{t}");
}

#[test]
fn templated_file_target_with_options() {
    let t = one("x [[{{Localized media|a|svg}}|thumb|Map of the area]] y\n");
    assert!(t.contains("Map of the area") && !t.contains("thumb"), "{t}");
}

#[test]
fn ordinary_links_keep_their_whole_text() {
    let t = one("Go to [[Hilfe:Seite|the links page]] now.\n");
    assert!(t.contains("the links page"), "{t}");
    let t = one("Go to [[Berlin|links]] now.\n");
    assert!(t.contains("links"), "{t}");
}

#[test]
fn hindi_file_namespace_hides_its_options() {
    let t = one("x [[\u{91a}\u{93f}\u{924}\u{94d}\u{930}:a.jpg|\u{905}\u{902}\u{917}\u{942}\u{920}\u{93e}\u{915}\u{93e}\u{930}|\u{926}\u{93e}\u{90f}\u{901}|A small cat]] y\n");
    assert!(t.contains("A small cat") && !t.contains("a.jpg") && !t.contains('|'), "{t}");
}

#[test]
fn external_link_at_the_end_of_a_file_caption() {
    // `]]]`: the first bracket ends the external link, the last two end the file link
    let t = one("[[File:a.png|thumb|Cap from [https://example.org/a Site]]]\n");
    assert!(t.contains("Cap from") && t.contains("Site") && !t.contains(']'), "{t:?}");
}

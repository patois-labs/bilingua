//! Opus gate (11-05): attacks on the writer and the scanner.

mod common;

use bilingua_model::*;
use common::*;
use std::time::Instant;

const HEAD: &str = "<TRADOStag><FrontMatter><UserSettings SourceLanguage=\"EN-US\" TargetLanguage=\"DE-DE\"/></FrontMatter><Body><Raw>";
const TAIL: &str = "</Raw></Body></TRADOStag>";

fn wrap(inner: &str) -> Vec<u8> {
    format!("{HEAD}{inner}{TAIL}").into_bytes()
}

fn s(b: &[u8]) -> String {
    String::from_utf8(b.to_vec()).unwrap()
}

fn reread_units(out: &[u8]) -> usize {
    DEFAULT.extract(out, &opts().with_include_locked(true)).expect("merged file reads back").units.len()
}

#[test]
fn hidden_codes_cannot_be_written_through_a_target() {
    let raw = wrap("<Tu><Tuv Lang=\"EN-US\">Hello <ut>&lt;br/&gt;</ut></Tuv><Tuv Lang=\"DE-DE\"></Tuv></Tu>");
    let mut d = extract_with(&DEFAULT, &raw);
    // set directly (set_target would refuse the invented ids): a caller that bypasses the check
    d.units[0].segments[0].target = vec![
        Inline::text("Hallo"),
        Inline::tag("~raw", TagKind::Standalone, ""),
        Inline::tag("~meta", TagKind::Standalone, ""),
        Inline::tag("~raw", TagKind::Open, ""),
        Inline::tag("~raw", TagKind::Close, ""),
        Inline::tag("1", TagKind::Standalone, ""),
    ];
    let out = DEFAULT.merge(&d).unwrap();
    let text = s(&out);
    assert_eq!(text.matches("<Tu ").count() + text.matches("<Tu>").count(), 1, "{text}");
    assert!(!text.contains('\u{1f}'), "{text}");
    assert!(text.contains("<Tuv Lang=\"DE-DE\">Hallo<ut>&lt;br/&gt;</ut></Tuv>"), "{text}");
    assert_eq!(reread_units(&out), 1);
}

#[test]
fn translations_cannot_make_markup() {
    let raw = wrap("<Tu><Tuv Lang=\"EN-US\">Hello</Tuv></Tu><Tu><Tuv Lang=\"EN-US\">World</Tuv></Tu>");
    let evil = [
        "</Tuv></Tu><Tu><Tuv Lang=\"EN-US\">x</Tuv><Tuv Lang=\"DE-DE\">y",
        "<![CDATA[ a ]]> b ]]> c",
        "&lt; &amp; &#x41; &bogus;",
        "<!DOCTYPE x [<!ENTITY a \"b\">]>",
        "<?xml version=\"1.0\"?>",
        "<!-- c -->",
        "\u{0}\u{1}\u{b}\u{c}\u{fffe}\u{ffff}",
    ];
    for e in evil {
        let mut d = extract_with(&DEFAULT, &raw);
        set(&mut d, 0, e);
        let out = DEFAULT.merge(&d).unwrap();
        assert_eq!(reread_units(&out), 2, "{e:?}: {}", s(&out));
        let back = extract_with(&DEFAULT, &out);
        let t = tok(&back.units[0].segments[0].target);
        if !e.starts_with('\u{0}') {
            assert_eq!(t, e.trim(), "{e:?}");
        }
    }
}

#[test]
fn utf16_without_bom_both_orders_translate() {
    let raw = wrap("<Tu><Tuv Lang=\"EN-US\">Hello \u{1f600} \u{65e5}</Tuv></Tu>");
    for be in [false, true] {
        let data = utf16(&raw, be, false);
        let d = extract_with(&DEFAULT, &data);
        assert_eq!(DEFAULT.merge(&d).unwrap(), data);
        let mut d2 = d.clone();
        set(&mut d2, 0, "Hallo \u{1f601}");
        let out = DEFAULT.merge(&d2).unwrap();
        assert!(!out.starts_with(&[0xFF, 0xFE]) && !out.starts_with(&[0xFE, 0xFF]));
        assert!(decode16(&out, be, false).contains("<Tuv Lang=\"DE-DE\">Hallo \u{1f601}</Tuv>"));
        assert_eq!(reread_units(&out), 1);
    }
}

#[test]
fn locos_mode_skips_finished_segments_default_loads_them() {
    let raw = wrap(
        "<Tu MatchPercent=\"100\"><Tuv Lang=\"EN-US\">Done</Tuv><Tuv Lang=\"DE-DE\">Fertig</Tuv></Tu>\
         <Tu><Tuv Lang=\"EN-US\">Open</Tuv><Tuv Lang=\"DE-DE\"/></Tu>",
    );
    let d = extract_with(&DEFAULT, &raw);
    assert_eq!(d.units.len(), 2);
    assert_eq!(tok(&d.units[0].segments[0].target), "Fertig");
    assert_eq!(d.units[0].segments[0].state, SegState::Translated);
    assert_eq!(DEFAULT.merge(&d).unwrap(), raw);
    let l = extract_with(&LOCOS, &raw);
    assert_eq!(l.units.iter().map(|u| u.context.key.clone().unwrap()).collect::<Vec<_>>(), ["1"]);
    assert_eq!(LOCOS.merge(&l).unwrap(), raw);
    let l = LOCOS.extract(&raw, &opts().with_include_locked(true)).unwrap();
    assert_eq!(l.units.len(), 2);
    assert!(!l.units[0].translate);
    assert_eq!(LOCOS.merge(&l).unwrap(), raw);
}

#[test]
fn a_target_before_the_source_and_three_tuvs() {
    let raw = wrap(
        "<Tu><Tuv Lang=\"DE-DE\"></Tuv><Tuv Lang=\"FR-FR\">Bonjour</Tuv><Tuv Lang=\"EN-US\">Hello</Tuv></Tu>",
    );
    let mut d = extract_with(&DEFAULT, &raw);
    assert_eq!(tok(&d.units[0].source()), "Hello");
    assert!(d.units[0].segments[0].target.is_empty());
    set(&mut d, 0, "Hallo");
    let out = s(&DEFAULT.merge(&d).unwrap());
    assert!(out.contains("<Tuv Lang=\"DE-DE\">Hallo</Tuv><Tuv Lang=\"FR-FR\">Bonjour</Tuv><Tuv Lang=\"EN-US\">Hello</Tuv>"), "{out}");
}

#[test]
fn tag_damage_in_a_target_is_repaired() {
    let raw = wrap(
        "<Tu><Tuv Lang=\"EN-US\">A <df Bold=\"on\">b <ut Type=\"start\">&lt;i&gt;</ut>c<ut Type=\"end\">&lt;/i&gt;</ut></df> d</Tuv></Tu>",
    );
    let d = extract_with(&DEFAULT, &raw);
    let src = d.units[0].source();
    let tags: Vec<Inline> = src.iter().filter(|i| matches!(i, Inline::Tag { .. })).cloned().collect();
    // every order, every duplication: the output is well-formed and reads back with one unit
    let mut orders: Vec<Vec<Inline>> = vec![tags.iter().rev().cloned().collect(), tags.iter().chain(tags.iter()).cloned().collect()];
    orders.push(vec![tags[1].clone(), tags[0].clone(), tags[3].clone(), tags[2].clone()]);
    orders.push(vec![tags[3].clone(), Inline::text("x")]);
    orders.push(vec![tags[0].clone(), Inline::text("x")]);
    for o in orders {
        let mut e = d.clone();
        let mut t = vec![Inline::text("T ")];
        t.extend(o);
        e.units[0].segments[0].target = t;
        let out = DEFAULT.merge(&e).unwrap();
        assert_eq!(reread_units(&out), 1, "{}", s(&out));
    }
}

#[test]
fn many_namespace_declarations_are_not_quadratic() {
    let n = 40_000;
    let decl: String = (0..n).map(|i| format!(" xmlns:p{i}=\"u{i}\"")).collect();
    let used: String = (0..n).map(|i| format!(" p{i}:a=\"v\"")).collect();
    let raw = format!("<TRADOStag{decl}{used}><Body><Tu><Tuv Lang=\"EN-US\">x</Tuv></Tu></Body></TRADOStag>");
    let t = Instant::now();
    let _ = DEFAULT.extract(raw.as_bytes(), &opts());
    assert!(t.elapsed().as_secs_f64() < 5.0, "{:?}", t.elapsed());
}

#[test]
fn wide_inputs_stay_linear() {
    let t = Instant::now();
    // one Tu with very many Tuvs
    let tuvs = "<Tuv Lang=\"XX\">a</Tuv>".repeat(60_000);
    let _ = DEFAULT.extract(&wrap(&format!("<Tu><Tuv Lang=\"EN-US\">x</Tuv>{tuvs}</Tu>")), &opts());
    // one Tuv with very many unmatched start codes and references
    let starts = "<ut Type=\"start\">&lt;b&gt;</ut>&amp;".repeat(30_000);
    let _ = DEFAULT.extract(&wrap(&format!("<Tu><Tuv Lang=\"EN-US\">x{starts}</Tuv></Tu>")), &opts());
    // many '<!' that are not declarations
    let _ = DEFAULT.extract(&wrap(&"<!-- - -->".repeat(100_000)), &opts());
    // many tiny elements nested in df
    let dfs = "<df>a</df>".repeat(100_000);
    let _ = DEFAULT.extract(&wrap(&format!("<Tu><Tuv Lang=\"EN-US\">x{dfs}</Tuv></Tu>")), &opts());
    assert!(t.elapsed().as_secs_f64() < 20.0, "{:?}", t.elapsed());
}

#[test]
fn odd_but_valid_files_round_trip_exactly() {
    for inner in [
        "<Tu><Tuv Lang=\"EN-US\"><!-- c -->Hi<?pi x?> <![CDATA[a<b]]></Tuv><Tuv Lang=\"DE-DE\"/></Tu>",
        "<Tu \r\n MatchPercent = '0' ><Tuv\tLang=\"EN-US\" >Hi</Tuv ><Tuv Lang=\"DE-DE\" /></Tu >",
        "<tu><tuv Lang=\"EN-US\">lower</tuv></tu>",
        "<Tu><Tuv Lang=\"EN-US\">  </Tuv></Tu><Tu/><Tu><Tuv Lang=\"EN-US\">x</Tuv></Tu>",
    ] {
        let raw = wrap(inner);
        let d = extract_with(&DEFAULT, &raw);
        assert_eq!(DEFAULT.merge(&d).unwrap(), raw, "{inner}");
        let mut e = d.clone();
        for u in &d.units {
            e.units.iter_mut().find(|x| x.id == u.id).unwrap().segments[0].target = vec![Inline::text("Z")];
        }
        let out = DEFAULT.merge(&e).unwrap();
        assert_eq!(reread_units(&out), DEFAULT.extract(&raw, &opts().with_include_locked(true)).unwrap().units.len(), "{}", s(&out));
    }
}

#[test]
fn stamp_only_when_translated() {
    let raw = format!("<TRADOStag><FrontMatter><UserSettings SourceLanguage=\"EN-US\" TargetLanguage=\"\"/></FrontMatter><Body>{}</Body></TRADOStag>", "<Tu><Tuv Lang=\"EN-US\">Hello</Tuv></Tu>").into_bytes();
    let d = extract_with(&DEFAULT, &raw);
    assert_eq!(DEFAULT.merge(&d).unwrap(), raw);
    let mut e = d.clone();
    set(&mut e, 0, "Hallo");
    let out = s(&DEFAULT.merge(&e).unwrap());
    assert!(out.contains("TargetLanguage=\"DE-DE\""), "{out}");
    assert!(out.contains("<Tuv Lang=\"DE-DE\">Hallo</Tuv>"), "{out}");
}

//! Regression tests for the findings of the 0.9.1 source audit.

use bilingua_model::*;
use bilingua_wordfast::{Limits, Txlf};
use std::time::{Duration, Instant};

fn opts() -> ExtractOptions {
    ExtractOptions::new().with_target_lang("de-DE")
}

const HEAD: &str = "<xliff version=\"1.2\" xmlns:gs4tr=\"http://www.gs4tr.org/schema/xliff-ext\"";
const FILE: &str = "<file source-language=\"en\" target-language=\"de\" original=\"a\" datatype=\"x\"><body>";
const TAIL: &str = "</body></file></xliff>";

/// A TXLF whose root declares `prefixes` extra namespace prefixes and whose one trans-unit follows `elements` empty
/// prefixed elements.
fn namespaced(prefixes: usize, elements: usize) -> Vec<u8> {
    let mut s = String::from(HEAD);
    s.push_str(" xmlns:first=\"urn:first\"");
    for i in 0..prefixes {
        s.push_str(&format!(" xmlns:p{i}=\"u\""));
    }
    s.push('>');
    s.push_str(FILE);
    for _ in 0..elements {
        s.push_str("<first:x/>");
    }
    s.push_str("<trans-unit id=\"1\"><source>Hello</source></trans-unit>");
    s.push_str(TAIL);
    s.into_bytes()
}

/// `bilingua-xaml/src/scan.rs:lookup:linear-namespace-scan` (the same lookup in `xml.rs`): too many declarations
/// in scope are refused, and under the cap a lookup does not search them all.
#[test]
fn namespaces_in_scope_are_capped_and_indexed() {
    let t = Instant::now();
    let e = Txlf::new().extract(&namespaced(40_000, 40_000), &opts()).expect_err("40 000 namespaces in scope");
    assert!(matches!(&e, Error::Limit(l) if l.kind == LimitKind::XmlElements), "{e:?}");
    let doc = Txlf::new().extract(&namespaced(4_000, 60_000), &opts()).expect("under the cap");
    assert_eq!(doc.units.len(), 1);
    assert!(t.elapsed() < Duration::from_secs(5), "took {:?}", t.elapsed());
}

/// Scoping is as before: a prefix is gone when the element that declared it closes, and an attribute takes the
/// innermost declaration of its prefix.
#[test]
fn namespace_scoping_is_unchanged() {
    let gone = format!("{HEAD}>{FILE}<group xmlns:q=\"urn:q\"/><q:x/><trans-unit id=\"1\"><source>Hello</source></trans-unit>{TAIL}");
    let e = Txlf::new().extract(gone.as_bytes(), &opts()).expect_err("q is out of scope");
    assert!(matches!(&e, Error::Parse { msg, .. } if msg.contains("namespace prefix q is not defined")), "{e:?}");
    // gs4tr rebound to another namespace inside the group: its `locked` is not the gs4tr lock there
    let inner = format!(
        "{HEAD}>{FILE}<group xmlns:gs4tr=\"urn:other\"><trans-unit id=\"1\" gs4tr:locked=\"true\"><source>Inner</source></trans-unit></group><trans-unit id=\"2\" gs4tr:locked=\"true\"><source>Outer</source></trans-unit>{TAIL}"
    );
    let doc = Txlf::new().extract(inner.as_bytes(), &opts().with_include_locked(true)).expect("well formed");
    let locked: Vec<bool> = doc.units.iter().map(|u| !u.translate).collect();
    assert_eq!(locked, [false, true]);
}

fn noted(sources: usize, notes: usize) -> Vec<u8> {
    let mut s = format!("{HEAD}>{FILE}<trans-unit id=\"1\">");
    for _ in 0..sources {
        s.push_str("<source>a</source>");
    }
    for _ in 0..notes {
        s.push_str("<note>n</note>");
    }
    s.push_str("</trans-unit>");
    s.push_str(TAIL);
    s.into_bytes()
}

/// `bilingua-wordfast/src/txlf.rs:walk:notes-cloned-per-source-element`: every `<source>` child of one trans-unit
/// got its own copy of all the notes (2 000 x 2 000 = 4 000 000 notes from a 64 KB file). The bound of 10 000
/// copies now holds per trans-unit, over all its source children.
#[test]
fn notes_are_not_copied_per_source_child() {
    let t = Instant::now();
    let doc = Txlf::new().extract(&noted(2_000, 2_000), &opts()).expect("well formed");
    assert_eq!(doc.units.len(), 2_000);
    let total: usize = doc.units.iter().map(|u| u.notes.len()).sum();
    assert!(total <= 10_000, "{total} notes in the document");
    assert_eq!(doc.units[0].notes.len(), 2_000, "the first unit carries the notes");
    assert!(t.elapsed() < Duration::from_secs(5), "took {:?}", t.elapsed());
    // more notes than the bound: one unit carries them, as with seg-source segments
    let doc = Txlf::new().extract(&noted(50, 10_001), &opts()).expect("well formed");
    assert_eq!(doc.units.iter().map(|u| u.notes.len()).collect::<Vec<_>>()[..2], [10_001, 0]);
    // an ordinary trans-unit is as before: its unit has its notes, and so has every seg-source segment
    let doc = Txlf::new().extract(&noted(1, 3), &opts()).expect("well formed");
    assert_eq!(doc.units.len(), 1);
    assert_eq!(doc.units[0].notes.iter().map(|n| n.text.as_str()).collect::<Vec<_>>(), ["n", "n", "n"]);
    let seg = format!(
        "{HEAD}>{FILE}<trans-unit id=\"1\"><source>One. Two.</source><seg-source><mrk mtype=\"seg\" mid=\"1\">One.</mrk> <mrk mtype=\"seg\" mid=\"2\">Two.</mrk></seg-source><note from=\"pm\">n</note></trans-unit>{TAIL}"
    );
    let doc = Txlf::new().extract(seg.as_bytes(), &opts()).expect("well formed");
    assert_eq!(doc.units.len(), 2);
    assert!(doc.units.iter().all(|u| u.notes.len() == 1 && u.notes[0].author.as_deref() == Some("pm")));
}

/// The unit cap stops the walk (it used to run on the finished list): more source elements than `max_units` is
/// the unit-limit error, and segments that never become units do not count against it.
#[test]
fn the_unit_cap_applies_inside_the_walk() {
    let small = Limits::default().with_max_units(100);
    let e = Txlf::new().extract_with(&noted(101, 2), &opts(), &small).expect_err("101 units");
    assert!(matches!(&e, Error::Limit(l) if l.kind == LimitKind::Units), "{e:?}");
    assert_eq!(Txlf::new().extract_with(&noted(100, 2), &opts(), &small).expect("100 units").units.len(), 100);
    // 150 locked trans-units and 100 open ones: the locked ones are not listed, so the file is within the cap
    let mut s = format!("{HEAD}>{FILE}");
    for i in 0..250 {
        let lock = if i % 5 < 3 { " translate=\"no\"" } else { "" };
        s.push_str(&format!("<trans-unit id=\"{i}\"{lock}><source>a</source></trans-unit>"));
    }
    s.push_str(TAIL);
    assert_eq!(Txlf::new().extract_with(s.as_bytes(), &opts(), &small).expect("100 open units").units.len(), 100);
}

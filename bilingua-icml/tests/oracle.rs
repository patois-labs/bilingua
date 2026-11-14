//! Okapi's own ICML test resource (`Test01.wcml`, Smart Connection v3 layout, 145 KB) against golden lists made
//! with the LocOS filter (exact token text and paragraph numbers) and with Okapi `okf_icml` (text units,
//! normalised like the Okapi oracle: codes dropped, NBSP to space, white space collapsed).

mod common;

use bilingua_icml::Icml;
use bilingua_model::*;
use common::*;

const FILE: &[u8] = include_bytes!("data/okapi_Test01.wcml");
const LOCOS: &str = include_str!("data/okapi_Test01.locos.json");
const OKAPI: &str = include_str!("data/okapi_Test01.okapi.json");

fn norm(s: &str) -> String {
    s.replace('\u{a0}', " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn units_equal_the_locos_filter_exactly() {
    let golden: Vec<serde_json::Value> = serde_json::from_str(LOCOS).unwrap();
    let d = extract(FILE);
    assert_eq!(d.units.len(), golden.len(), "unit count (LocOS 46)");
    for (u, g) in d.units.iter().zip(&golden) {
        assert_eq!(p_index(u) as u64, g["p"].as_u64().unwrap(), "paragraph number");
        assert_eq!(tok_text(u, &u.source()), g["text"].as_str().unwrap(), "unit {}", u.id);
    }
}

#[test]
fn units_equal_okapi_after_normalisation() {
    let golden: Vec<String> = serde_json::from_str(OKAPI).unwrap();
    let d = extract(FILE);
    let ours: Vec<String> = d.units.iter().map(|u| norm(&plain_text(&u.source()))).filter(|s| !s.is_empty()).collect();
    // Okapi lists 37 empty paragraphs as empty units; Bilingua (like LocOS) offers none, so only the 46 with text.
    assert_eq!(ours, golden, "Okapi text units");
}

#[test]
fn inline_codes_are_range_pairs_and_points() {
    let d = extract(FILE);
    let u = &d.units[0];
    let kinds: Vec<(TagKind, String)> = u
        .source()
        .iter()
        .filter_map(|i| match i {
            Inline::Tag { kind, id, .. } => Some((*kind, u.codes[id].label.clone().unwrap_or_default())),
            _ => None,
        })
        .collect();
    assert_eq!(kinds, vec![(TagKind::Open, "run".to_string()), (TagKind::Close, "run".to_string())]);
    assert_eq!(tok_text(u, &u.source()), "Corporate Governance der Siegfried Gruppe\u{27E6}r1\u{27E7}und \u{27E6}/r1\u{27E7}das Schweizerische Obligationenrechts (OR)");
}

#[test]
fn untouched_and_pseudo_translated_round_trip() {
    let d = extract(FILE);
    assert_eq!(Icml.merge(&d).unwrap(), FILE, "untouched merge is the original bytes");
    let mut t = d.clone();
    for u in &mut t.units {
        let mut v = vec![Inline::text("[")];
        v.extend(u.source());
        v.push(Inline::text("]"));
        u.segments[0].target = v;
    }
    let out = Icml.merge(&t).unwrap();
    assert!(well_formed(&out));
    let back = extract(&out);
    assert_eq!(back.units.len(), d.units.len());
    for (a, b) in back.units.iter().zip(&d.units) {
        assert_eq!(tok_text(a, &a.source()), format!("[{}]", tok_text(b, &b.source())), "unit {}", b.id);
    }
    // the StoryInfo wrapper and everything outside the one story is untouched
    let cut = FILE.windows(10).position(|w| w == b"<Document ").unwrap();
    assert_eq!(&out[..cut], &FILE[..cut]);
    let tail_at = FILE.windows(13).rposition(|w| w == b"</ea:Stories>").unwrap();
    assert!(out.ends_with(&FILE[tail_at..]));
}

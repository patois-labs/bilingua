//! Every fixture in `tests/data`: round trips, the LocOS golden list (made with the LocOS venv), Okapi's own
//! `okf_ttx` test resources (Okapi's output frozen in `okapi_golden.json`), the 11 languages and each encoding.

mod common;

use bilingua_model::*;
use common::*;
use std::collections::BTreeMap;

include!("data_list.inc");

fn data(name: &str) -> &'static [u8] {
    FILES.iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("no fixture {name}")).1
}

fn encoding(d: &Document) -> String {
    match &d.skeleton {
        Skeleton::Text { encoding, .. } => encoding.split(';').next().unwrap().to_string(),
        _ => String::new(),
    }
}

fn tag_count(d: &Document) -> usize {
    d.units.iter().map(|u| u.segments[0].source.iter().filter(|i| matches!(i, Inline::Tag { .. })).count()).sum()
}

fn with_locked() -> ExtractOptions {
    opts().with_include_locked(true)
}

#[test]
fn untouched_merge_is_the_original_bytes() {
    for (name, bytes) in FILES {
        for f in [&DEFAULT, &LOCOS] {
            for o in [opts(), with_locked()] {
                let d = f.extract(bytes, &o).unwrap_or_else(|e| panic!("{name}: {e}"));
                assert_eq!(f.merge(&d).unwrap(), bytes, "{name} locked={}", o.include_locked);
            }
        }
    }
}

#[test]
fn pseudo_translation_reads_back_with_the_same_units_and_tags() {
    for (name, bytes) in FILES {
        let before = DEFAULT.extract(bytes, &with_locked()).unwrap();
        let mut d = before.clone();
        for u in &mut d.units {
            if u.translate {
                let mut t = vec![Inline::text("[")];
                t.extend(u.source());
                t.push(Inline::text("]"));
                u.segments[0].target = t;
            }
        }
        let out = DEFAULT.merge(&d).unwrap_or_else(|e| panic!("{name}: {e}"));
        let after = DEFAULT.extract(&out, &with_locked()).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(after.units.len(), before.units.len(), "{name}");
        assert_eq!(tag_count(&after), tag_count(&before), "{name}");
        assert_eq!(encoding(&after), encoding(&before), "{name}: encoding kept");
        for (a, b) in after.units.iter().zip(&before.units) {
            assert_eq!(a.id, b.id, "{name}");
            assert_eq!(a.segments[0].source, b.segments[0].source, "{name}: sources never change");
            if b.translate {
                assert_eq!(tok(&a.segments[0].target), format!("[{}]", tok(&b.segments[0].source)), "{name} {}", a.id);
            }
        }
        // a second untouched round trip of the output is stable too
        assert_eq!(DEFAULT.merge(&after).unwrap(), out, "{name}");
        // a target language is stamped only when a unit was translated
        if !before.units.iter().any(|u| u.translate) {
            assert_eq!(out, bytes, "{name}: nothing to translate, original bytes");
        }
    }
}

#[test]
fn locos_golden_units() {
    let golden: BTreeMap<String, serde_json::Value> = serde_json::from_str(include_str!("data/locos_golden.json")).unwrap();
    assert_eq!(golden.len(), FILES.len());
    let mut matched = 0;
    for (name, bytes) in FILES {
        let want = &golden[&format!("{name}.ttx")];
        let d = LOCOS.extract(bytes, &opts()).unwrap();
        if let Some(err) = want.get("error") {
            // LocOS refuses the file (not segmented, or nothing left to translate): no units here either
            assert!(d.units.is_empty(), "{name}: LocOS refuses with {err}");
            if err == "UnsegmentedTtxError" {
                assert!(d.warnings.iter().any(|w| w.kind == "ttx_not_segmented"), "{name}");
            }
            continue;
        }
        let got: Vec<(i64, String)> = d.units.iter().map(|u| (u.context.key.as_ref().unwrap().parse().unwrap(), tok(&u.source()))).collect();
        let exp: Vec<(i64, String)> = want["units"].as_array().unwrap().iter().map(|p| (p[0].as_i64().unwrap(), p[1].as_str().unwrap().to_string())).collect();
        assert_eq!(got, exp, "{name}");
        matched += 1;
    }
    assert_eq!(matched, 12);
}

/// Okapi's normalisation (as the oracle's `norm`): tags are one marker each, NBSP is a space, white space collapses.
const CODE: char = char::from_u32(0xa4).unwrap();

fn okapi_norm(inl: &[Inline]) -> String {
    let mut s = String::new();
    for i in inl {
        match i {
            Inline::Text(t) => s.push_str(&t.replace('\u{a0}', " ")),
            Inline::Tag { .. } => s.push('\u{a4}'),
        }
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Okapi makes one text unit per paragraph, holding every `<Tu>` of the paragraph as a segment; LocOS (and so
/// Bilingua) makes one unit per `<Tu>`. So Okapi's units must be exactly the runs of consecutive Bilingua units joined.
#[test]
fn okapi_units_are_runs_of_ours() {
    let golden: BTreeMap<String, Vec<serde_json::Value>> = serde_json::from_str(include_str!("data/okapi_golden.json")).unwrap();
    for (name, rows) in &golden {
        // Okapi reads text outside the segments only in a file that has no <Tu> at all
        let bytes = data(name.trim_end_matches(".ttx"));
        let mut d = DEFAULT.extract(bytes, &opts()).unwrap();
        if d.units.is_empty() {
            d = DEFAULT.extract(bytes, &with_locked()).unwrap();
        }
        let mut at = 0;
        let mut with_target = 0;
        for (n, row) in rows.iter().enumerate() {
            // codes at the edges of a paragraph belong to markup outside the segments
            let raw_want = row["text"].as_str().unwrap();
            let edge = raw_want.trim_matches(|c: char| c == CODE || c.is_whitespace());
            let want_t = row["target"].as_str();
            let (mut src, mut tgt) = (Vec::<String>::new(), Vec::<String>::new());
            let mut locked = false;
            loop {
                let u = d.units.get(at).unwrap_or_else(|| panic!("{name}: Okapi unit {n} ({raw_want:?}) not found, ran out at {src:?}"));
                at += 1;
                locked |= !u.translate;
                let s = okapi_norm(&u.segments[0].source);
                if !s.is_empty() {
                    src.push(s);
                }
                let t = okapi_norm(&u.segments[0].target);
                if !t.is_empty() {
                    tgt.push(t);
                }
                // text outside the segments is split by its codes here and joined by them in Okapi
                let multi = src.len() > 1; // markup between two segments is a code of the paragraph in Okapi only
                // (LocOS lists only text runs with a letter, so punctuation-only runs such as "/>" are not ours)
                let squash = |x: &str| {
                    if locked {
                        x.chars().filter(|c| c.is_alphanumeric()).collect::<String>()
                    } else if multi {
                        x.replace(CODE, "").split_whitespace().collect::<Vec<_>>().join("")
                    } else {
                        x.to_string()
                    }
                };
                let ours = src.join(" ");
                let (joined, want) = (squash(ours.trim_matches(|c: char| c == CODE || c.is_whitespace())), squash(edge));
                if joined == want {
                    break;
                }
                let last = n + 1 == rows.len();
                // text between and after the last segments joins the paragraph in Okapi: letters and digits must still agree
                let an = |x: &str| x.chars().filter(|c| c.is_alphanumeric()).collect::<String>();
                let ok = if last { an(&want).starts_with(&an(&joined)) } else { want.starts_with(&joined) };
                assert!(ok, "{name}: Okapi unit {n}
  okapi {want:?}
  ours  {joined:?}");
                if last && at == d.units.len() {
                    break;
                }
            }
            if let Some(wt) = want_t {
                with_target += 1;
                assert_eq!(tgt.join(" "), wt.trim_matches('\u{a4}').trim(), "{name}: target of Okapi unit {n}");
            }
        }
        assert_eq!(at, d.units.len(), "{name}: units we have that Okapi has not");
        if name == "okapi_test01.ttx" {
            assert!(with_target >= 4);
        }
    }
}

#[test]
fn okapi_files_in_locos_mode() {
    // LocOS needs a segmented file with something left to translate; Okapi's resources have neither
    for name in ["okapi_issue_164", "okapi_test01", "okapi_test02_allseg", "okapi_test02_noseg"] {
        let d = LOCOS.extract(data(name), &opts()).unwrap();
        assert!(d.units.is_empty(), "{name}");
    }
    let d = DEFAULT.extract(data("okapi_test01"), &opts()).unwrap();
    assert_eq!(d.units.len(), 6);
    assert_eq!((d.source_lang.as_deref(), d.target_lang.as_deref()), (Some("EN-US"), Some("FR-FR")));
    // Test01 is Trados 8.2 output: a df wraps a start tag whose end sits outside it, so both are point codes
    assert_eq!(tok(&d.units[0].segments[0].source), "Text in ⟦g1⟧⟦x2⟧bold⟦/g1⟧⟦x3⟧.");
    let d = DEFAULT.extract(data("okapi_issue_164"), &opts()).unwrap();
    assert!(d.units.is_empty());
    assert!(d.warnings.iter().any(|w| w.kind == "ttx_not_segmented"));
    assert!(d.warnings.iter().any(|w| w.kind == "ttx_unsegmented_text" && w.message.starts_with("5 ")));
}

#[test]
fn eleven_languages_and_their_encodings() {
    let enc = [
        ("en", "utf-16le"),
        ("de", "utf-8"),
        ("fr", "utf-16be"),
        ("es", "utf-16le-nobom"),
        ("pt", "utf-8-bom"),
        ("ru", "utf-16le"),
        ("tr", "utf-16le"),
        ("ar", "utf-16le"),
        ("ja", "utf-16be"),
        ("zh", "utf-16le"),
        ("ko", "utf-16le-nobom"),
    ];
    for (l, e) in enc {
        let bytes = data(&format!("lang_{l}"));
        let d = extract_with(&DEFAULT, bytes);
        assert_eq!(encoding(&d), e, "{l}");
        assert_eq!(d.units.len(), 4, "{l}");
        assert_eq!(LOCOS.extract(bytes, &opts()).unwrap().units.len(), 3, "{l}");
        let u = &d.units[1].segments[0].source;
        assert!(u.iter().any(|i| matches!(i, Inline::Tag { kind: TagKind::Open, .. })), "{l}: pair tag");
        assert_eq!(tok(&d.units[1].segments[0].source).matches('\u{27E6}').count(), 2, "{l}");
        assert_eq!(tok(&d.units[2].segments[0].source).matches("x1").count(), 1, "{l}");
        assert_eq!(tok(&d.units[3].segments[0].target), "Done & dusted.", "{l}");
        assert_eq!(d.units[3].segments[0].state, SegState::Translated, "{l}");
        // translate in the language itself (RTL and CJK included) and read back
        let mut t = d.clone();
        set(&mut t, 0, &format!("{} \u{645}\u{631}\u{62d}\u{628}\u{627} \u{65e5}\u{672c}\u{8a9e} \u{d55c}\u{ad6d}\u{c5b4} \u{4e2d}\u{6587} \u{1f600}", tok(&d.units[0].segments[0].source)));
        let out = DEFAULT.merge(&t).unwrap();
        let back = extract_with(&DEFAULT, &out);
        assert!(tok(&back.units[0].segments[0].target).ends_with("\u{4e2d}\u{6587} \u{1f600}"), "{l}");
        assert_eq!(encoding(&back), e, "{l}: encoding kept");
        assert_eq!(out[..bom_len(e)], bytes[..bom_len(e)], "{l}: byte-order mark kept");
        // the non-translated source is untouched
        assert_eq!(tok(&back.units[1].segments[0].source), tok(&d.units[1].segments[0].source), "{l}");
    }
}

fn bom_len(e: &str) -> usize {
    match e {
        "utf-8-bom" => 3,
        "utf-16le" | "utf-16be" => 2,
        _ => 0,
    }
}

#[test]
fn locked_units_for_context() {
    let d = DEFAULT.extract(data("locos_suite_sample"), &with_locked()).unwrap();
    let ids: Vec<_> = d.units.iter().map(|u| (u.id.as_str(), u.translate)).collect();
    assert_eq!(ids, [("u1", true), ("u2", true), ("u3", true), ("k1", false), ("u4", true), ("u5", true)]);
    assert_eq!(d.units[3].lock_reason.as_deref(), Some("text outside any segment"));
    let d = LOCOS.extract(data("locos_suite_sample"), &with_locked()).unwrap();
    let c = d.units.iter().find(|u| u.context.key.as_deref() == Some("2")).unwrap();
    assert!(!c.translate && c.lock_reason.as_deref() == Some("already translated"));
    // a locked unit refuses a target
    let mut d2 = d.clone();
    assert!(matches!(d2.set_target(&c.id.clone(), vec![Inline::text("x")]), Err(Error::Locked(_))));
    // a Tu with only a tag is "no translatable text"
    let raw = "<TRADOStag><Body><Raw><Tu><Tuv Lang=\"EN-US\"><ut DisplayText=\"br\">&lt;br&gt;</ut></Tuv></Tu></Raw></Body></TRADOStag>";
    assert!(DEFAULT.extract(raw.as_bytes(), &opts()).unwrap().units.is_empty());
    assert_eq!(DEFAULT.extract(raw.as_bytes(), &with_locked()).unwrap().units[0].lock_reason.as_deref(), Some("no translatable text"));
}

//! Under the LocOS profile a translation may repeat a standalone placeholder (`#` in ICU, Chrome `$x$`, go `{{..}}`,
//! a `<br/>`): `profile::locos_set_target` takes it, and every format's merge writes the placeholder each time.
//! Without the profile `set_target` still refuses the repeat. Paired tags keep the exactly-once rule.

use bilingua::*;
use bilingua_model::profile;
use std::path::PathBuf;

fn corpus() -> Vec<(String, Vec<u8>)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus");
    let mut out = Vec::new();
    for dir in std::fs::read_dir(&root).unwrap().flatten() {
        if !dir.path().is_dir() {
            continue;
        }
        for f in std::fs::read_dir(dir.path()).unwrap().flatten() {
            let name = f.file_name().to_string_lossy().to_string();
            if !name.starts_with('.') {
                out.push((name, std::fs::read(f.path()).unwrap()));
            }
        }
    }
    out.sort();
    out
}

fn count_standalone(inl: &[Inline], display: &str) -> usize {
    inl.iter().filter(|i| matches!(i, Inline::Tag { kind: TagKind::Standalone, display: d, .. } if d == display)).count()
}

/// For every corpus file: repeat the first standalone tag of each unit, merge under the profile, read the result
/// back, and find the placeholder twice.
#[test]
fn every_format_writes_a_repeated_standalone_placeholder_each_time() {
    let mut problems: Vec<String> = Vec::new();
    let (mut checked, mut formats) = (0usize, std::collections::BTreeSet::new());
    for (name, bytes) in corpus() {
        let o = ExtractOptions::new()
            .with_source_lang("en")
            .with_target_lang("de-DE")
            .with_granularity(Granularity::Paragraph)
            .with_profile("locos");
        let Ok(mut doc) = extract(&name, &bytes, &o) else { continue };
        let mut wanted: Vec<(String, String, usize)> = Vec::new(); // unit id, tag display, tags in the source
        for u in doc.units.clone() {
            if !u.translate || u.segments.len() != 1 {
                continue;
            }
            let seg = &u.segments[0];
            let Some(Inline::Tag { id, display, .. }) = seg.source.iter().find(|i| matches!(i, Inline::Tag { kind: TagKind::Standalone, .. })).cloned() else { continue };
            let mut target = seg.source.clone();
            let again = seg.source.iter().find(|i| matches!(i, Inline::Tag { id: t, .. } if *t == id)).cloned().unwrap();
            target.push(Inline::text(" "));
            target.push(again);
            // the plain rule still refuses the repeat
            if doc.clone().set_target(&seg.id, target.clone()).is_ok() {
                problems.push(format!("{name}: plain set_target took a repeated standalone tag in {}", u.id));
            }
            if let Err(e) = profile::locos_set_target(&mut doc, &seg.id, target) {
                problems.push(format!("{name}: locos_set_target refused a repeat in {}: {e}", u.id));
                continue;
            }
            let before = count_standalone(&seg.source, &display);
            wanted.push((u.id.clone(), display, before));
        }
        if wanted.is_empty() {
            continue;
        }
        let m = profile::locos().merge_options(&doc.format);
        let out = match merge_with(&doc, &m) {
            Ok(b) => b,
            Err(e) => {
                problems.push(format!("{name}: merge refused a repeated standalone tag: {e}"));
                continue;
            }
        };
        let back = match extract(&name, &out, &o) {
            Ok(d) => d,
            Err(e) => {
                problems.push(format!("{name}: the merged file does not read back: {e}"));
                continue;
            }
        };
        checked += wanted.len();
        formats.insert(doc.format.clone());
        for (id, display, before) in wanted {
            let Some(u) = back.units.iter().find(|u| u.id == id) else {
                problems.push(format!("{name}: unit {id} is gone after the merge"));
                continue;
            };
            let n = count_standalone(&u.segments.iter().flat_map(|s| s.source.clone()).collect::<Vec<_>>(), &display);
            if n < before.max(1) + 1 {
                problems.push(format!("{name}: unit {id} has the placeholder {display:?} {n} time(s) after the merge, wanted {}", before.max(1) + 1));
            }
        }
    }
    assert!(checked >= 40 && formats.len() >= 10, "too little covered: {checked} units in {formats:?}");
    assert!(problems.is_empty(), "{} problem(s):\n{}", problems.len(), problems.join("\n"));
}

/// Paired tags keep the exactly-once rule under the profile; a tag the source lacks is still refused.
#[test]
fn paired_tags_and_invented_tags_are_still_refused() {
    let o = ExtractOptions::new().with_source_lang("en").with_granularity(Granularity::Paragraph).with_profile("locos");
    let mut doc = extract("a.html", b"<html><body><p>Hello <b>big</b> world<br/>again</p></body></html>", &o).unwrap();
    let seg = doc.units[0].segments[0].clone();
    let paired = seg.source.iter().find(|i| matches!(i, Inline::Tag { kind: TagKind::Open, .. })).cloned().expect("an open tag");
    let mut twice = seg.source.clone();
    twice.push(paired);
    assert!(profile::locos_set_target(&mut doc, &seg.id, twice).is_err());
    let mut invented = seg.source.clone();
    invented.push(Inline::tag("99", TagKind::Standalone, "<hr/>"));
    assert!(profile::locos_set_target(&mut doc, &seg.id, invented).is_err());
    // a repeated <br/> is fine
    let br = seg.source.iter().find(|i| matches!(i, Inline::Tag { kind: TagKind::Standalone, .. })).cloned().expect("a standalone tag");
    let mut again = seg.source.clone();
    again.push(br);
    assert!(profile::locos_set_target(&mut doc, &seg.id, again).is_ok());
}

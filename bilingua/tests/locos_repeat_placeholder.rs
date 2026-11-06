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

/// Formats whose LocOS writer keeps a placeholder once (its `_rebuild_target`, `used` set or `placed` set drops a
/// second use), measured against the LocOS filters on the corpus (11-06): Bilingua matches and drops the repeat
/// too. Server pages also write a tag that carries code once (`<a href="<?php x() ?>">`), on purpose. Every other
/// format writes a repeated standalone placeholder each time, as LocOS does.
const LOCOS_KEEPS_ONCE: &[&str] = &[
    "android", "aspx", "ass", "dita", "docbook", "docx", "erb", "icml", "idml", "jsp", "mif", "phtml", "shtml", "ttml", "ttx", "txlf", "txml", "vsdx", "vtt",
    "xlz", "xmb", "xml", "xslt", "xtb", "xtg", "wsxz", "php", "ascx", "jspf",
];

/// For every corpus file: translate each unit as itself (the baseline) and as itself with its first standalone tag
/// written twice, merge both under the profile: the repeat must be accepted, written (the output differs and is
/// longer) and the result must read back.
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
        let Ok(mut base) = extract(&name, &bytes, &o) else { continue };
        let mut rep = base.clone();
        let fmt = base.format.clone();
        let mut repeated = 0usize;
        for u in base.units.clone() {
            if !u.translate || u.segments.len() != 1 {
                continue;
            }
            let seg = &u.segments[0];
            let Some(at) = seg.source.iter().position(|i| matches!(i, Inline::Tag { kind: TagKind::Standalone, .. })) else { continue };
            let mut twice = seg.source.clone();
            twice.insert(at, seg.source[at].clone());
            // the plain rule still refuses the repeat
            if rep.clone().set_target(&seg.id, twice.clone()).is_ok() {
                problems.push(format!("{name} [{fmt}]: plain set_target took a repeated standalone tag in {}", u.id));
            }
            if let Err(e) = profile::locos_set_target(&mut base, &seg.id, seg.source.clone()) {
                problems.push(format!("{name} [{fmt}]: locos_set_target refused the source of {}: {e}", u.id));
                continue;
            }
            if let Err(e) = profile::locos_set_target(&mut rep, &seg.id, twice) {
                problems.push(format!("{name} [{fmt}]: locos_set_target refused a repeat in {}: {e}", u.id));
                continue;
            }
            repeated += 1;
        }
        if repeated == 0 {
            continue;
        }
        let m = profile::locos().merge_options(&base.format);
        let Ok(out_base) = merge_with(&base, &m) else { continue };
        let out_rep = match merge_with(&rep, &m) {
            Ok(b) => b,
            Err(e) => {
                problems.push(format!("{name} [{fmt}]: merge refused a repeated standalone tag: {e}"));
                continue;
            }
        };
        checked += repeated;
        formats.insert(base.format.clone());
        if out_rep == out_base && !LOCOS_KEEPS_ONCE.contains(&fmt.as_str()) {
            problems.push(format!("{name} [{fmt}]: {repeated} repeated placeholder(s) were not written (the output is the baseline's, {} bytes)", out_base.len()));
            continue;
        }
        if extract(&name, &out_base, &o).is_ok() {
            if let Err(e) = extract(&name, &out_rep, &o) {
                problems.push(format!("{name} [{fmt}]: the merged file does not read back: {e}"));
            }
        }
    }
    assert!(checked >= 100 && formats.len() >= 20, "too little covered: {checked} units in {formats:?}");
    let mut by_format: std::collections::BTreeMap<String, usize> = Default::default();
    for p in &problems {
        let f = p.split('[').nth(1).and_then(|r| r.split(']').next()).unwrap_or("?").to_string();
        *by_format.entry(f).or_default() += 1;
    }
    assert!(problems.is_empty(), "{} problem(s) by format {by_format:?}:\n{}", problems.len(), problems.join("\n"));
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

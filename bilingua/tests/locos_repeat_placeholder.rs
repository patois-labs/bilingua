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
/// second use), measured against the LocOS filters on the corpus (11-06 gate: each unit translated as `Q` + its
/// text, with and without its first `⟦xN⟧` written twice, LocOS `build` compared): Bilingua drops the repeat too.
/// Qt Linguist (`ts`) is one of them (LocOS `render_inner` drops a duplicated token), and so are the memoQ and Trados
/// packages (`mqxlz`, `sdlppx`: LocOS writes their XLIFF through `cat_xliff._render`, which drops a used token).
/// Server pages also write a tag that carries code once (`<a href="<?php x() ?>">`), on purpose, where LocOS writes
/// such a tag twice. The Xcode bundle (`xcloc`) under the profile writes its members with the LocOS XLIFF writer,
/// which drops a used token, as LocOS's xcloc build does (measured on the corpus bundles, 11-07). Every other format but those in [`LOCOS_REFUSES`] writes a repeated standalone placeholder each
/// time, as LocOS does.
const LOCOS_KEEPS_ONCE: &[&str] = &[
    "android", "aspx", "ass", "dita", "docbook", "docx", "erb", "icml", "idml", "jsp", "mif", "mqxlz", "odt", "phtml", "sdlppx", "shtml", "ts", "ttml",
    "ttx", "txlf", "txml", "vsdx", "vtt", "xcloc", "xlz", "xmb", "xml", "xslt", "xtb", "xtg",
];

/// Formats whose LocOS writer refuses a repeated placeholder: the Contentful, Sanity Portable Text and Jira ADF
/// walkers (`contentful_rich_text._rebuild`, `sanity_portable_text._rebuild_children`, `adf._rebuild_inline`) want
/// the translation's tokens to be the source's exactly (a sorted list compare), so a doubled `⟦xN⟧` marks the unit
/// bad and the connector keeps the field's stored value (11-06, measured on the corpus files). Bilingua's merge
/// refuses the repeat for them. Notion is not here: its units are plain text runs, with no tags to repeat.
const LOCOS_REFUSES: &[&str] = &["adf", "contentful", "sanity"];

/// For every corpus file: translate each unit as `Q ` + its source (the baseline) and the same with its first
/// standalone tag written twice, merge both under the profile: the repeat must be accepted, written (the output
/// differs from the baseline's) in every format but those in [`LOCOS_KEEPS_ONCE`], dropped (the baseline's output)
/// in those, refused in those in [`LOCOS_REFUSES`], and the result must read back. The baseline is a changed
/// translation too, so a writer that re-renders a changed unit (escaping, white space) does not pass for one that
/// writes the repeat.
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
            let mut changed = vec![Inline::text("Q ")];
            changed.extend(seg.source.iter().cloned());
            let mut twice = changed.clone();
            twice.insert(at + 1, seg.source[at].clone());
            // the plain rule still refuses the repeat
            if rep.clone().set_target(&seg.id, twice.clone()).is_ok() {
                problems.push(format!("{name} [{fmt}]: plain set_target took a repeated standalone tag in {}", u.id));
            }
            if let Err(e) = profile::locos_set_target(&mut base, &seg.id, changed) {
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
        let refuses = LOCOS_REFUSES.contains(&fmt.as_str());
        let out_rep = match merge_with(&rep, &m) {
            Ok(_) if refuses => {
                problems.push(format!("{name} [{fmt}]: LocOS refuses a repeated placeholder, but the merge wrote it"));
                continue;
            }
            Err(_) if refuses => {
                checked += repeated;
                formats.insert(base.format.clone());
                continue;
            }
            Ok(b) => b,
            Err(e) => {
                problems.push(format!("{name} [{fmt}]: merge refused a repeated standalone tag: {e}"));
                continue;
            }
        };
        checked += repeated;
        formats.insert(base.format.clone());
        let once = LOCOS_KEEPS_ONCE.contains(&fmt.as_str());
        if out_rep == out_base && !once {
            problems.push(format!("{name} [{fmt}]: {repeated} repeated placeholder(s) were not written (the output is the baseline's, {} bytes)", out_base.len()));
            continue;
        }
        if out_rep != out_base && once {
            problems.push(format!("{name} [{fmt}]: LocOS keeps a placeholder once, but the output with a repeat differs from the baseline's"));
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

/// Cases compared with the LocOS filters (11-06, their `build` on the same translations): a repeated `#` in an ICU
/// plural, Chrome `$NAME$` and go `{{.Count}}` are accepted and written each time.
#[test]
fn repeats_are_written_as_locos_writes_them() {
    use bilingua_model::profile::tokens::{from_locos_text, to_locos_text};
    let o = ExtractOptions::new().with_source_lang("en").with_granularity(Granularity::Paragraph).with_profile("locos");
    for (name, src, units, translations, want) in [
        (
            "a.arb",
            r##"{"@@locale": "en", "n": "{c, plural, one{# item} other{# items}}"}"##,
            vec!["\u{27E6}x1\u{27E7} item", "\u{27E6}x1\u{27E7} items"],
            vec!["\u{27E6}x1\u{27E7} \u{27E6}x1\u{27E7} Stuck", "\u{27E6}x1\u{27E7} und \u{27E6}x1\u{27E7} Stucke"],
            r##"{"@@locale": "en", "n": "{c, plural, one{# # Stuck} other{# und # Stucke}}"}"##,
        ),
        (
            "a.json",
            r##"{"hi": {"message": "Hello $NAME$", "placeholders": {"name": {"content": "$1"}}}}"##,
            vec!["Hello \u{27E6}x1\u{27E7}"],
            vec!["Hallo \u{27E6}x1\u{27E7} \u{27E6}x1\u{27E7}"],
            r##"{"hi": {"message": "Hallo $NAME$ $NAME$", "placeholders": {"name": {"content": "$1"}}}}"##,
        ),
        (
            "b.json",
            r##"[{"id": "n", "translation": {"one": "{{.Count}} file", "other": "{{.Count}} files"}}]"##,
            vec!["\u{27E6}x1\u{27E7} file", "\u{27E6}x1\u{27E7} files"],
            vec!["\u{27E6}x1\u{27E7} \u{27E6}x1\u{27E7} Datei", "\u{27E6}x1\u{27E7} Dateien"],
            r##"[{"id": "n", "translation": {"one": "{{.Count}} {{.Count}} Datei", "other": "{{.Count}} Dateien"}}]"##,
        ),
    ] {
        let mut doc = extract(name, src.as_bytes(), &o).unwrap();
        let mut texts = Vec::new();
        let mut sets = Vec::new();
        for (u, t) in doc.units.iter().zip(&translations) {
            let (text, map) = to_locos_text(&u.segments[0].source);
            texts.push(text);
            sets.push((u.segments[0].id.clone(), from_locos_text(t, &map)));
        }
        assert_eq!(texts, units, "{name}: unit texts");
        for (id, inl) in sets {
            profile::locos_set_target(&mut doc, &id, inl).unwrap_or_else(|e| panic!("{name}: {e}"));
        }
        let out = merge_with(&doc, &profile::locos().merge_options(&doc.format)).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), want, "{name}");
    }
}

/// A go-i18n file with a string `@@locale` opens as go-i18n under the LocOS profile (LocOS checks go-i18n before
/// ARB); without the profile it keeps opening as ARB, and a real ARB file stays ARB under both.
#[test]
fn a_go_i18n_file_with_an_arb_locale_follows_locos_under_the_profile() {
    let go = br#"{"@@locale": "en", "files": {"one": "{{.Count}} file", "other": "{{.Count}} files"}, "hi": "Hello"}"#;
    let plain = ExtractOptions::new().with_source_lang("en").with_granularity(Granularity::Paragraph);
    assert_eq!(extract("go.json", go, &plain).unwrap().format, "arb");
    let locos = plain.clone().with_profile("locos");
    let doc = extract("go.json", go, &locos).unwrap();
    assert_eq!(doc.format, "json");
    // LocOS (dump on the same bytes): the @@locale string is a message too, then the two plural forms, then hi
    let texts: Vec<String> = doc.units.iter().map(|u| bilingua_model::profile::tokens::to_locos_text(&u.segments[0].source).0).collect();
    assert_eq!(texts, ["en", "\u{27E6}x1\u{27E7} file", "\u{27E6}x1\u{27E7} files", "Hello"]);
    assert!(merge(&doc).is_ok());
    let arb = br#"{"@@locale": "en", "hi": "Hello {name}", "@hi": {"placeholders": {"name": {}}}}"#;
    assert_eq!(extract("a.json", arb, &plain).unwrap().format, "arb");
    assert_eq!(extract("a.json", arb, &locos).unwrap().format, "arb");
    assert_eq!(bilingua::detect_with("go.json", go, Some("locos")).unwrap().id(), "json");
    assert_eq!(bilingua::detect_with("go.json", go, None).unwrap().id(), "arb");
}

/// A message that is only a literal `⟦x1⟧` has no unit under the LocOS profile (LocOS strips the tokens before it
/// looks for letters); without the profile it is text like any other.
#[test]
fn an_arb_message_of_only_a_token_has_no_unit_under_the_profile() {
    let src = "{\"@@locale\": \"en\", \"a\": \"\u{27E6}x1\u{27E7}\", \"b\": \"\u{27E6}x2\u{27E7} \u{27E6}x1\u{27E7}\", \"c\": \"Hi \u{27E6}x1\u{27E7}\", \"d\": \"\u{27E6}x\u{27E7}\"}";
    let plain = ExtractOptions::new().with_source_lang("en").with_granularity(Granularity::Paragraph);
    let keys = |d: &Document| d.units.iter().filter_map(|u| u.context.key.clone()).collect::<Vec<_>>();
    assert_eq!(keys(&extract("a.arb", src.as_bytes(), &plain).unwrap()), ["a", "b", "c", "d"]);
    let doc = extract("a.arb", src.as_bytes(), &plain.with_profile("locos")).unwrap();
    assert_eq!(keys(&doc), ["c", "d"]);
}

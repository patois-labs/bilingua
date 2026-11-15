//! Resource limits (security review run 2): the project reader's target-language set, attribute names and
//! namespace prefixes are looked up through indexes, and a project names a bounded number of target languages.

use bilingua_model::*;
use bilingua_sdlproj::{project_info, MAX_TARGET_LANGS};
use std::time::{Duration, Instant};

fn directions(n: usize) -> Vec<u8> {
    let mut s = String::from("<PackageProject Guid=\"g\">");
    for i in 0..n {
        s.push_str(&format!("<LanguageDirection SourceLanguageCode=\"en\" TargetLanguageCode=\"t{i:05}\"/>"));
    }
    s.push_str("</PackageProject>");
    s.into_bytes()
}

/// Every new target was compared with every earlier one (two allocations per comparison); a file of tens of
/// thousands of directions is now refused, and the count is checked through a set.
#[test]
fn too_many_target_languages_are_refused() {
    let r = project_info(&directions(5000));
    assert!(matches!(&r, Err(Error::Limit(e)) if e.kind == LimitKind::Units), "{r:?}");
}

#[test]
fn many_target_languages_up_to_the_cap_still_open() {
    let info = project_info(&directions(MAX_TARGET_LANGS)).expect("open");
    assert_eq!(info.target_langs.len(), MAX_TARGET_LANGS);
    // the same language twice in another case is still named once
    let twice = b"<Project><LanguageDirection SourceLanguageCode=\"en\" TargetLanguageCode=\"de-DE\"/><LanguageDirection SourceLanguageCode=\"EN\" TargetLanguageCode=\"de-de\"/></Project>";
    assert_eq!(project_info(twice).expect("open").target_langs, vec!["de-DE".to_string()]);
}

/// A root with tens of thousands of attributes (each a namespace declaration) above as many prefixed children:
/// the duplicate check and every prefix lookup were linear scans, quadratic in all.
#[test]
fn wide_tag_and_many_namespaces_are_linear() {
    let k = 50_000;
    let mut s = String::from("<Project");
    for i in 0..k {
        s.push_str(&format!(" xmlns:p{i}=\"u{i}\""));
    }
    s.push('>');
    s.push_str(&"<p0:x/>".repeat(k));
    s.push_str("<LanguageDirection SourceLanguageCode=\"en\" TargetLanguageCode=\"de\"/></Project>");
    let start = Instant::now();
    let info = project_info(s.as_bytes()).expect("open");
    assert_eq!(info.target_langs, vec!["de".to_string()]);
    assert!(start.elapsed() < Duration::from_secs(10), "took {:?}", start.elapsed());
    // a duplicate in a wide tag is still found
    let dup = format!("<Project{} a=\"1\" a=\"2\"><LanguageDirection SourceLanguageCode=\"en\" TargetLanguageCode=\"de\"/></Project>", (0..20).map(|i| format!(" b{i}=\"x\"")).collect::<String>());
    assert!(matches!(project_info(dup.as_bytes()), Err(Error::Parse { msg, .. }) if msg.contains("twice")));
}

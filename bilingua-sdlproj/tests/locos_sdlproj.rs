//! The sdlproj cases of the LocOS suite `tests/verify_sdl_package_filter.py` (3 of its 8 test functions touch the
//! manifest: `test_inspect`, `test_member_blob_and_identity`, `test_hostile`; the other 5 are zip, database, web
//! and CLI tests that belong to `bilingua-sdl` and the app) and the `.sdlproj` checks of
//! `app/filters/sdl_package.py`. Fixture: `demo.sdlproj` of the suite, as `tests/data/locos_demo.sdlproj`.

mod common;

use bilingua_model::guard::LimitKind;
use bilingua_model::*;
use bilingua_sdlproj::{flip_package_type, project_info, Sdlproj, MAX_SDLPROJ_BYTES};
use common::*;

const PROJ: &[u8] = include_bytes!("data/locos_demo.sdlproj");

// test_inspect: the language pair(s), GUID, package type and project name of the manifest
#[test]
fn inspect_reads_languages_guid_type_and_name() {
    let i = project_info(PROJ).unwrap();
    assert_eq!(i.source_lang, "en-US");
    assert_eq!(i.target_langs, ["de-DE", "fr-FR"]);
    assert_eq!(i.package_guid, "c0ffee00-1234-4abc-9def-001122334455");
    assert_eq!(i.package_type, "ProjectPackage");
    assert_eq!(i.name, "demo");
    assert_eq!(i.root, "PackageProject");
}

#[test]
fn the_document_carries_the_languages_and_no_units() {
    let d = extract(PROJ);
    assert_eq!(d.format, "sdlproj");
    assert_eq!(d.source_lang.as_deref(), Some("en-US"));
    assert_eq!(d.target_lang.as_deref(), Some("de-DE"));
    assert_eq!(d.name.as_deref(), Some("demo"));
    // LocOS has no filter for a loose .sdlproj and extracts no text from the manifest of a package
    assert!(d.units.is_empty());
    let all = Sdlproj.extract(PROJ, &ExtractOptions { include_locked: true, ..opts() }).unwrap();
    assert!(all.units.is_empty());
}

// test_member_blob_and_identity: the manifest differs ONLY by PackageType in a return package
#[test]
fn identity_and_package_type_flip() {
    assert_eq!(Sdlproj.merge(&extract(PROJ)).unwrap(), PROJ);
    let flipped = flip_package_type(PROJ);
    let expect = String::from_utf8(PROJ.to_vec()).unwrap().replace("PackageType=\"ProjectPackage\"", "PackageType=\"ReturnPackage\"");
    assert_eq!(flipped, expect.as_bytes());
    assert_eq!(project_info(&flipped).unwrap().package_type, "ReturnPackage");
    // the flip touches the root element only, never a nested or absent attribute
    assert_eq!(flip_package_type(b"<Project Name=\"x\"/>"), b"<Project Name=\"x\"/>");
    let nested = b"<PackageProject PackageType=\"ReturnPackage\"><X PackageType=\"ProjectPackage\"/></PackageProject>";
    assert_eq!(flip_package_type(nested), nested);
    // single quotes and spaces around the equals sign
    assert_eq!(flip_package_type(b"<PackageProject PackageType = 'ProjectPackage' Name=\"a\">"), b"<PackageProject PackageType = 'ReturnPackage' Name=\"a\">");
    // not a word boundary: MyPackageType is another attribute
    assert_eq!(flip_package_type(b"<PackageProject MyPackageType=\"ProjectPackage\">"), b"<PackageProject MyPackageType=\"ProjectPackage\">");
}

// test_hostile: DOCTYPE, no language direction, not a Trados project file
#[test]
fn hostile_manifests_are_refused() {
    let doctype = b"<?xml version=\"1.0\"?><!DOCTYPE x [<!ENTITY a \"aaaa\">]><PackageProject>&a;</PackageProject>";
    assert!(matches!(Sdlproj.extract(doctype, &opts()), Err(Error::Limit(l)) if l.kind == LimitKind::XmlEntities));
    // a DOCTYPE with no entity is refused too (LocOS refuses every DOCTYPE)
    let plain = b"<?xml version=\"1.0\"?><!DOCTYPE PackageProject><PackageProject><LanguageDirection SourceLanguageCode=\"en\" TargetLanguageCode=\"de\"/></PackageProject>";
    assert!(err(plain).contains("DOCTYPE"));
    let nolang = b"<PackageProject PackageType=\"ProjectPackage\"><LanguageDirections/></PackageProject>";
    assert!(err(nolang).contains("no language direction"));
    assert!(err(b"<Other/>").contains("not a Trados"));
}

// sdl_package.inspect: one source language per project; case-insensitive comparison; unusable pairs skipped
#[test]
fn one_source_language_and_unique_targets() {
    let two = b"<Project><LanguageDirection SourceLanguageCode=\"en-US\" TargetLanguageCode=\"de-DE\"/><LanguageDirection SourceLanguageCode=\"fr-FR\" TargetLanguageCode=\"de-DE\"/></Project>";
    assert!(err(two).contains("more than one source language"));
    let i = project_info(include_bytes!("data/edge.sdlproj")).unwrap();
    assert_eq!(i.source_lang, "en-US", "the first pair's spelling, trimmed");
    assert_eq!(i.target_langs, ["de-DE", "fr-FR"], "DE-de is the same target; an empty target is no pair");
    assert_eq!(i.root, "PackageProject", "a prefixed root is read by its local name");
    assert_eq!(i.package_guid, "g-1", "PackageGuid wins over Guid and is trimmed");
    assert_eq!(i.name, "A & B \u{e9} <c>");
    let only_guid = b"<Project Guid=\"abc\"><LanguageDirection SourceLanguageCode=\"en\" TargetLanguageCode=\"de\"/></Project>";
    assert_eq!(project_info(only_guid).unwrap().package_guid, "abc");
}

#[test]
fn attribute_values_are_cut_to_the_locos_lengths() {
    let long = "x".repeat(300);
    let xml = format!("<Project Guid=\"{long}\" PackageType=\"{long}\" Name=\"{long}\"><LanguageDirection SourceLanguageCode=\"en\" TargetLanguageCode=\"de\"/></Project>");
    let i = project_info(xml.as_bytes()).unwrap();
    assert_eq!((i.package_guid.len(), i.package_type.len(), i.name.len()), (64, 40, 200));
}

// MAX_SDLPROJ_BYTES: 16 MB
#[test]
fn a_manifest_over_16_mb_is_refused() {
    let head = b"<Project><LanguageDirection SourceLanguageCode=\"en\" TargetLanguageCode=\"de\"/><!--".to_vec();
    let mut big = head.clone();
    big.resize(MAX_SDLPROJ_BYTES + 1, b' ');
    big.extend_from_slice(b"--></Project>");
    assert!(matches!(Sdlproj.extract(&big, &opts()), Err(Error::Limit(l)) if l.kind == LimitKind::InputBytes));
    // just under the cap reads
    let mut ok = head;
    ok.resize(MAX_SDLPROJ_BYTES - 20, b' ');
    ok.extend_from_slice(b"--></Project>");
    assert!(ok.len() <= MAX_SDLPROJ_BYTES);
    assert!(Sdlproj.extract(&ok, &opts()).is_ok());
}

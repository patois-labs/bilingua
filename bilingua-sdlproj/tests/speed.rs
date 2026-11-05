//! A generated project file of more than 5 MB. The real timing (release build, against the LocOS filter) is
//! `examples/sdlproj_dump.rs`; this test keeps the size and the round trip honest in every build.

mod common;

use bilingua_model::Filter;
use bilingua_sdlproj::{project_info, Sdlproj};
use common::*;

pub fn generate(min_bytes: usize) -> String {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n<PackageProject Guid=\"7d1c2a40-5b6e-4f70-8a91-0b2c3d4e5f60\" PackageGuid=\"c0ffee00-1234-4abc-9def-001122334455\" PackageType=\"ProjectPackage\" Name=\"big\">\r\n  <LanguageDirections>\r\n    <LanguageDirection TargetLanguageCode=\"de-DE\" SourceLanguageCode=\"en-US\" />\r\n    <LanguageDirection TargetLanguageCode=\"ja-JP\" SourceLanguageCode=\"en-US\" />\r\n  </LanguageDirections>\r\n  <ProjectFiles>\r\n");
    let mut i = 0usize;
    while s.len() < min_bytes {
        s.push_str(&format!("    <ProjectFile Guid=\"99999999-0000-4000-8000-{i:012}\" Name=\"file{i}.docx\" Path=\"\" Role=\"Translatable\" FilterDefinitionId=\"WordprocessingML v. 2\">\r\n      <LanguageFiles>\r\n        <LanguageFile Guid=\"a{i}\" Path=\"de-DE\\\" LanguageCode=\"de-DE\"><FileVersions><FileVersion VersionNumber=\"1\" FileName=\"file{i}.docx.sdlxliff\" PhysicalPath=\"de-DE\\file{i}.docx.sdlxliff\" /></FileVersions></LanguageFile>\r\n      </LanguageFiles>\r\n    </ProjectFile>\r\n"));
        i += 1;
    }
    s.push_str("  </ProjectFiles>\r\n</PackageProject>\r\n");
    s
}

#[test]
fn five_megabytes_round_trip() {
    let data = generate(5_200_000).into_bytes();
    assert!(data.len() > 5_000_000);
    let i = project_info(&data).unwrap();
    assert_eq!(i.target_langs, ["de-DE", "ja-JP"]);
    let d = extract(&data);
    assert_eq!(Sdlproj.merge(&d).unwrap(), data);
}

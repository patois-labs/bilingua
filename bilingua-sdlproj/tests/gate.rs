//! Gate checks (11-05): XML hardening, legacy encodings that must write back exactly, the manifest flip against
//! LocOS's regex, merge options, and a sniff that never claims another format's file.

mod common;

use bilingua_model::*;
use bilingua_sdlproj::{flip_package_type, project_info, Sdlproj};
use common::*;
use std::path::Path;

include!("data_list.inc");

const LD: &str = "<LanguageDirection SourceLanguageCode=\"en-US\" TargetLanguageCode=\"de-DE\"/>";

fn proj(attrs: &str) -> String {
    format!("<PackageProject{attrs}>{LD}</PackageProject>")
}

fn utf16le(s: &str) -> Vec<u8> {
    let mut v = vec![0xFF, 0xFE];
    for u in s.encode_utf16() {
        v.extend_from_slice(&u.to_le_bytes());
    }
    v
}

#[test]
fn doctype_is_refused_in_every_encoding_and_shape() {
    let bodies = [
        format!("<!DOCTYPE PackageProject>{}", proj("")),
        format!("<!DOCTYPE PackageProject [<!ENTITY e \"x\">]>{}", proj(" Name=\"&e;\"")),
        format!("<!DOCTYPE PackageProject SYSTEM \"file:///etc/passwd\">{}", proj("")),
        format!("<!DOCTYPE PackageProject [<!ENTITY % p SYSTEM \"http://x/\"> %p;]>{}", proj("")),
        format!("<?xml version=\"1.0\"?>\n<!DOCTYPE PackageProject [<!ENTITY a \"aaaa\"><!ENTITY b \"&a;&a;&a;\">]>{}", proj(" Name=\"&b;\"")),
    ];
    for b in &bodies {
        assert!(Sdlproj.extract(b.as_bytes(), &opts()).is_err(), "utf-8: {b}");
        assert!(Sdlproj.extract(&utf16le(b), &opts()).is_err(), "utf-16: {b}");
        assert!(project_info(b.as_bytes()).is_err(), "{b}");
    }
}

#[test]
fn only_predefined_entities_and_xml_character_references() {
    let ok = proj(" Name=\"&lt;&gt;&amp;&quot;&apos;&#65;&#x42;&#x10FFFF;\"");
    assert_eq!(project_info(ok.as_bytes()).unwrap().name, "<>&\"'AB\u{10FFFF}");
    for bad in ["&nbsp;", "&#0;", "&#xD800;", "&#xFFFE;", "&#x110000;", "&#;", "&#x;", "&amp", "&#12a;"] {
        let s = proj(&format!(" Name=\"{bad}\""));
        assert!(Sdlproj.extract(s.as_bytes(), &opts()).is_err(), "attribute {bad}");
        let s = format!("<PackageProject>{bad}{LD}</PackageProject>");
        assert!(Sdlproj.extract(s.as_bytes(), &opts()).is_err(), "text {bad}");
    }
}

#[test]
fn depth_is_capped_by_the_model_limit() {
    let l = Limits::default();
    let at = |n: usize| format!("<PackageProject>{}{LD}{}</PackageProject>", "<a>".repeat(n), "</a>".repeat(n));
    assert!(Sdlproj.extract(at(l.max_xml_depth - 1).as_bytes(), &opts()).is_ok());
    assert!(matches!(Sdlproj.extract(at(l.max_xml_depth + 1).as_bytes(), &opts()), Err(Error::Limit(_))));
    let small = Limits::new().with_max_xml_depth(4);
    assert!(matches!(Sdlproj.extract_with(at(4).as_bytes(), &opts(), &small), Err(Error::Limit(_))));
}

#[test]
fn undeclared_non_utf8_is_refused() {
    for byte in [0xE9u8, 0x80, 0xFF, 0xC3] {
        let mut data = b"<PackageProject Name=\"Caf".to_vec();
        data.push(byte);
        data.extend_from_slice(format!("\">{LD}</PackageProject>").as_bytes());
        assert!(matches!(Sdlproj.extract(&data, &opts()), Err(Error::Encoding(_)) | Err(Error::Parse { .. })), "byte {byte:02X}");
        assert!(project_info(&data).is_err(), "byte {byte:02X}");
        // the same bytes with a declaration naming the code page read
        let mut decl = b"<?xml version=\"1.0\" encoding=\"windows-1252\"?>".to_vec();
        decl.extend_from_slice(&data);
        let d = Sdlproj.extract(&decl, &opts()).unwrap_or_else(|e| panic!("declared {byte:02X}: {e}"));
        assert_eq!(Sdlproj.merge(&d).unwrap(), decl);
    }
    // a declaration that says UTF-8 does not excuse bytes that are not UTF-8
    let mut lie = b"<?xml version=\"1.0\" encoding=\"utf-8\"?><PackageProject Name=\"Caf".to_vec();
    lie.push(0xE9);
    lie.extend_from_slice(format!("\">{LD}</PackageProject>").as_bytes());
    assert!(matches!(Sdlproj.extract(&lie, &opts()), Err(Error::Encoding(_))));
    assert!(project_info(&lie).is_err());
    // bytes that are not valid in the declared legacy encoding are not read with a fallback code page either
    let mut bad = b"<?xml version=\"1.0\" encoding=\"shift_jis\"?><PackageProject Name=\"".to_vec();
    bad.push(0xA0);
    bad.extend_from_slice(format!("\">{LD}</PackageProject>").as_bytes());
    assert!(matches!(Sdlproj.extract(&bad, &opts()), Err(Error::Encoding(_))));
}

#[test]
fn legacy_bytes_that_would_not_write_back_are_refused() {
    // Shift_JIS 0xED 0x40 (an NEC-selected IBM extension) decodes to U+7E8A, which encodes back as 0xFA 0x5C
    let mut sjis = b"<?xml version=\"1.0\" encoding=\"shift_jis\"?><PackageProject Name=\"".to_vec();
    sjis.extend_from_slice(&[0xED, 0x40]);
    sjis.extend_from_slice(format!("\">{LD}</PackageProject>").as_bytes());
    assert!(matches!(Sdlproj.extract(&sjis, &opts()), Err(Error::Encoding(_))), "{:?}", Sdlproj.extract(&sjis, &opts()).map(|_| ()));
    // the canonical bytes for the same character read and write back exactly
    let mut canon = sjis.clone();
    let at = canon.windows(2).position(|w| w == [0xED, 0x40]).unwrap();
    canon[at] = 0xFA;
    canon[at + 1] = 0x5C;
    let d = Sdlproj.extract(&canon, &opts()).unwrap();
    assert_eq!(Sdlproj.merge(&d).unwrap(), canon);
}

#[test]
fn every_legacy_fixture_writes_back_exactly_under_every_merge_option() {
    let all = [
        MergeOptions::default(),
        bilingua_model::profile::locos().merge_options("xml"),
        MergeOptions::new().with_output_encoding(OutputEncoding::Utf8).with_stamp_target_always(false),
        MergeOptions::new().with_output_encoding(OutputEncoding::default()).with_stamp_target_always(true),
    ];
    for (name, data) in FILES {
        let d = Sdlproj.extract(data, &opts().with_target_lang("ja-JP")).unwrap();
        for o in &all {
            assert_eq!(Sdlproj.merge_with(&d, o).unwrap(), data, "{name}: {o:?}");
        }
    }
}

/// LocOS `flip_package_type`, written out by hand from `app/filters/sdl_package.py`:
/// `end = find(">", find("PackageProject") or 0)`; `head = data[:end + 1]` (empty when there is no `>`), and the
/// first `\bPackageType\s*=\s*(["'])ProjectPackage\1` in head becomes `ReturnPackage`. Python's bytes `\b` and
/// `\s` are ASCII.
fn locos_flip(data: &[u8]) -> Vec<u8> {
    let find = |n: &[u8], from: usize| data.get(from..).and_then(|h| h.windows(n.len()).position(|w| w == n)).map(|p| p + from);
    let start = find(b"PackageProject", 0).unwrap_or(0);
    let head_len = find(b">", start).map_or(0, |e| e + 1);
    let (head, tail) = data.split_at(head_len);
    let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let ws = |b: u8| matches!(b, b' ' | b'\t' | b'\r' | b'\n' | 0x0B | 0x0C);
    for i in 0..head.len() {
        if !head[i..].starts_with(b"PackageType") || (i > 0 && word(head[i - 1])) {
            continue;
        }
        let mut j = i + 11;
        while j < head.len() && ws(head[j]) {
            j += 1;
        }
        if head.get(j) != Some(&b'=') {
            continue;
        }
        j += 1;
        while j < head.len() && ws(head[j]) {
            j += 1;
        }
        let Some(&q) = head.get(j).filter(|q| **q == b'"' || **q == b'\'') else { continue };
        if head[j + 1..].starts_with(b"ProjectPackage") && head.get(j + 15) == Some(&q) {
            let mut out = head[..j + 1].to_vec();
            out.extend_from_slice(b"ReturnPackage");
            out.extend_from_slice(&head[j + 15..]);
            out.extend_from_slice(tail);
            return out;
        }
    }
    data.to_vec()
}

#[test]
fn flip_matches_locos_and_changes_only_the_attribute_value() {
    let mut cases: Vec<Vec<u8>> = FILES.iter().map(|(_, d)| d.to_vec()).collect();
    for c in [
        &b"PackageType=\"ProjectPackage\""[..],                        // no '>' at all: LocOS leaves it alone
        b"<PackageProject PackageType=\"ProjectPackage\"",               // truncated root tag
        b"<PackageProject \xC3\xA9PackageType=\"ProjectPackage\">",      // non-ASCII before the name is a boundary in Python
        b"<PackageProject _PackageType=\"ProjectPackage\">",
        b"<PackageProject PackageType=\"ProjectPackage'>",
        b"<PackageProject PackageType\x0B=\x0C\"ProjectPackage\">",
        b"<?xml version=\"1.0\"?><!-- PackageType=\"ProjectPackage\" --><PackageProject PackageType=\"ProjectPackage\">",
        b"<Project PackageType=\"ProjectPackage\"><PackageProject/></Project>",
        b"<Project PackageType=\"ProjectPackage\"/>",
        b"<?xml version=\"1.0\"?><Project PackageType=\"ProjectPackage\"/>",
        b"<PackageProject Name=\"a>b\" PackageType=\"ProjectPackage\">",
        b"<PackageProject PackageType=\"ProjectPackage\" PackageType=\"ProjectPackage\">",
        b"",
        b">",
    ] {
        cases.push(c.to_vec());
    }
    for c in &cases {
        let got = flip_package_type(c);
        assert_eq!(got, locos_flip(c), "{:?}", String::from_utf8_lossy(c));
        if got != *c {
            // exactly one value changed: same bytes before it, same bytes after it
            let p = c.windows(14).position(|w| w == b"ProjectPackage").unwrap();
            let q = got.windows(13).position(|w| w == b"ReturnPackage").unwrap();
            assert!(q <= p);
            assert_eq!(&got[..q], &c[..q]);
            assert_eq!(&got[q + 13..], &c[q + 14..]);
        }
    }
    let demo = fixture(&FILES, "locos_demo");
    assert_ne!(flip_package_type(demo), demo);
    assert_eq!(flip_package_type(&flip_package_type(demo)), flip_package_type(demo));
}

#[test]
fn sniff_never_claims_another_format() {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus");
    let mut seen = 0;
    let mut stack = vec![corpus];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            let data = std::fs::read(&p).unwrap();
            if p.extension().is_some_and(|x| x == "sdlproj") {
                // a prefixed root (`<p:PackageProject`, edge.sdlproj) is left to the extension: the sniff is tight
                let prefixed = data.windows(16).any(|w| w == b":PackageProject ");
                assert_eq!(Sdlproj.sniff(&data), !prefixed, "{}", p.display());
            } else {
                assert!(!Sdlproj.sniff(&data), "{}", p.display());
            }
            seen += 1;
        }
    }
    assert!(seen > 100, "corpus went missing: {seen}");
    // MSBuild and other Project roots
    for other in [
        "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><TargetFramework>net8.0</TargetFramework></PropertyGroup></Project>",
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n<Project ToolsVersion=\"15.0\" DefaultTargets=\"Build\" xmlns=\"http://schemas.microsoft.com/developer/msbuild/2003\">\r\n<PropertyGroup><ProjectGuid>{0A1B2C3D-4E5F-4A6B-8C7D-9E0F1A2B3C4D}</ProjectGuid></PropertyGroup><ItemGroup><ProjectReference Include=\"..\\a.csproj\"><Project>{11111111-2222-4333-8444-000000000001}</Project></ProjectReference></ItemGroup></Project>",
        "<Project DefaultTargets=\"Build\" xmlns=\"http://schemas.microsoft.com/developer/msbuild/2003\"><PropertyGroup><Name>Db</Name><ProjectGuid>{x}</ProjectGuid><DSP>Microsoft.Data.Tools.Schema.Sql.Sql160DatabaseSchemaProvider</DSP></PropertyGroup></Project>",
        "<Project xmlns=\"http://maven.apache.org/POM/4.0.0\"><modelVersion>4.0.0</modelVersion><groupId>x</groupId></Project>",
        "<project name=\"x\" default=\"build\"><target name=\"build\"/></project>",
        "<xliff version=\"1.2\"><file original=\"a\" source-language=\"en\"><body><trans-unit id=\"1\"><source>&lt;PackageProject</source></trans-unit></body></file></xliff>",
        "# PackageProject Guid= LanguageDirection",
    ] {
        assert!(!Sdlproj.sniff(other.as_bytes()), "{other}");
    }
}

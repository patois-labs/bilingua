//! Port of the resx / resw cases of the Localization OS suite `tests/verify_xml_resource_filters.py`:
//! `test_registration` (resx part), `test_resx`, `test_encoding_and_line_endings`, `test_hostile_inputs` (every
//! check, run on a resx), `test_upload_translate_download_end_to_end` (resx part) and the resx entry of
//! `tests/verify_keys_bridge.py`. Same inputs, same expected bytes.

mod common;

use bilingua_model::*;
use bilingua_resx::{Limits, Resx};
use common::*;

const STRINGS: &[u8] = include_bytes!("data/Strings.resx");

fn s(b: &[u8]) -> &str {
    std::str::from_utf8(b).unwrap()
}

fn refused(data: &[u8], needle: &str) {
    let e = Resx.extract(data, &opts()).expect_err("expected a refusal").to_string().to_lowercase();
    assert!(e.contains(&needle.to_lowercase()), "{needle:?} not in {e:?}");
}

#[test]
fn registration() {
    assert_eq!(Resx.id(), "resx");
    assert!(Resx.extensions().contains(&"resx") && Resx.extensions().contains(&"resw"));
    assert!(Resx.sniff(STRINGS));
    assert!(!Resx.sniff(b"<resources><string name=\"a\">x</string></resources>"));
    assert!(!Resx.sniff(b"{\"a\": 1}"));
}

#[test]
fn units_and_skips() {
    // file order: Greeting, Padded, okButton.Text; Logo, Readme (typed), >>okButton.Name and Empty are skipped
    let mut got = units(STRINGS);
    assert_eq!(got, ["Hello, {0}! You have {1} new messages.", "Save & close", "OK, continue"]);
    got.sort();
    assert_eq!(got, ["Hello, {0}! You have {1} new messages.", "OK, continue", "Save & close"]);
    assert_identity(STRINGS);
}

#[test]
fn translated_merge_changes_only_the_values_and_keeps_padding() {
    let out = translate(STRINGS, &[(0, "Hallo, {0}! You have {1} new messages."), (1, "Sichern & <ok> & close")]);
    only_changed(
        STRINGS,
        &out,
        &["Hello, {0}! You have {1} new messages.", "Save &amp; close"],
        &["Hallo, {0}! You have {1} new messages.", "Sichern &amp; &lt;ok&gt; &amp; close"],
    );
    assert!(s(&out).contains("<value>  Sichern &amp; &lt;ok&gt; &amp; close  </value>"), "padding kept");
}

#[test]
fn records_are_the_key_view() {
    let recs = Resx.records(STRINGS).unwrap();
    let by = |k: &str| recs.iter().find(|r| r.key == k).unwrap_or_else(|| panic!("no record {k}"));
    assert_eq!(by("Greeting").comment, "{0} is the user name, {1} a count");
    assert!(!by("Logo").translatable && !by("Readme").translatable);
    assert!(!by(">>okButton.Name").translatable && by("okButton.Text").translatable);
    assert_eq!(by("Padded").value, "  Save & close  ");
    assert_eq!(by("okButton.Text").unit_index, Some(2));
}

#[test]
fn resw_is_the_same_format_and_keeps_bom_and_crlf() {
    let mut resw = vec![0xEF, 0xBB, 0xBF];
    resw.extend_from_slice(s(STRINGS).replace('\n', "\r\n").as_bytes());
    assert_identity(&resw);
    let out = translate(&resw, &[(0, "Hallo, {0}! You have {1} new messages."), (1, "Sichern & <ok> & close")]);
    assert!(out.starts_with(&[0xEF, 0xBB, 0xBF]));
    let crlf = |b: &[u8]| b.windows(2).filter(|w| w == b"\r\n").count();
    assert_eq!(crlf(&out), crlf(&resw));
    assert!(!out.windows(2).any(|w| w[1] == b'\n' && w[0] != b'\r'), "no bare LF");
    assert_eq!(units(&out).len(), 3);
}

#[test]
fn a_multi_line_translation_in_a_crlf_file_gets_crlf() {
    let resx = s(STRINGS).replace('\n', "\r\n");
    let out = translate(resx.as_bytes(), &[(1, "line one\nline two")]);
    assert!(!out.windows(2).any(|w| w[1] == b'\n' && w[0] != b'\r'), "bare LF in a CRLF file");
    assert!(String::from_utf8_lossy(&out).contains("line one\r\nline two"));
}

#[test]
fn utf16_upload_keeps_its_encoding() {
    // LocOS converts to UTF-8; Bilingua writes the file back the way it came (UTF-16 with its BOM and declaration)
    let text = s(STRINGS).replace("encoding=\"utf-8\"", "encoding=\"utf-16\"");
    for big in [false, true] {
        let mut data = if big { vec![0xFE, 0xFF] } else { vec![0xFF, 0xFE] };
        for u in text.encode_utf16() {
            data.extend_from_slice(&if big { u.to_be_bytes() } else { u.to_le_bytes() });
        }
        let d = extract(&data);
        assert_eq!(d.units.len(), 3);
        assert_identity(&data);
        let out = translate(&data, &[(0, "Hallo {0}")]);
        assert!(out.starts_with(if big { &[0xFE, 0xFF] } else { &[0xFF, 0xFE] }));
        assert_eq!(units(&out)[0], "Hallo {0}");
    }
}

#[test]
fn hostile_inputs() {
    let lol = b"<?xml version=\"1.0\"?><!DOCTYPE root [<!ENTITY a \"aaaaaaaaaa\"><!ENTITY b \"&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;\">]>\
<root><data name=\"x\"><value>&b;</value></data></root>";
    refused(lol, "dtd");
    // a ">" inside a quoted system id must not hide the internal subset
    let hidden = b"<?xml version=\"1.0\"?><!DOCTYPE root SYSTEM \"a>b\" [<!ENTITY a \"x\">]>\
<root><data name=\"x\" y=\"&a;\"><value>hi</value></data></root>";
    refused(hidden, "dtd");
    let xxe = b"<?xml version=\"1.0\"?><!DOCTYPE root SYSTEM \"file:///etc/passwd\">\
<root><data name=\"x\"><value>&xxe;</value></data></root>";
    refused(xxe, "entity");
    refused(b"export const x: number = 1;\n", "not well-formed");
    refused(b"<?xml version=\"1.0\"?><catalog><item>Hello</item></catalog>", "<root>");
    let mut deep = b"<root><data name=\"x\"><value>".to_vec();
    deep.extend(b"<b>".repeat(3000));
    deep.push(b't');
    deep.extend(b"</b>".repeat(3000));
    deep.extend_from_slice(b"</value></data></root>");
    refused(&deep, "deep");
    // the size budget (LocOS: 300 elements against a cap of 5 segments)
    let mut big = b"<root>".to_vec();
    big.extend(b"<x/>".repeat(300));
    big.extend_from_slice(b"</root>");
    let small = Limits::new().with_max_xml_elements(200);
    let e = Resx.extract_with(&big, &opts(), &small).unwrap_err().to_string();
    assert!(e.contains("elements"), "{e}");
    let many = Limits::new().with_max_units(2);
    let e = Resx.extract_with(STRINGS, &opts(), &many).unwrap_err().to_string();
    assert!(e.contains("segments"), "{e}");
}

#[test]
fn stray_cdata_end_in_a_translation_falls_back_to_escaped_text() {
    let data = b"<root><data name=\"a\"><value><![CDATA[Read <this>]]></value></data></root>";
    assert_eq!(units(data), ["Read <this>"]);
    assert_identity(data);
    let out = translate(data, &[(0, "a ]]> b")]);
    assert_eq!(s(&out), "<root><data name=\"a\"><value>a ]]&gt; b</value></data></root>");
    let out = translate(data, &[(0, "Lies <das>")]);
    assert_eq!(s(&out), "<root><data name=\"a\"><value><![CDATA[Lies <das>]]></value></data></root>");
}

#[test]
fn end_to_end_translate_keeps_header_schema_and_non_text() {
    let d = extract(STRINGS);
    let out = Resx.merge(&pseudo(&d)).unwrap();
    let text = s(&out);
    let src = s(STRINGS);
    let head = src.split("<data name=\"Greeting\"").next().unwrap();
    assert!(text.starts_with(head), "header and schema stay byte-identical");
    assert!(text.contains("Qk02AAAAAAAAADYAAAAoAAAAAQAAAAEAAAABABgAAAAAAAAAAADEDgAAxA4AAAAAAAAAAAAA"));
    assert!(text.contains("{0}") && text.contains("{1}"));
    assert_eq!(Resx.extract(&out, &opts()).unwrap().units.len(), d.units.len());
    assert!(text.contains("[Save &amp; close]"));
}

#[test]
fn keys_bridge_sample() {
    let data = b"<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<root>\n  <data name=\"Hello\" xml:space=\"preserve\">\n    <value>Hello</value>\n    <comment>Greeting</comment>\n  </data>\n</root>\n";
    let recs = Resx.records(data).unwrap();
    assert_eq!(recs.len(), 1);
    assert_eq!((recs[0].key.as_str(), recs[0].value.as_str(), recs[0].comment.as_str()), ("Hello", "Hello", "Greeting"));
    assert_identity(data);
}

#[test]
fn placeholders_are_tags_that_flatten_to_the_locos_text() {
    let d = extract(STRINGS);
    let u = &d.units[0];
    let n = u.source().iter().filter(|i| matches!(i, Inline::Tag { .. })).count();
    assert_eq!(n, 2);
    for (id, c) in u.codes.iter().filter(|(k, _)| !k.starts_with('~')) {
        assert_eq!(c.label.as_deref(), Some("placeholder"), "{id}");
    }
    assert_eq!(show(&u.source()), "Hello, {0}! You have {1} new messages.");
    assert!(d.units[1].source().iter().all(|i| matches!(i, Inline::Text(_))));
}

#[test]
fn sniff_survives_an_8_kb_cut_inside_a_multi_byte_character() {
    for pad in 0..4 {
        let mut doc = format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<root>{}<data name=\"a\"><value>x</value></data>\n", " ".repeat(pad)).into_bytes();
        while doc.len() < 9000 {
            doc.extend_from_slice("<!-- \u{8bed}\u{8a00} -->\n".as_bytes());
        }
        doc.extend_from_slice(b"</root>");
        assert!(Resx.sniff(&doc), "pad {pad}");
    }
}

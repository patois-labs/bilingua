//! Port of the Localization OS suite `tests/verify_xaml_filter.py`: 9 of its 11 cases. The other two are
//! `test_registration` (the LocOS registry, upload list and Graph map) and `test_api_and_cli_round_trip` (the
//! LocOS web API and CLI); the facade contract test and the oracle cover their Bilingua counterparts.
//! Same inputs, same expected bytes. The fixtures are the suite's own (`tests/data/locos_suite_*.xaml`).

mod common;

use bilingua_model::*;
use bilingua_xaml::{Limits, Xaml};
use common::*;

const WPF: &[u8] = include_bytes!("data/locos_suite_wpf.xaml");
const MAUI: &[u8] = include_bytes!("data/locos_suite_maui.xaml");
const VIEW: &[u8] = include_bytes!("data/locos_suite_view.xaml");

fn s(b: &[u8]) -> String {
    String::from_utf8(b.to_vec()).unwrap()
}

fn refused(f: &Xaml, data: &[u8], needle: &str) -> String {
    let e = f.extract(data, &opts()).expect_err("expected a refusal").to_string();
    assert!(e.to_lowercase().contains(&needle.to_lowercase()), "{needle}: {e}");
    e
}

fn well_formed(out: &[u8]) {
    ON.extract(out, &opts()).expect("output is still well-formed XAML");
}

#[test]
fn fixtures_are_the_suite_fixtures() {
    assert!(WPF.starts_with(b"<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n<!-- Strings for the main window -->\r\n"));
    assert!(MAUI.starts_with(b"<ResourceDictionary xmlns=\"http://schemas.microsoft.com/dotnet/2021/maui\"\n"));
    assert!(VIEW.starts_with(b"<Window x:Class=\"App.Main\" Title=\"Main window\"\n"));
}

#[test]
fn wpf_resources() {
    let texts = units(WPF);
    assert_eq!(texts, ["Hello, {0}!", "Save & close", "First line A<b>"]);
    assert_identity(WPF);
    let out = translate(WPF, &[(0, "Hallo, {0}!"), (1, "Speichern & schließen"), (2, "Erste <Zeile>")]);
    let exp = s(WPF)
        .replace(">Hello, {0}!<", ">Hallo, {0}!<")
        .replace(">Save &amp; close<", ">Speichern &amp; schließen<")
        .replace("First line &#x41;&lt;b&gt;", "Erste &lt;Zeile&gt;");
    assert_eq!(s(&out), exp);
    well_formed(&out);
    assert_eq!(units(&out), ["Hallo, {0}!", "Speichern & schließen", "Erste <Zeile>"]);
}

#[test]
fn multiline_crlf_translation() {
    let out = s(&translate(WPF, &[(2, "Zeile eins\nZeile zwei")]));
    assert!(out.contains("Zeile eins\r\nZeile zwei"), "{out}");
}

#[test]
fn maui_x_string_and_placeholders() {
    assert_eq!(units(MAUI), ["Welcome", "%1$s items"]);
    let out = translate(MAUI, &[(1, "%1$s Elemente")]);
    assert_eq!(s(&out), s(MAUI).replace("%1$s items", "%1$s Elemente"));
    // the placeholders are tags that may be moved, not text
    let d = extract(MAUI);
    assert!(d.units[1].source().iter().any(|i| matches!(i, Inline::Tag { display, free: true, .. } if display == "%1$s")));
}

#[test]
fn view_attributes_off_by_default() {
    assert!(!Xaml::default().view_attributes);
    assert_eq!(units(VIEW), ["OK"]);
    // a pure view with the option off has nothing to translate (LocOS refuses it with a hint; Bilingua returns
    // an empty document and the caller decides)
    let pure: &[u8] = b"<Window xmlns=\"http://schemas.microsoft.com/winfx/2006/xaml/presentation\" Title=\"T\"/>";
    assert!(extract(pure).units.is_empty());
    assert_eq!(extract_with(&ON, pure).units.len(), 1);
}

#[test]
fn view_attributes_on() {
    let want = ["OK", "Main window", "Click me", "Saves \"all\"", "{0} files", "Settings"];
    assert_eq!(units_with(&ON, VIEW), want);
    assert_identity_with(&ON, VIEW);
    let map = [
        (0, "Gut"),
        (1, "Hauptfenster"),
        (2, "Klick & mich"),
        (3, "Speichert 'alles' \"ganz\""),
        (4, "{0} Dateien"),
        (5, "{Einstellungen}"),
    ];
    let out = s(&translate_with(&ON, VIEW, &map));
    assert!(out.contains("Title=\"Hauptfenster\""));
    assert!(out.contains("Content=\"Klick &amp; mich\""));
    assert!(out.contains("ToolTip='Speichert &apos;alles&apos; \"ganz\"'"), "{out}");
    assert!(out.contains("Text=\"{}{0} Dateien\""));
    // a translation starting with a brace gets the {} escape: never a markup extension
    assert!(out.contains("Header=\"{}{Einstellungen}\""), "{out}");
    assert!(out.contains("Text=\"{Binding Status}\"") && out.contains("Tag=\"Not translated\""));
    assert!(out.contains(">Gut</sys:String>"));
    well_formed(out.as_bytes());
    // the option is a property of the extraction: resource strings keep their ids and indices with it on or off,
    // and a merge of a document extracted with it on is the same whatever the filter instance is set to
    let off = extract(VIEW);
    let on = extract_with(&ON, VIEW);
    assert_eq!(off.units[0].id, on.units[0].id);
    let mut d = on.clone();
    let id = d.units[0].segments[0].id.clone();
    d.set_target(&id, vec![Inline::text("Gut")]).unwrap();
    let id = d.units[1].segments[0].id.clone();
    d.set_target(&id, vec![Inline::text("Hauptfenster")]).unwrap();
    let a = OFF.merge(&d).unwrap();
    let b = ON.merge(&d).unwrap();
    assert_eq!(a, b);
    let t = s(&a);
    assert!(t.contains(">Gut</sys:String>") && t.contains("Title=\"Hauptfenster\""));
}

#[test]
fn encodings_and_bom() {
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend_from_slice(WPF);
    assert_eq!(units(&bom)[0], "Hello, {0}!");
    assert_identity(&bom);
    let out = translate(&bom, &[(0, "Hallo")]);
    assert!(out.starts_with(&[0xEF, 0xBB, 0xBF]) && s(&out[3..]).contains(">Hallo<"));
    for (mark, le) in [([0xFFu8, 0xFE], true), ([0xFE, 0xFF], false)] {
        let enc = |t: &str| -> Vec<u8> {
            let mut v = mark.to_vec();
            for u in t.encode_utf16() {
                v.extend_from_slice(&if le { u.to_le_bytes() } else { u.to_be_bytes() });
            }
            v
        };
        let u16 = enc(&s(MAUI));
        let d = extract(&u16);
        assert_eq!(units(&u16), ["Welcome", "%1$s items"]);
        assert!(matches!(&d.skeleton, Skeleton::Text { encoding, .. } if encoding == if le { "utf-16le" } else { "utf-16be" }));
        assert_identity(&u16);
        let out = translate(&u16, &[(0, "Willkommen ✓")]);
        assert_eq!(out, enc(&s(MAUI).replace(">Welcome<", ">Willkommen ✓<")));
    }
    // a declared legacy charset is re-encoded the same way; what it cannot hold becomes a character reference
    let mut legacy = b"<?xml version=\"1.0\" encoding=\"windows-1252\"?>\n".to_vec();
    legacy.extend(encoding_rs::WINDOWS_1252.encode(&s(MAUI).replace("Welcome", "Café")).0.iter());
    assert_eq!(units(&legacy)[0], "Café");
    assert_identity(&legacy);
    let out = translate(&legacy, &[(0, "Café ✓")]);
    assert!(out.windows(15).any(|w| w == b">Caf\xe9 &#10003;<"), "{}", String::from_utf8_lossy(&out));
    // UTF-8 files carry the plain tag
    assert!(matches!(&extract(MAUI).skeleton, Skeleton::Text { encoding, .. } if encoding == "utf-8"));
    // undeclared non-UTF-8 is refused, never mojibake
    let bad = encoding_rs::WINDOWS_1252.encode(&s(MAUI).replace("Welcome", "Café")).0.into_owned();
    refused(&OFF, &bad, "UTF-8");
}

#[test]
fn hostile_inputs() {
    let xxe = b"<?xml version=\"1.0\"?><!DOCTYPE r [<!ENTITY x SYSTEM \"file:///c:/windows/win.ini\">]>\
<ResourceDictionary xmlns:x=\"http://schemas.microsoft.com/winfx/2006/xaml\">\
<x:String x:Key=\"a\">&x;</x:String></ResourceDictionary>";
    refused(&OFF, xxe, "DOCTYPE");
    refused(&OFF, b"<!DOCTYPE l [<!ENTITY a \"aaaa\"><!ENTITY b \"&a;&a;&a;\">]><r>&b;</r>", "DOCTYPE");
    refused(&OFF, b"<ResourceDictionary><x:String>", "well-formed");
    refused(&OFF, b"<a>&undefined;</a>", "well-formed");
    assert!(OFF.extract(b"\x00\x01binary", &opts()).is_err());
    let small = Limits { max_units: 3, ..Limits::default() };
    let many = format!(
        "<ResourceDictionary xmlns:x=\"http://schemas.microsoft.com/winfx/2009/xaml\">{}</ResourceDictionary>",
        (0..5).map(|i| format!("<x:String x:Key=\"k{i}\">Value {i}</x:String>")).collect::<String>()
    );
    let e = OFF.extract_with(many.as_bytes(), &opts(), &small).unwrap_err();
    assert!(matches!(e, Error::Unsupported(m) if m.contains("limit")));
    let flood = format!("<a>{}</a>", "<b/>".repeat(200));
    assert!(matches!(OFF.extract_with(flood.as_bytes(), &opts(), &small), Err(Error::Unsupported(_))));
    let chars = Limits { max_unit_chars: 1000, ..Limits::default() };
    let big = format!(
        "<ResourceDictionary xmlns:x=\"http://schemas.microsoft.com/winfx/2009/xaml\"><x:String x:Key=\"k\">{}</x:String></ResourceDictionary>",
        "word ".repeat(400)
    );
    assert!(matches!(OFF.extract_with(big.as_bytes(), &opts(), &chars), Err(Error::Unsupported(m)) if m.contains("limit")));
}

#[test]
fn illegal_xml_chars_in_translation() {
    let out = translate(MAUI, &[(0, "Bad\u{1}char\u{b}break")]);
    well_formed(&out);
    assert!(out.windows(15).any(|w| w == b">Badchar\nbreak<"), "{}", String::from_utf8_lossy(&out));
}

#[test]
fn namespace_resolution() {
    // a String in some other namespace, or an unprefixed String, is not a resource string
    let data = b"<ResourceDictionary xmlns=\"http://schemas.microsoft.com/winfx/2006/xaml/presentation\"\
\x20xmlns:x=\"http://schemas.microsoft.com/winfx/2006/xaml\"\
\x20xmlns:my=\"clr-namespace:MyApp\" xmlns:s=\"using:System\">\
<my:String x:Key=\"a\">Not me</my:String><String>Nor me</String>\
<s:String x:Key=\"b\">Uwp string</s:String>\
<Grid xmlns:s=\"clr-namespace:Other\"><s:String>Rebound</s:String></Grid>\
</ResourceDictionary>";
    assert_eq!(units(data), ["Uwp string"]);
}

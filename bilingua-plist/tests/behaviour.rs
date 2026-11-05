//! Behaviour beyond the LocOS suite: what the writer keeps byte for byte, how translations are escaped, locked
//! units, sniffing, the plural fill's corner cases and the text-encoding envelope.
mod common;
use bilingua_model::*;
use bilingua_plist::{plist_records, stringsdict_records, Plist, Stringsdict};
use common::*;

const HEAD: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";

fn plist(body: &str) -> Vec<u8> {
    format!("{HEAD}<plist version=\"1.0\">{body}</plist>\n").into_bytes()
}

fn s(b: &[u8]) -> String {
    String::from_utf8(b.to_vec()).unwrap()
}

#[test]
fn everything_outside_the_translated_text_is_kept_byte_for_byte() {
    let src = format!(
        "{HEAD}<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<?custom pi?>\n<!-- top -->\n<plist version=\"1.0\">\n<dict>\n\t<!-- c1 -->\n\t<key  >CFBundleName</key  >\n   <string   >Hello  &amp;  <![CDATA[<x>]]> there</string  >\n\t<key>Other</key><integer> 7 </integer>\n\t<key>NSCameraUsageDescription</key>\n\n\t<string>  Padded &#x41; text\n\twith lines  </string>\n</dict>\n</plist>\n\n\n"
    );
    let mut d = ex(F::Plist, src.as_bytes());
    assert_eq!(texts(&d), ["Hello  &  <x> there", "Padded A text\n\twith lines"]);
    set_nth(&mut d, 0, "Hallo & <Welt>");
    set_nth(&mut d, 1, "Gepolstert");
    let out = s(&merged(F::Plist, &d));
    let expect = src.replace("Hello  &amp;  <![CDATA[<x>]]> there", "Hallo &amp; &lt;Welt&gt;").replace("Padded &#x41; text\n\twith lines", "Gepolstert");
    assert_eq!(out, expect);
}

#[test]
fn translations_are_escaped_and_cleaned() {
    let src = plist("<dict><key>CFBundleName</key><string>Name</string></dict>");
    let mut d = ex(F::Plist, &src);
    set_nth(&mut d, 0, "A & B < C > D ]]> E \u{1}\u{b}F \u{fffe}");
    let out = s(&merged(F::Plist, &d));
    assert!(out.contains("<string>A &amp; B &lt; C &gt; D ]]&gt; E \nF </string>"), "{out}");
    assert_eq!(units_of(F::Plist, out.as_bytes()), ["A & B < C > D ]]> E \nF"]);
    // astral and RTL characters are written as they are
    let mut d = ex(F::Plist, &src);
    set_nth(&mut d, 0, "\u{1F600} \u{5d0}\u{5d1}");
    assert!(s(&merged(F::Plist, &d)).contains("<string>\u{1F600} \u{5d0}\u{5d1}</string>"));
}

#[test]
fn crlf_files_keep_crlf_for_new_line_breaks() {
    let src = plist("\n<dict>\n<key>CFBundleName</key>\n<string>Name</string>\n</dict>\n").iter().flat_map(|b| if *b == b'\n' { vec![b'\r', b'\n'] } else { vec![*b] }).collect::<Vec<u8>>();
    let mut d = ex(F::Plist, &src);
    set_nth(&mut d, 0, "Line one\nLine two");
    let out = s(&merged(F::Plist, &d));
    assert!(out.contains("<string>Line one\r\nLine two</string>"), "{out:?}");
    assert!(!out.replace("\r\n", "").contains('\n'));
}

#[test]
fn locked_units_show_what_is_not_translated() {
    let src = plist("<dict><key>CFBundleIdentifier</key><string>com.example.app</string><key>CFBundleName</key><string>Name</string><key>CFBundleVersion</key><string>12</string><key>Empty</key><string/><key>Blank</key><string>  </string></dict>");
    let o = ExtractOptions::new().with_include_locked(true);
    let mut d = ex_with(F::Plist, &src, &o);
    // empty and blank values have no text to show
    let ids: Vec<(&str, bool)> = d.units.iter().map(|u| (u.id.as_str(), u.translate)).collect();
    assert_eq!(ids, [("s1", false), ("s2", true), ("s3", false)]);
    assert_eq!(d.units[0].lock_reason.as_deref(), Some("not a translatable key"));
    assert_eq!(d.units[0].context.key.as_deref(), Some("CFBundleIdentifier"));
    assert_eq!(d.units[1].context.style.as_deref(), Some("CFBundleName"));
    assert!(matches!(d.set_target("s1", vec![Inline::text("x")]), Err(Error::Locked(_))));
    d.set_target("s2", vec![Inline::text("Neu")]).unwrap();
    assert!(s(&merged(F::Plist, &d)).contains("<string>Neu</string>"));
    // the same ids with and without locked units
    let plain = ex(F::Plist, &src);
    assert_eq!(plain.units.iter().map(|u| u.id.clone()).collect::<Vec<_>>(), ["s2"]);
    // a value with only a placeholder is locked, with its own reason
    let src = plist("<dict><key>CFBundleName</key><string>%@</string></dict>");
    let d = ex_with(F::Plist, &src, &o);
    assert_eq!(d.units[0].lock_reason.as_deref(), Some("no translatable text"));
}

#[test]
fn context_carries_path_line_and_key() {
    let d = ex(F::Plist, include_bytes!("data/Root.plist"));
    let u: Vec<(&str, &str, &str)> =
        d.units.iter().map(|u| (u.context.key.as_deref().unwrap(), u.context.style.as_deref().unwrap(), u.context.location.as_deref().unwrap())).collect();
    assert_eq!(u[0], ("PreferenceSpecifiers/0/Title", "Title", "line 13"));
    assert_eq!(u[3].0, "PreferenceSpecifiers/1/Titles/0");
    assert_eq!(u[3].1, "Titles");
}

#[test]
fn sniffing_is_tight() {
    let info = include_bytes!("data/Info.plist");
    let sd = include_bytes!("data/Localizable.stringsdict");
    assert!(Plist.sniff(info) && Plist.sniff(sd) && !Stringsdict.sniff(info) && Stringsdict.sniff(sd));
    assert!(Plist.sniff(b"bplist00\x00") && !Stringsdict.sniff(b"bplist00\x00"));
    let mut u16 = vec![0xFF, 0xFE];
    for u in s(info).encode_utf16() {
        u16.extend_from_slice(&u.to_le_bytes());
    }
    assert!(Plist.sniff(&u16));
    for no in [&b""[..], b"hello", b"{ a = 1; }", b"<html><body>x</body></html>", b"<?xml version=\"1.0\"?><root/>", b"\"a\" = \"b\";", b"[plist]"] {
        assert!(!Plist.sniff(no) && !Stringsdict.sniff(no), "{:?}", String::from_utf8_lossy(no));
    }
    assert_eq!(bilingua_plist::filters().len(), 2);
}

#[test]
fn xml_forms_the_scanner_accepts() {
    let ok = |body: &str| plist(body);
    for src in [
        ok("<array/>"),
        ok("<dict/>"),
        ok("<string/>"),
        ok("<dict><key/><string>x</string></dict>"),
        ok("<array><integer>-12</integer><integer>0x1F</integer><real>1e3</real><real>-nan</real><date>2026-02-28T10:20:30Z</date><date>2026Z</date><data> AAEC\nAw== </data><true/><false></false></array>"),
        b"\xef\xbb\xbf<?xml version='1.0' encoding='utf-8' standalone='yes'?><plist><string>Hi</string></plist>".to_vec(),
        b"\n  \n<?xml version=\"1.0\"?>\n<plist version=\"1.0\"><string>No declared encoding</string></plist>".to_vec(),
        ok("<array><string>a</string><!-- c --><?pi x?></array>"),
    ] {
        let d = ex(F::Plist, &src);
        assert_eq!(merged(F::Plist, &d), src, "{}", s(&src));
    }
    assert_eq!(units_of(F::Plist, &ok("<string>Hello there</string>")), ["Hello there"]);
    // a top-level string is a generic plist and is translated when it reads as prose
    assert!(units_of(F::Plist, &ok("<string>com.example.id</string>")).is_empty());
}

#[test]
fn xml_forms_the_scanner_refuses() {
    for (body, why) in [
        ("<dict><key>a</key><string>1</string><key>a</key><string>2</string></dict>", "repeats"),
        ("<array><key>a</key></array>", "outside a <dict>"),
        ("<dict><key>a</key><key>b</key><string>x</string></dict>", "no value"),
        ("<dict><string>x</string></dict>", "no <key>"),
        ("<string>a</string><string>b</string>", "more than one"),
        ("<plist><string>x</string></plist>", "nested"),
        ("<integer>abc</integer>", "integer"),
        ("<real>1.2.3</real>", "real"),
        ("<date>2026-13-01T00:00:00Z</date>", "date"),
        ("<date>2026-02-30T00:00:00Z</date>", "date"),
        ("<data>@@@@</data>", "data"),
        ("<string>a<b/></string>", "cannot contain"),
        ("<string a='1' a='2'>x</string>", "repeated"),
        ("<string a=1>x</string>", "well-formed"),
        ("<string a='<'>x</string>", "well-formed"),
        ("<string>&#0;</string>", "character reference"),
        ("<string>&#xD800;</string>", "character reference"),
        ("<string>&amp</string>", "entity"),
        ("<string>x ]]> y</string>", "well-formed"),
        ("<!-- a -- b --><string>x</string>", "well-formed"),
        ("<string>x</strin>", "mismatched"),
        
        
        
        ("<1/>", "well-formed"),
        ("<?xml version=\"1.0\"?><string>x</string>", "well-formed"),
        ("<?xml?><string>x</string>", "well-formed"),
        ("<!DOCTYPE plist><string>x</string>", "well-formed"),
        ("<![CDATA[x]]><string>x</string>", "CDATA"),
        ("x<string>x</string>", "text"),
        ("<string>x</string>y", "text"),
        ("<foo/>", "unknown"),
    ] {
        let src = plist(body);
        let e = parse_err(F::Plist, &src);
        assert!(matches!(e, Error::Parse { .. }), "{body}: {e:?}");
        assert!(e.to_string().contains(why), "{body}: {e}");
    }
    // cut off inside the markup
    for (src, why) in [
        ("<plist><string>x", "ends inside"),
        ("<plist><string>x</string>", "ends inside"),
        ("<plist><string>x</string", "well-formed"),
        ("<plist><string", "not closed"),
        ("<plist><string a='x", "not closed"),
        ("<plist><!-- x", "not closed"),
        ("<plist><![CDATA[x", "not closed"),
        ("<plist><?pi x", "not closed"),
        ("<!DOCTYPE plist", "not closed"),
    ] {
        let e = parse_err(F::Plist, format!("{HEAD}{src}").as_bytes());
        assert!(e.to_string().contains(why), "{src}: {e}");
    }
    // not a plist at all
    for src in [&b""[..], b"   ", b"<", b"<plist>", b"<plist/>", b"plain text", b"\xff\xfe", b"\x00\x00\xfe\xff<"] {
        assert!(Plist.extract(src, &ExtractOptions::default()).is_err(), "{src:?}");
    }
}

#[test]
fn only_utf8_and_utf16_xml_are_read() {
    let latin = "<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?>\n<plist><string>caf\u{e9}</string></plist>";
    let (bytes, _, _) = bilingua_model::encoding_rs::WINDOWS_1252.encode(latin);
    assert!(matches!(parse_err(F::Plist, &bytes), Error::Encoding(_)));
    // declared, but written in UTF-8
    let e = parse_err(F::Plist, latin.as_bytes());
    assert!(e.to_string().contains("other than UTF-8/UTF-16"), "{e}");
    // an undeclared invalid UTF-8 file is refused too
    let bad = b"<?xml version=\"1.0\"?>\n<plist><string>caf\xe9</string></plist>";
    assert!(matches!(parse_err(F::Plist, bad), Error::Encoding(_)));
    // UTF-16 without a BOM
    let mut nobom = Vec::new();
    for u in "<plist><string>Hi</string></plist>".encode_utf16() {
        nobom.extend_from_slice(&u.to_le_bytes());
    }
    assert!(Plist.extract(&nobom, &ExtractOptions::default()).is_err());
    // the declaration may say utf-16 in a UTF-8 file or utf-8 in a UTF-16 one (LocOS reads both)
    let src = b"<?xml version=\"1.0\" encoding=\"UTF-16\"?><plist><string>Hello there</string></plist>";
    assert_eq!(units_of(F::Plist, src), ["Hello there"]);
}

#[test]
fn plural_fill_corner_cases() {
    let sd = |body: &str| plist(&format!("<dict><key>e</key><dict><key>NSStringLocalizedFormatKey</key><string>%#@n@</string><key>n</key><dict>{body}</dict></dict></dict>"));
    let spec = "<key>NSStringFormatSpecTypeKey</key><string>NSStringPluralRuleType</string><key>NSStringFormatValueTypeKey</key><string>d</string>";
    let ru = |bytes: &[u8]| {
        let d = ex_with(F::Sd, bytes, &ExtractOptions::new().with_target_lang("ru"));
        s(&merged(F::Sd, &prefixed(&d, "R ")))
    };
    // a compact one-line file has no white space between its keys, so the added forms start on a new line
    // (the LocOS rule: white space is copied only when the file has some there)
    let tight = sd(&format!("{spec}<key>one</key><string>%d one</string><key>other</key><string>%d many</string>"));
    assert!(
        ru(&tight).contains("<key>one</key><string>R %d one</string><key>few</key><string>R %d many</string>\n<key>many</key><string>R %d many</string>\n<key>other</key>"),
        "{}",
        ru(&tight)
    );
    // no `other`: nothing to fill from, nothing added
    let no_other = sd(&format!("{spec}<key>one</key><string>%d one</string>"));
    assert!(!ru(&no_other).contains("<key>few</key>"));
    // `other` first in the dictionary: the new forms go before it, after the type tags
    let first = sd(&format!("{spec}<key>other</key><string>%d x</string>"));
    let out = ru(&first);
    assert!(out.find("<key>one</key>").unwrap() < out.find("<key>few</key>").unwrap() && out.find("<key>many</key>").unwrap() < out.find("<key>other</key>").unwrap());
    // a dictionary that is not a plural rule (a device or width rule) is left alone
    let dev = sd("<key>NSStringFormatSpecTypeKey</key><string>NSStringVariableWidthRuleType</string><key>other</key><string>x</string>");
    assert!(!ru(&dev).contains("<key>few</key>"));
    // an `other` that is not a string is left alone
    let weird = sd(&format!("{spec}<key>other</key><array><string>x y</string></array>"));
    assert!(!ru(&weird).contains("<key>few</key>"));
    // unknown, empty and malformed target languages add nothing
    for lang in ["xx", "", "  ", "-", "_RU", "zzz-ZZ"] {
        let d = ex_with(F::Sd, &tight, &ExtractOptions::new().with_target_lang(lang));
        assert!(!s(&merged(F::Sd, &prefixed(&d, "R "))).contains("<key>few</key>"), "{lang:?}");
    }
    // a plist (not a stringsdict) never gets forms added
    let d = ex_with(F::Plist, &tight, &ExtractOptions::new().with_target_lang("ru"));
    assert!(!s(&merged(F::Plist, &prefixed(&d, "R "))).contains("<key>few</key>"));
    // the added form carries the translated text of `other`, including a CDATA original
    let cd = sd(&format!("{spec}<key>other</key><string><![CDATA[<b>%d</b> things]]></string>"));
    let out = ru(&cd);
    assert!(out.contains("<key>few</key><string>R &lt;b&gt;%d&lt;/b&gt; things</string>"), "{out}");
}

#[test]
fn a_translated_plural_fills_from_the_translation_not_the_source() {
    let d = ex_with(F::Sd, include_bytes!("data/Localizable.stringsdict"), &ExtractOptions::new().with_target_lang("ru"));
    let mut p = d.clone();
    // translate only `other` of files_count
    let id = d.units.iter().filter(|u| u.translate).nth(2).unwrap().segments[0].id.clone();
    p.set_target(&id, vec![Inline::tag("1", TagKind::Standalone, "%lld"), Inline::text(" \u{444}\u{430}\u{439}\u{43b}\u{43e}\u{432}")]).unwrap();
    let out = merged(F::Sd, &p);
    let recs = stringsdict_records(&out).unwrap();
    let f = &recs[0].plural_forms;
    assert_eq!(f.iter().find(|x| x.0 == "few").unwrap().1, "%lld \u{444}\u{430}\u{439}\u{43b}\u{43e}\u{432}");
    // forms in other variables, untranslated, are filled from their English `other`
    let folders = recs.iter().find(|r| r.context == "folders").unwrap();
    assert_eq!(folders.plural_forms.iter().find(|x| x.0 == "many").unwrap().1, "%d folders");
}

#[test]
fn records_cover_every_string_and_the_stringsdict_shape() {
    let recs = plist_records(include_bytes!("data/Root.plist")).unwrap();
    let keys: Vec<&str> = recs.iter().map(|r| r.key.as_str()).collect();
    assert_eq!(keys[0], "StringsTable");
    assert!(keys.contains(&"PreferenceSpecifiers/1/Titles/1"));
    assert_eq!(recs.iter().filter(|r| r.unit_index.is_some()).count(), 5);
    let sd = stringsdict_records(include_bytes!("data/Localizable.stringsdict")).unwrap();
    assert_eq!(sd.len(), 4);
    let tap = sd.iter().find(|r| r.key == "tap_to_continue").unwrap();
    assert_eq!(tap.plural_forms.len(), 3);
    assert_eq!(tap.unit_index, Some(8));
    // a stringsdict with a plain string entry and a nested entry with no variables
    let src = plist("<dict><key>plain</key><string>Hello there</string><key>empty</key><dict/><key>list</key><array/></dict>");
    let r = stringsdict_records(&src).unwrap();
    assert_eq!((r.len(), r[0].key.as_str(), r[0].unit_index), (2, "plain", Some(0)));
    assert_eq!(r[1].key, "empty");
}

#[test]
fn positional_specifiers_may_move() {
    let d = ex(F::Sd, include_bytes!("data/Localizable.stringsdict"));
    let u = d.units.iter().filter(|u| u.translate).nth(3).unwrap();
    let id = u.segments[0].id.clone();
    let tags: Vec<(String, bool)> = u.source().iter().filter_map(|i| if let Inline::Tag { id, free, .. } = i { Some((id.clone(), *free)) } else { None }).collect();
    assert_eq!(tags, [("1".into(), true), ("2".into(), true)]);
    let mut p = d.clone();
    p.set_target(&id, vec![Inline::free_tag("2", "%2$#@folders@"), Inline::text(" / "), Inline::free_tag("1", "%1$#@messages@")]).unwrap();
    assert!(s(&merged(F::Sd, &p)).contains("<string>%2$#@folders@ / %1$#@messages@</string>"));
    // an unnumbered specifier is not free
    let d = ex(F::Sd, include_bytes!("data/Localizable.stringsdict"));
    let u = d.units.iter().filter(|u| u.translate).nth(1).unwrap();
    assert!(u.source().iter().all(|i| !i.is_free()));
}

#[test]
fn a_binary_file_survives_json_and_a_second_extract() {
    let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/plist/binary_ru.stringsdict")).unwrap();
    let mut d = ex_with(F::Sd, &bytes, &ExtractOptions::new().with_target_lang("ru"));
    set_nth(&mut d, 0, "\u{41d}\u{435}\u{442} \u{444}\u{430}\u{439}\u{43b}\u{43e}\u{432}");
    let json = serde_json::to_string(&d).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(back, d);
    let out = merged(F::Sd, &back);
    assert_eq!(merged(F::Sd, &d), out);
    assert_eq!(units_of(F::Sd, &out)[0], "\u{41d}\u{435}\u{442} \u{444}\u{430}\u{439}\u{43b}\u{43e}\u{432}");
    // a damaged skeleton is refused, not guessed at
    let mut broken = d.clone();
    if let Skeleton::Text { parts, .. } = &mut broken.skeleton {
        parts[0] = SkelPart::Raw("not base64!!".into());
    }
    assert!(matches!(Stringsdict.merge(&broken), Err(Error::Write(_))));
    let mut wrong = d.clone();
    wrong.units[0].set_hidden("ord", "9999");
    assert!(matches!(Stringsdict.merge(&wrong), Err(Error::Write(_))));
    let mut zip = d.clone();
    zip.skeleton = Skeleton::Zip { entries: vec![], parser: None };
    assert!(matches!(Stringsdict.merge(&zip), Err(Error::Write(_))));
}

#[test]
fn binary_value_kinds_come_back() {
    use common::bp::{dict, s as bs, V};
    let src = dict(vec![
        ("CFBundleName", bs("Name")),
        ("Null-ish", V::Arr(vec![V::Bool(true), V::Int(-5), V::Int(70_000), V::Int(5_000_000_000), V::Real(0.25), V::Date(1.5), V::Data(vec![7; 40])])),
        ("Unicode", bs("\u{65e5}\u{672c}\u{8a9e} \u{1F600}")),
        ("Long", bs(&"x".repeat(300))),
        ("Nested", dict(vec![("a", V::Arr(vec![bs("one")])), ("b", V::Dict(vec![]))])),
    ]);
    let data = common::bp::write(&src);
    let mut d = ex(F::Plist, &data);
    set_nth(&mut d, 0, "Neu \u{1F600}");
    let out = merged(F::Plist, &d);
    let V::Dict(mut items) = common::bp::read(&out) else { panic!() };
    assert_eq!(items.remove(0).1, bs("Neu \u{1F600}"));
    let V::Dict(orig) = src else { panic!() };
    assert_eq!(items, orig[1..].to_vec());
}

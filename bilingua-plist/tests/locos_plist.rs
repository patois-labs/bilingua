//! The Localization OS suite `tests/verify_plist_filter.py`, ported case by case (9 test functions in LocOS, 9
//! here: the web-surface one runs the same upload, translate and export through the filter and a target locale),
//! plus the fixtures it reads (`tests/data/`).
mod common;
use bilingua_model::*;
use bilingua_plist::{filters, plist_records, stringsdict_records, Limits, Plist, Stringsdict};
use common::bp::{self, dict, s, V};
use common::*;

const INFO: &[u8] = include_bytes!("data/Info.plist");
const ROOT: &[u8] = include_bytes!("data/Root.plist");
const SD: &[u8] = include_bytes!("data/Localizable.stringsdict");

fn text(b: &[u8]) -> String {
    String::from_utf8(b.to_vec()).unwrap()
}

fn opts_for(lang: &str) -> ExtractOptions {
    ExtractOptions::new().with_target_lang(lang)
}

/// The stringsdict fixture as a binary plist (what `plistlib.dumps(..., FMT_BINARY)` made in the suite).
fn sd_value() -> V {
    dict(vec![
        ("files_count", dict(vec![
            ("NSStringLocalizedFormatKey", s("%#@files@")),
            ("files", dict(vec![
                ("NSStringFormatSpecTypeKey", s("NSStringPluralRuleType")),
                ("NSStringFormatValueTypeKey", s("lld")),
                ("zero", s("No files")),
                ("one", s("%lld file")),
                ("other", s("%lld files")),
            ])),
        ])),
        ("inbox_summary", dict(vec![
            ("NSStringLocalizedFormatKey", s("You have %1$#@messages@ in %2$#@folders@ & more")),
            ("messages", dict(vec![
                ("NSStringFormatSpecTypeKey", s("NSStringPluralRuleType")),
                ("NSStringFormatValueTypeKey", s("d")),
                ("one", s("%d message")),
                ("other", s("%d messages")),
            ])),
            ("folders", dict(vec![
                ("NSStringFormatSpecTypeKey", s("NSStringPluralRuleType")),
                ("NSStringFormatValueTypeKey", s("d")),
                ("one", s("one folder")),
                ("other", s("%d folders")),
            ])),
        ])),
        ("tap_to_continue", dict(vec![(
            "NSStringDeviceSpecificRuleType",
            dict(vec![("iphone", s("Tap to continue")), ("mac", s("Click to continue")), ("appletv", s("Press <Select> to continue"))]),
        )])),
    ])
}

#[test]
fn test_registered() {
    assert_eq!(Plist.extensions(), ["plist"]);
    assert_eq!(Stringsdict.extensions(), ["stringsdict"]);
    assert_eq!((Plist.id(), Stringsdict.id()), ("plist", "stringsdict"));
    let ids: Vec<&str> = filters().iter().map(|f| f.id()).collect();
    assert_eq!(ids, ["stringsdict", "plist"]);
}

#[test]
fn test_identity_round_trips() {
    for (data, f) in [(SD, F::Sd), (INFO, F::Plist), (ROOT, F::Plist)] {
        assert_eq!(merged(f, &ex(f, data)), data);
        assert_eq!(echo(f, data), data);
        // a build for no locale (LocOS `build_localized(.., None)`) adds nothing
        assert_eq!(merged(f, &ex_with(f, data, &ExtractOptions::default())), data);
    }
    // CRLF, a UTF-8 BOM and UTF-16 LE with BOM survive byte for byte too
    let base = INFO;
    let crlf = text(base).replace('\n', "\r\n").into_bytes();
    let bom = [&[0xEF, 0xBB, 0xBF][..], base].concat();
    let mut u16 = vec![0xFF, 0xFE];
    for u in text(base).replace("encoding=\"UTF-8\"", "encoding=\"UTF-16\"").encode_utf16() {
        u16.extend_from_slice(&u.to_le_bytes());
    }
    let want = units_of(F::Plist, base);
    for data in [&crlf, &bom, &u16] {
        assert_eq!(merged(F::Plist, &ex(F::Plist, data)), *data);
        assert_eq!(echo(F::Plist, data), *data);
        assert_eq!(units_of(F::Plist, data), want);
    }
    let mut d = ex(F::Plist, &u16);
    set_nth(&mut d, 0, "Wanderhefte");
    let out = merged(F::Plist, &d);
    assert!(out.starts_with(&[0xFF, 0xFE]));
    let back: Vec<u16> = out[2..].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    assert!(String::from_utf16(&back).unwrap().contains("<string>Wanderhefte</string>"));
    let mut d = ex(F::Plist, &crlf);
    set_nth(&mut d, 0, "Wanderhefte");
    let expected = text(&crlf).replacen("Trail Notes</string>", "Wanderhefte</string>", 1).into_bytes();
    assert_eq!(merged(F::Plist, &d), expected);
}

#[test]
fn test_policy_info_settings_generic() {
    assert_eq!(
        units_of(F::Plist, INFO),
        [
            "Trail Notes",
            "Trail Notes uses the camera to attach photos to a note.",
            "Copyright 2026 Example Co. All rights reserved.",
            "Precise location draws your position on the trail map.",
            "Trail Note",
        ]
    );
    assert_eq!(units_of(F::Plist, ROOT), ["Sync", "Notes sync when the app is open.", "Sync interval", "Every hour", "Every day"]);
    let generic = br#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>welcome</key><string>Welcome back</string>
<key>button</key><string>Save</string>
<key>id</key><string>com.example.thing</string>
<key>icon</key><string>star.fill</string>
<key>url</key><string>https://example.com/help</string>
<key>build</key><string>$(PRODUCT_NAME)</string>
<key>version</key><string>2.0.1</string>
<key>slug</key><string>home_screen</string>
<key>camel</key><string>isEnabled</string>
<key>nested</key><array><dict><key>label</key><string>Delete note</string></dict></array>
<key>count</key><integer>3</integer>
<key>flag</key><true/>
</dict></plist>"#;
    assert_eq!(units_of(F::Plist, generic), ["Welcome back", "Save", "Delete note"]);
}

#[test]
fn test_stringsdict_units_tokens_and_splice() {
    let d = ex(F::Sd, SD);
    let ph = "\u{27e6}x1\u{27e7}";
    assert_eq!(
        unit_tokens(&d),
        [
            "No files".to_string(),
            format!("{ph} file"),
            format!("{ph} files"),
            // #693: positional %1$#@messages@ / %2$#@folders@ are free-order tokens
            "You have \u{27e6}p1\u{27e7} in \u{27e6}p2\u{27e7} & more".to_string(),
            format!("{ph} message"),
            format!("{ph} messages"),
            "one folder".to_string(),
            format!("{ph} folders"),
            "Tap to continue".to_string(),
            "Click to continue".to_string(),
            "Press <Select> to continue".to_string(),
        ]
    );
    // %#@files@ alone and the type tags (NSStringPluralRuleType, lld) are never units
    assert!(!texts(&d).iter().any(|u| u.contains("NSString") || u == "lld"));
    let mut d = ex(F::Sd, SD);
    set_nth(&mut d, 1, "%lld Datei");
    // free tags (positional) may move; here they keep their order
    set_nth(&mut d, 3, "Du hast %1$#@messages@ in %2$#@folders@ & mehr");
    set_nth(&mut d, 10, "Dr\u{fc}cke <Auswahl>");
    let out = text(&merged(F::Sd, &d));
    assert!(out.contains("<string>%lld Datei</string>"), "{out}");
    assert!(out.contains("<string>Du hast %1$#@messages@ in %2$#@folders@ &amp; mehr</string>"), "{out}");
    assert!(out.contains("<string>Dr\u{fc}cke &lt;Auswahl&gt;</string>"), "{out}");
    let (before, after): (Vec<&str>, Vec<&str>) = (std::str::from_utf8(SD).unwrap().lines().collect(), out.lines().collect());
    assert_eq!(before.len(), after.len());
    assert_eq!(before.iter().zip(&after).filter(|(a, b)| a != b).count(), 3);
    // a token the engine invented is refused before it can reach the file
    let mut d = ex(F::Sd, SD);
    let id = d.units.iter().filter(|u| u.translate).nth(1).unwrap().segments[0].id.clone();
    let t = vec![Inline::tag("1", TagKind::Standalone, "%lld"), Inline::text(" Datei "), Inline::tag("9", TagKind::Standalone, "%lld")];
    assert!(matches!(d.set_target(&id, t), Err(Error::InventedTag(_))));
}

fn forms_of(out: &[u8], key: &str, ctx: &str) -> Vec<(String, String)> {
    stringsdict_records(out).unwrap().into_iter().find(|r| r.key == key && r.context == ctx).unwrap().plural_forms
}

#[test]
fn test_stringsdict_plural_categories_follow_target() {
    let ru = prefixed(&ex_with(F::Sd, SD, &opts_for("ru")), "RU ");
    let out = merged(F::Sd, &ru);
    let files = forms_of(&out, "files_count", "");
    let names: Vec<&str> = files.iter().map(|f| f.0.as_str()).collect();
    assert_eq!(names, ["zero", "one", "few", "many", "other"]);
    let t = text(&out);
    // the spec and value type tags stay first, in the dictionary
    assert!(t.find("NSStringFormatValueTypeKey").unwrap() < t.find("<key>zero</key>").unwrap());
    for c in ["few", "many", "other"] {
        assert_eq!(files.iter().find(|f| f.0 == c).unwrap().1, "RU %lld files");
    }
    let folders = forms_of(&out, "inbox_summary", "folders");
    assert_eq!(&folders.iter().map(|f| f.0.as_str()).collect::<Vec<_>>()[folders.len() - 3..], ["few", "many", "other"]);
    // device variants are not plural rules: untouched shape
    let t_dev = t.split("NSStringDeviceSpecificRuleType").nth(1).unwrap();
    assert!(t_dev.find("<key>iphone</key>").unwrap() < t_dev.find("<key>mac</key>").unwrap());
    assert!(t_dev.find("<key>mac</key>").unwrap() < t_dev.find("<key>appletv</key>").unwrap());
    assert!(!t_dev.contains("<key>few</key>"));
    // the file is still the same shape: reading it again gives the same units
    assert_eq!(ex(F::Sd, &out).units.len(), 17);
    // Japanese has only `other`: nothing is added
    let ja = merged(F::Sd, &prefixed(&ex_with(F::Sd, SD, &opts_for("ja")), "JA "));
    let jf = forms_of(&ja, "files_count", "");
    assert_eq!(jf.iter().map(|f| f.0.as_str()).collect::<Vec<_>>(), ["zero", "one", "other"]);
    // a build without a locale never adds categories
    let plain = merged(F::Sd, &prefixed(&ex(F::Sd, SD), "RU "));
    assert!(!text(&plain).contains("<key>few</key>"));
    // an untouched merge for a target locale is still the original file
    assert_eq!(merged(F::Sd, &ex_with(F::Sd, SD, &opts_for("ru"))), SD);
    // binary stringsdict gets the same treatment
    let binary = bp::write(&sd_value());
    assert_eq!(echo(F::Sd, &binary), binary);
    let bru = merged(F::Sd, &prefixed(&ex_with(F::Sd, &binary, &opts_for("ru")), "RU "));
    let V::Dict(top) = bp::read(&bru) else { panic!() };
    let V::Dict(entry) = &top.iter().find(|(k, _)| k == "files_count").unwrap().1 else { panic!() };
    let V::Dict(files) = &entry.iter().find(|(k, _)| k == "files").unwrap().1 else { panic!() };
    let keys: Vec<&str> = files.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, ["NSStringFormatSpecTypeKey", "NSStringFormatValueTypeKey", "zero", "one", "few", "many", "other"]);
    assert_eq!(files.iter().find(|(k, _)| k == "many").unwrap().1, s("RU %lld files"));
}

#[test]
fn test_binary_plist_round_trip() {
    let src = dict(vec![
        ("CFBundleIdentifier", s("com.example.app")),
        ("CFBundleDisplayName", s("Trail Notes")),
        ("NSMicrophoneUsageDescription", s("Record voice memos for a note.")),
        ("Blob", V::Data(vec![0, 1, 2])),
        ("When", V::Date(813_749_400.0)),
        ("Ratio", V::Real(1.5)),
        ("Enabled", V::Bool(false)),
        ("Ids", V::Arr(vec![V::Int(1), V::Int(2), V::Int(3)])),
    ]);
    let data = bp::write(&src);
    let d = ex(F::Plist, &data);
    let Skeleton::Text { encoding, .. } = &d.skeleton else { panic!() };
    assert_eq!(encoding, "bplist");
    assert_eq!(texts(&d), ["Trail Notes", "Record voice memos for a note."]);
    assert_eq!(merged(F::Plist, &d), data);
    assert_eq!(echo(F::Plist, &data), data);
    let mut d = ex(F::Plist, &data);
    set_nth(&mut d, 1, "Sprachnotizen aufnehmen.");
    let out = merged(F::Plist, &d);
    assert!(out.starts_with(b"bplist00"));
    let V::Dict(items) = bp::read(&out) else { panic!() };
    let V::Dict(orig) = &src else { panic!() };
    assert_eq!(items.iter().map(|(k, _)| k).collect::<Vec<_>>(), orig.iter().map(|(k, _)| k).collect::<Vec<_>>(), "key order kept");
    for ((k, v), (_, o)) in items.iter().zip(orig) {
        if k == "NSMicrophoneUsageDescription" {
            assert_eq!(v, &s("Sprachnotizen aufnehmen."));
        } else {
            assert_eq!(v, o, "{k}");
        }
    }
}

#[test]
fn test_records() {
    let recs = stringsdict_records(SD).unwrap();
    let first = &recs[0];
    assert_eq!((first.key.as_str(), first.value.as_str(), first.unit_index), ("files_count", "%#@files@", Some(0)));
    let forms: Vec<(&str, &str)> = first.plural_forms.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    assert_eq!(forms, [("zero", "No files"), ("one", "%lld file"), ("other", "%lld files")]);
    assert_eq!(first.plural_units, [("zero".to_string(), 0), ("one".to_string(), 1), ("other".to_string(), 2)]);
    let inbox: Vec<&str> = recs.iter().filter(|r| r.key == "inbox_summary").map(|r| r.context.as_str()).collect();
    assert_eq!(inbox, ["", "folders"]);
    let info: std::collections::HashMap<String, Option<usize>> = plist_records(INFO).unwrap().into_iter().map(|r| (r.key, r.unit_index)).collect();
    assert_eq!(info["CFBundleIdentifier"], None);
    assert_eq!(info["CFBundleDisplayName"], Some(0));
    assert_eq!(info["NSLocationTemporaryUsageDescriptionDictionary/TrailMap"], Some(3));
}

const HEAD: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n";
const DT: &str = "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n";

fn is_limit(e: &Error, kind: LimitKind) -> bool {
    matches!(e, Error::Limit(l) if l.kind == kind)
}

#[test]
fn test_hostile_inputs() {
    let f = F::Plist;
    let laughs = format!("{HEAD}<!DOCTYPE plist [<!ENTITY a \"aaaaaaaaaa\"><!ENTITY b \"&a;&a;&a;&a;&a;&a;&a;&a;\">]>\n<plist><dict><key>k</key><string>&b;</string></dict></plist>");
    assert!(is_limit(&parse_err(f, laughs.as_bytes()), LimitKind::XmlEntities));
    let xxe = format!("{HEAD}<!DOCTYPE plist [<!ENTITY x SYSTEM \"file:///c:/windows/win.ini\">]>\n<plist><dict><key>k</key><string>&x;</string></dict></plist>");
    assert!(is_limit(&parse_err(f, xxe.as_bytes()), LimitKind::XmlEntities));
    let undefined = format!("{HEAD}{DT}<plist><dict><key>k</key><string>Hi &nbsp; there</string></dict></plist>");
    assert!(parse_err(f, undefined.as_bytes()).to_string().contains("entity"));
    for (bad, why) in [
        (format!("{HEAD}{DT}<plist><dict><key>k</key><string>x</dict></plist>"), "mismatched"),
        (format!("{HEAD}<plist><dict><key>k</key></dict></plist>"), "no value"),
        (format!("{HEAD}<plist><dict><foo>x</foo></dict></plist>"), "unknown"),
        ("{ title = \"Hello\"; }".to_string(), "OpenStep"),
        ("bplist00garbage".to_string(), "binary property list"),
    ] {
        let e = parse_err(f, bad.as_bytes());
        assert!(matches!(e, Error::Parse { .. }), "{bad}: {e:?}");
        assert!(e.to_string().to_lowercase().contains(&why.to_lowercase()), "{bad}: {e}");
    }
    // a binary plist whose array contains itself (a reference cycle)
    let mut cyc = b"bplist00\xa1\x00\x08".to_vec();
    cyc.extend_from_slice(&[0; 6]);
    cyc.extend_from_slice(&[1, 1]);
    for n in [1u64, 0, 10] {
        cyc.extend_from_slice(&n.to_be_bytes());
    }
    let e = parse_err(f, &cyc);
    assert!(is_limit(&e, LimitKind::XmlDepth) && e.to_string().contains("nested"), "{e:?}");
    let deep = format!("{HEAD}<plist>{}<string>x</string>{}</plist>", "<array>".repeat(300), "</array>".repeat(300));
    let e = parse_err(f, deep.as_bytes());
    assert!(is_limit(&e, LimitKind::XmlDepth) && e.to_string().contains("deep"), "{e:?}");
    // between the plist limit (128) and the XML guard (256)
    let mid = format!("{HEAD}<plist>{}<string>x</string>{}</plist>", "<array>".repeat(200), "</array>".repeat(200));
    let e = parse_err(f, mid.as_bytes());
    assert!(is_limit(&e, LimitKind::XmlDepth) && e.to_string().contains("nested"), "{e:?}");
    // caps: segment count, markup budget, one huge value
    let small = Limits::new().with_max_units(3);
    let o = ExtractOptions::default();
    let many = format!("{HEAD}<plist><array>{}</array></plist>", "<string>Hello there</string>".repeat(5));
    assert!(is_limit(&Plist.extract_with(many.as_bytes(), &o, &small).unwrap_err(), LimitKind::Units));
    let budget = format!("{HEAD}<plist><array>{}</array></plist>", "<true/>".repeat(200));
    assert!(is_limit(&Plist.extract_with(budget.as_bytes(), &o, &small).unwrap_err(), LimitKind::Units));
    let short = Limits::new().with_max_unit_chars(50);
    let big = format!("{HEAD}<plist><string>{}</string></plist>", "word ".repeat(40));
    assert!(is_limit(&Plist.extract_with(big.as_bytes(), &o, &short).unwrap_err(), LimitKind::UnitChars));
    // entities and CDATA that ARE allowed decode once and round-trip
    let ok = format!("{HEAD}{DT}<plist><dict><key>k</key><string>Tom &amp; Jerry&#39;s <![CDATA[<b>show</b>]]></string></dict></plist>");
    assert_eq!(units_of(f, ok.as_bytes()), ["Tom & Jerry's <b>show</b>"]);
    assert_eq!(echo(f, ok.as_bytes()), ok.as_bytes());
}

#[test]
fn test_web_surface_stringsdict() {
    // upload -> translate -> export for a Russian project: the same steps the web test runs, through the filter
    let doc = ex_with(F::Sd, SD, &opts_for("ru"));
    let out = merged(F::Sd, &prefixed(&doc, "RU "));
    let files = forms_of(&out, "files_count", "");
    let names: Vec<&str> = files.iter().map(|f| f.0.as_str()).collect();
    assert!(names.contains(&"few") && names.contains(&"many"), "{names:?}");
    let other = &files.iter().find(|f| f.0 == "other").unwrap().1;
    assert!(other.contains("%lld") && !text(&out).contains('\u{27e6}'), "{other}");
    assert!(text(&out).contains("<key>NSStringLocalizedFormatKey</key>\n\t\t<string>%#@files@</string>"));
}

#[test]
fn utf16_plist_stays_utf16_when_output_is_utf8() {
    // a UTF-16 LE stringsdict with a mark: a changed build under the profile keeps UTF-16 and the mark, also with
    // UTF-8 output; natively UTF-8 output is UTF-8 and the declaration says so
    let body = "<?xml version=\"1.0\" encoding=\"UTF-16\"?>
<plist version=\"1.0\"><dict><key>k</key><dict><key>NSStringLocalizedFormatKey</key><string>%#@n@</string><key>n</key><dict><key>NSStringFormatSpecTypeKey</key><string>NSStringPluralRuleType</string><key>NSStringFormatValueTypeKey</key><string>d</string><key>one</key><string>%d file</string><key>other</key><string>%d files</string></dict></dict></dict></plist>
";
    let mut src = vec![0xFF, 0xFE];
    for u in body.encode_utf16() {
        src.extend_from_slice(&u.to_le_bytes());
    }
    let f = Stringsdict;
    for profile in [None, Some("locos".to_string())] {
        let o = ExtractOptions::new().with_target_lang("de").with_profile(profile.clone());
        let mut d = f.extract(&src, &o).unwrap();
        set_nth(&mut d, 0, "Eine Datei");
        let mo = MergeOptions::new().with_output_encoding(OutputEncoding::Utf8).with_profile(profile.clone());
        let out = f.merge_with(&d, &mo).unwrap();
        if profile.is_some() {
            assert!(out.starts_with(&[0xFF, 0xFE]), "UTF-16 LE mark kept");
            assert_eq!(out.len() % 2, 0);
            let back: Vec<u16> = out[2..].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            let t = String::from_utf16(&back).unwrap();
            assert!(t.starts_with("<?xml version=\"1.0\" encoding=\"UTF-16\"?>"));
            assert!(t.contains("Eine Datei"));
        } else {
            let t = String::from_utf8(out).expect("UTF-8 out is UTF-8");
            assert!(t.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"), "{t}");
            assert!(t.contains("Eine Datei"));
        }
        // without the UTF-8 switch the file keeps UTF-16 in both modes
        let keep = MergeOptions::new().with_profile(profile);
        assert!(f.merge_with(&d, &keep).unwrap().starts_with(&[0xFF, 0xFE]));
    }
}

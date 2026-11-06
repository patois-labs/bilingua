//! `records()`, the key-based view of LocOS's `XmlResourceFilter.records`. The expected rows were read from the
//! LocOS filters (`get_filter("xml").records(data)`) on these two fixtures.

use bilingua_xmlres::{records, AndroidStrings, JavaXmlProperties};

const STRINGS: &[u8] = include_bytes!("data/strings.xml");
const MESSAGES: &[u8] = include_bytes!("data/messages.xml");

/// key, value, comment, plural, index, translatable
type Row = (&'static str, &'static str, &'static str, &'static str, Option<usize>, bool);

const ANDROID: &[Row] = &[
    ("app_name", "Northwind", "App name shown in the launcher", "", None, false),
    ("welcome", "Welcome, %1$s! You have %2$d new messages.", "", "", None, true),
    ("cant_open", "Can't open the file \"%s\".", "", "", None, true),
    ("at_literal", "@home is the default tag", "", "", None, true),
    ("alias", "@string/welcome", "", "", None, false),
    ("theme_ref", "?android:attr/textColorPrimary", "", "", None, false),
    ("styled", "Tap \u{27e6}g1\u{27e7}Save\u{27e6}/g1\u{27e7} to keep \u{27e6}g2\u{27e7}your\u{27e6}/g2\u{27e7} changes.", "", "", None, true),
    ("placeholder", "Signed in as \u{27e6}x1\u{27e7}.", "", "", None, true),
    ("html_cdata", "Read the <a href=\"https://example.com/terms\">terms</a> first.", "", "", None, true),
    ("quoted", "It's   spaced   out", "", "", None, true),
    ("newline", "First line\\nSecond line", "", "", None, true),
    ("percent", "100% sure", "", "", None, true),
    ("lint_ok", "Only in English", "", "", None, true),
    ("empty", "", "", "", None, false),
    ("songs", "%d song found.", "", "one", None, true),
    ("songs", "%d songs found.", "", "other", None, true),
    ("files", "%d file", "", "one", None, true),
    ("files", "%d files (few)", "", "few", None, true),
    ("files", "%d files (many)", "", "many", None, true),
    ("files", "%d files", "", "other", None, true),
    ("planets", "Mercury", "", "", Some(0), true),
    ("planets", "Venus", "", "", Some(1), true),
    ("codes", "MRC", "", "", Some(0), false),
];

const JAVA: &[Row] = &[
    ("greeting", "Hello {0}, it's {1,date,long}.", "", "", None, true),
    ("farewell", "Goodbye & thanks", "", "", None, true),
    ("build.id", "4.2.0", "", "", None, true),
];

fn check(got: Vec<bilingua_xmlres::Record>, want: &[Row]) {
    assert_eq!(got.len(), want.len());
    for (g, w) in got.iter().zip(want) {
        assert_eq!((g.key.as_str(), g.value.as_str(), g.comment.as_str(), g.plural.as_str(), g.index, g.translatable), *w, "{}", w.0);
        assert!(g.max_length.is_none() && g.context.is_empty());
    }
}

#[test]
fn android_records_match_locos() {
    check(AndroidStrings.records(STRINGS).unwrap(), ANDROID);
    check(records(STRINGS).unwrap(), ANDROID);
}

#[test]
fn java_records_match_locos() {
    check(JavaXmlProperties.records(MESSAGES).unwrap(), JAVA);
    let keys: Vec<String> = records(MESSAGES).unwrap().into_iter().map(|r| r.key).collect();
    assert_eq!(keys, ["greeting", "farewell", "build.id"]);
}

#[test]
fn records_dispatch_by_root_and_refuse_other_xml() {
    assert!(records(b"<?xml version=\"1.0\"?><catalog><a>x</a></catalog>").is_err());
    assert!(records(b"not xml at all").is_err());
    assert!(records(b"").is_err());
    // a hostile file is refused as extract refuses it, never a panic
    let dtd = b"<?xml version=\"1.0\"?><!DOCTYPE resources [<!ENTITY a \"x\">]><resources><string name=\"x\">&a;</string></resources>";
    assert!(AndroidStrings.records(dtd).is_err());
}

#[test]
fn records_count_the_units_and_agree_with_extract() {
    use bilingua_model::{ExtractOptions, Filter};
    for (f, data) in [(&AndroidStrings as &dyn Filter, STRINGS), (&JavaXmlProperties as &dyn Filter, MESSAGES)] {
        let recs = records(data).unwrap();
        let doc = f.extract(data, &ExtractOptions::new()).unwrap();
        assert_eq!(recs.iter().filter(|r| r.translatable).count(), doc.units.len());
    }
}

#[test]
fn records_keep_outer_white_space_and_unescape_like_the_translator_view() {
    let x = b"<?xml version=\"1.0\"?><resources><string name=\"a\">  Hello \\'x\\'  </string></resources>";
    let r = AndroidStrings.records(x).unwrap();
    assert_eq!(r[0].value, "  Hello 'x'  ");
}

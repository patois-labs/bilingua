//! The ten cases of the LocOS Fluent suite (`tests/verify_ftl_filter.py`), ported one for one, with its fixture.
mod common;
use bilingua_ftl::{structure, FtlFilter, Limits};
use bilingua_model::*;
use common::*;

const SRC: &str = "### Resource comment

# Simple message with a variable.
hello = Hello, { $name }!

## Group: brand
-brand-name = Firefox
    .gender = masculine

emails = { $unread ->
    [one] You have one unread email.
   *[other] You have { $unread } unread emails in { -brand-name }.
}

multi =
    First line of the text
    second line { \"{\" } brace

login-button = Log in
    .title = Click to log in
    .accesskey = L

only-attrs =
    .placeholder = Search the web

numbers = { NUMBER($n, minimumFractionDigits: 2) }
broken = { $x
ref = { hello }
";

fn src() -> &'static [u8] {
    SRC.as_bytes()
}

fn texts(v: &[&str]) -> Vec<(usize, String)> {
    v.iter().enumerate().map(|(i, s)| (i, s.to_string())).collect()
}

fn build_s(data: &[u8], m: &[(usize, String)]) -> Vec<u8> {
    let p: Vec<(usize, &str)> = m.iter().map(|(i, s)| (*i, s.as_str())).collect();
    build(data, &p)
}

// case 1: registration (the facade test covers the lookup by extension; here the filter's own identity)
#[test]
fn registration() {
    assert_eq!(FtlFilter.id(), "ftl");
    assert_eq!(FtlFilter.extensions(), &["ftl"]);
    assert!(FtlFilter.sniff(src()));
    assert!(!FtlFilter.sniff(b"just = a line\nof = text\n"), "plain key = value is not enough");
}

// case 2
#[test]
fn units_and_identity() {
    let doc = extract(src()).unwrap();
    let got: Vec<(String, String)> = doc.units.iter().map(|u| (u.context.style.clone().unwrap(), flat(u))).collect();
    let want = [
        ("ftl:hello", "Hello, { $name }!"),
        ("ftl:emails[one]", "You have one unread email."),
        ("ftl:emails[other]", "You have { $unread } unread emails in { -brand-name }."),
        ("ftl:multi", "First line of the text\nsecond line { brace"),
        ("ftl:login-button", "Log in"),
        ("ftl:login-button.title", "Click to log in"),
        ("ftl:login-button.accesskey", "L"),
        ("ftl:only-attrs.placeholder", "Search the web"),
    ];
    assert_eq!(got.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect::<Vec<_>>(), want);
    assert_eq!(doc.units.iter().map(|u| u.id.as_str()).collect::<Vec<_>>(), ["u1", "u2", "u3", "u4", "u5", "u6", "u7", "u8"]);
    assert_identity(src());
    // the placeables are free inline tags that show their source
    let u = &doc.units[2];
    assert_eq!(u.segments[0].source.iter().filter(|i| i.is_free()).count(), 2);
    assert_eq!(u.codes["1"].label.as_deref(), Some("placeholder"));
    // the attached comment is the unit's note
    assert_eq!(doc.units[0].context.notes, vec!["Simple message with a variable.".to_string()]);
}

// case 3
#[test]
fn translated_round_trip() {
    let de = texts(&[
        "Hallo, { $name }!",
        "Sie haben eine ungelesene E-Mail.",
        "Sie haben { $unread } ungelesene E-Mails in { -brand-name }.",
        "Erste Zeile des Textes\nzweite Zeile { Klammer",
        "Anmelden",
        "Klicken, um sich anzumelden",
        "A",
        "Im Web suchen",
    ]);
    let out = build_s(src(), &de);
    let text = String::from_utf8(out.clone()).unwrap();
    assert!(text.contains("hello = Hallo, { $name }!\n"), "{text}");
    assert!(text.contains("    [one] Sie haben eine ungelesene E-Mail.\n"), "{text}");
    assert!(text.contains("   *[other] Sie haben { $unread } ungelesene E-Mails in { -brand-name }.\n"));
    assert!(text.contains("multi =\n    Erste Zeile des Textes\n    zweite Zeile {\"{\"} Klammer\n"), "{text}");
    assert!(text.contains("    .title = Klicken, um sich anzumelden\n"));
    assert!(text.contains("-brand-name = Firefox\n    .gender = masculine\n"), "term untouched");
    assert!(text.contains("broken = { $x\n"), "Junk kept byte for byte");
    assert!(text.contains("### Resource comment\n") && text.contains("## Group: brand\n"));
    assert_eq!(structure(&out).unwrap().junk, 1, "only the source's own Junk");
    let again = units(&out);
    assert_eq!(again, de.iter().map(|(_, s)| s.clone()).collect::<Vec<_>>());
    assert_eq!(again.len(), 8);
}

// case 4
#[test]
fn structure_cannot_be_broken() {
    let hostile = "Text { $evil } with } and {\n[one] fake variant\n*[other] x\n.attr = y\n  .z";
    let out = build_s(src(), &[(0, hostile.to_string())]);
    let st = structure(&out).unwrap();
    assert_eq!(st.junk, 1);
    let before = structure(src()).unwrap();
    assert_eq!((st.messages, st.terms, st.attributes), (before.messages, before.terms, before.attributes), "a translation must not create entries or attributes");
    let u0 = units(&out)[0].clone();
    assert_eq!(u0, hostile, "Fluent keeps relative indentation");
    assert_eq!(styles(&out), styles(src()));
    // a placeable the source did not have is escaped, never live: the first unit still has no tags
    let doc = extract(&out).unwrap();
    assert!(doc.units[0].segments[0].source.iter().all(|i| matches!(i, Inline::Text(_))), "{{ $evil }} is text, not a placeable");
}

// case 5
#[test]
fn terms_only_when_configured() {
    let doc = extract_terms(src()).unwrap();
    let styles: Vec<String> = doc.units.iter().map(|u| u.context.style.clone().unwrap()).collect();
    assert!(styles.contains(&"ftl:-brand-name".to_string()), "{styles:?}");
    assert!(!styles.iter().any(|s| s.contains("gender")), "{styles:?}");
    let k = styles.iter().position(|s| s == "ftl:-brand-name").unwrap();
    let mut doc = doc;
    let id = doc.units[k].segments[0].id.clone();
    doc.set_target(&id, vec![Inline::text("Feuerfuchs")]).unwrap();
    let out = String::from_utf8(FtlFilter.merge(&doc).unwrap()).unwrap();
    assert!(out.contains("-brand-name = Feuerfuchs\n    .gender = masculine\n"), "{out}");
    assert!(!extract(src()).unwrap().units.iter().any(|u| u.context.style.as_deref() == Some("ftl:-brand-name")));
}

// case 6
#[test]
fn crlf_and_bom() {
    let mut src_crlf = vec![0xEF, 0xBB, 0xBF];
    src_crlf.extend_from_slice(SRC.replace('\n', "\r\n").as_bytes());
    let u = units(&src_crlf);
    assert_eq!(u[3], "First line of the text\nsecond line { brace");
    assert_identity(&src_crlf);
    let out = build(&src_crlf, &[(3, "Eins\nZwei"), (0, "Hallo")]);
    assert!(out.starts_with(&[0xEF, 0xBB, 0xBF]));
    let s = String::from_utf8_lossy(&out).to_string();
    assert!(s.contains("multi =\r\n    Eins\r\n    Zwei\r\n"), "{s:?}");
    assert!(s.contains("\r\n\r\n") && !s.replace("\r\n", "").contains('\n'), "no bare LF");
    assert!(s.contains("hello = Hallo\r\n"));
}

// case 7
#[test]
fn non_translatable_skipped_but_aligned() {
    let src = b"a = { $only }\nb = Real text\nc = ---\nd = { -brand-name }\ne = More text\n";
    assert_eq!(units(src), ["Real text", "More text"]);
    let out = build(src, &[(0, "Echter Text"), (1, "Mehr Text")]);
    assert_eq!(String::from_utf8(out).unwrap(), "a = { $only }\nb = Echter Text\nc = ---\nd = { -brand-name }\ne = Mehr Text\n");
}

// case 8
#[test]
fn nested_select() {
    let src = b"shared = { $gender ->\n    [male] { $n ->\n        [one] He shared one photo.\n       *[other] He shared { $n } photos.\n    }\n   *[other] They shared photos.\n}\n";
    assert_eq!(styles(src), ["ftl:shared[male][one]", "ftl:shared[male][other]", "ftl:shared[other]"]);
    let out = build(src, &[(1, "Er teilte { $n } Fotos.\nzweite Zeile")]);
    assert_eq!(structure(&out).unwrap().junk, 0);
    let needle = b"       *[other] Er teilte { $n } Fotos.\n           zweite Zeile\n";
    assert!(out.windows(needle.len()).any(|w| w == needle), "{}", String::from_utf8_lossy(&out));
}

// case 9
#[test]
fn plural_variants_filled_729() {
    let src = b"files = { $count ->\n    [one] { $count } file was deleted.\n   *[other] { $count } files were deleted.\n}\ngreet = { $gender ->\n    [male] He said hi\n   *[other] They said hi\n}\n";
    let tr = [(0usize, "{ $count } S-pl"), (1, "{ $count } P-pl\nline two"), (3, "Oni")];
    let pl = String::from_utf8(build_locale(src, &tr, "pl-PL")).unwrap();
    assert_eq!(structure(pl.as_bytes()).unwrap().junk, 0);
    assert!(
        pl.contains("    [one] { $count } S-pl\n    [few] { $count } P-pl\n       line two\n    [many] { $count } P-pl\n       line two\n   *[other] { $count } P-pl\n       line two\n}\n"),
        "{pl}"
    );
    assert!(!pl.split("greet").nth(1).unwrap().contains("[few]"), "{pl}");
    let ar = String::from_utf8(build_locale(src, &tr, "ar")).unwrap();
    for cat in ["zero", "two", "few", "many"] {
        assert!(ar.contains(&format!("    [{cat}] {{ $count }} P-pl\n")), "{ar}");
    }
    assert_eq!(build_locale(src, &tr, "ja"), build(src, &tr));
    assert!(!String::from_utf8(build(src, &tr)).unwrap().contains("[few]"));
    // gate: an ordinal select follows the target's ORDINAL categories
    let ord = b"place = { NUMBER($n, type: \"ordinal\") ->\n    [one] { $n }st\n   *[other] { $n }th\n}\n";
    let otr = [(0usize, "{ $n }-O"), (1, "{ $n }-X")];
    assert_eq!(build_locale(ord, &otr, "pl-PL"), build(ord, &otr));
    let cy = String::from_utf8(build_locale(ord, &otr, "cy")).unwrap();
    assert_eq!(structure(cy.as_bytes()).unwrap().junk, 0);
    for cat in ["zero", "two", "few", "many"] {
        assert!(cy.contains(&format!("    [{cat}] {{ $n }}-X\n")), "{cy}");
    }
}

// case 10
#[test]
fn refusals() {
    let latin1 = b"hello = caf\xe9\n";
    // decode refusals are code `encoding` with LocOS's sentences (UnsupportedEncodingError)
    assert!(
        matches!(extract(latin1), Err(Error::Encoding(m)) if m == "Fluent .ftl files are UTF-8; byte 0xe9 at position 11 is not. Re-save the file as UTF-8."),
        "latin-1"
    );
    assert!(
        matches!(extract(b"\x00\x01\x02hello = x\x00"), Err(Error::Encoding(m)) if m.contains("looks like a binary file")),
        "binary"
    );
    let deep = format!("x = {}$a{}\n", "{ ".repeat(200), " }".repeat(200));
    assert!(
        matches!(extract(deep.as_bytes()), Err(Error::Limit(e)) if e.msg == "This Fluent file nests placeables more than 40 deep."),
        "deep nesting"
    );
    let small = { let mut built = Limits::default(); built.max_units = 3; built };
    let many: String = (0..10).map(|i| format!("m{i} = Message number {i}\n")).collect();
    assert!(FtlFilter.extract_with(many.as_bytes(), &opts(), &small).is_err(), "segment cap");
    let braces = format!("x = {}\n", "{ $a } ".repeat(100));
    let tiny = { let mut built = Limits::default(); built.max_units = 3; built };
    // the placeable budget is code `limit` with LocOS's sentence (InputLimitError naming MAX_DOCUMENT_SEGMENTS)
    let err = FtlFilter.extract_with(braces.as_bytes(), &opts(), &tiny).unwrap_err();
    assert!(
        matches!(&err, Error::Limit(e) if e.kind == bilingua_model::LimitKind::Units
            && e.msg == "This Fluent file has far more placeables than MAX_DOCUMENT_SEGMENTS allows (3). Split it into smaller files."),
        "placeable budget: {err:?}"
    );
    let chars = { let mut built = Limits::default(); built.max_value_chars = 1000; built };
    let long = format!("x = {}\n", "word ".repeat(400));
    assert!(FtlFilter.extract_with(long.as_bytes(), &opts(), &chars).is_err(), "segment length cap");
}

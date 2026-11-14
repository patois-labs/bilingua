//! Ported from the Localization OS suite `tests/verify_toml_filter.py` (10 test functions: all 10 ported; the
//! registry, API and upload-list cases are adapted to this crate's own surface). Fixture: `tests/data/hugo.toml`.

mod common;
use bilingua_model::profile::tokens::to_locos_text;
use bilingua_model::*;
use bilingua_toml::{keys, TomlFilter};
use common::*;

const HUGO: &str = include_str!("data/hugo.toml");

fn expected() -> Vec<(&'static str, &'static str)> {
    vec![
        ("toml:other", "Home"),
        ("toml:one", "One minute to read"),
        ("toml:other", "\u{27e6}x1\u{27e7} minutes to read"),
        ("toml:one", "\u{27e6}x1\u{27e7} has one cat."),
        ("toml:other", "\u{27e6}x1\u{27e7} has \u{27e6}x2\u{27e7} cats."),
        ("toml:other", "Hello \u{27e6}x1\u{27e7}, {{not a token}}"),
        ("toml", "Words: many"),
        ("toml", "My \"site\" \u{e9}"),
        ("toml", "alpha"),
        ("toml", "beta"),
        ("toml", "Gamma"),
        ("toml", "Quoted"),
    ]
}

fn exp_owned() -> Vec<(String, String)> {
    expected().into_iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

#[test]
fn registered() {
    assert_eq!(TomlFilter.id(), "toml");
    assert_eq!(TomlFilter.extensions(), &["toml"]);
    assert!(!TomlFilter.bilingual());
    assert!(TomlFilter.sniff(HUGO.as_bytes()));
    assert!(!TomlFilter.sniff(b"{\"a\": 1}"));
    assert!(!TomlFilter.sniff(b"hello world"));
    assert!(!TomlFilter.sniff(b"# only a comment\n"));
}

#[test]
fn units_and_identity() {
    let b = HUGO.as_bytes();
    let d = extract(b);
    assert_eq!(pairs(&d), exp_owned());
    let ids: Vec<&str> = d.units.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(ids, (1..=12).map(|i| format!("u{i}")).collect::<Vec<_>>());
    // inline codes: the template actions, one standalone tag each
    let code = |u: usize, id: &str| d.units[u].codes[id].standalone.clone().unwrap();
    assert_eq!(code(2, "1"), "{{ .Count }}");
    assert_eq!(code(5, "1"), "<<.Name>>");
    assert!(matches!(&d.units[2].source()[0], Inline::Tag { kind: TagKind::Standalone, .. }));
    assert_eq!(to_locos_text(&d.units[2].source()).1.len(), 1);
    assert_eq!(merge(&d), b);
    // translating every unit to its own text writes the same bytes
    let mut same = extract(b);
    translate_all(&mut same, |u| u.source());
    assert_eq!(merge(&same), b);
    // CRLF + BOM survive untouched
    let crlf = [b"\xef\xbb\xbf".as_slice(), HUGO.replace('\n', "\r\n").as_bytes()].concat();
    let dc = extract(&crlf);
    assert_eq!(pairs(&dc), exp_owned());
    assert_eq!(merge(&dc), crlf);
}

#[test]
fn translated_round_trip() {
    let b = HUGO.as_bytes();
    let mut d = extract(b);
    translate_all(&mut d, prefix_de);
    let out = merge(&d);
    let v = toml_value(&out);
    let s = |t: &toml::Table, k: &str| t[k].as_str().unwrap().to_string();
    assert_eq!(s(v["home"].as_table().unwrap(), "other"), "DE Home");
    assert_eq!(s(v["readingTime"].as_table().unwrap(), "other"), "DE {{ .Count }} minutes to read");
    let pc = v["PersonCats"].as_table().unwrap();
    assert_eq!(s(pc, "other"), "DE {{.Name}} has {{.Count}} cats.");
    assert_eq!(s(pc, "description"), "The number of cats a person has");
    assert_eq!((s(pc, "id"), s(pc, "hash")), ("PersonCats".into(), "sha1-abc".into()));
    assert_eq!(s(v["Custom"].as_table().unwrap(), "other"), "DE Hello <<.Name>>, {{not a token}}");
    assert_eq!(v["i18n"][0]["translation"].as_str().unwrap(), "DE Words: many");
    assert_eq!(v["site"]["title"]["main"].as_str().unwrap(), "DE My \"site\" \u{e9}");
    let tags: Vec<String> = v["site"]["tags"].as_array().unwrap().iter().map(|x| x.to_string()).collect();
    assert_eq!(tags, vec!["\"DE alpha\"", "\"DE beta\"", "3", "{ label = \"DE Gamma\" }", "\"{{ .Only }}\"", "\"---\""]);
    assert_eq!(s(v["site"].as_table().unwrap(), "quoted key"), "DE Quoted");
    let text = String::from_utf8(out).unwrap();
    for keep in [
        "# Hugo i18n (en)",
        "   # nav label",
        "   # trailing comment",
        "when = 1979-05-27 07:32:00Z",
        "ratio = 0.5",
        "[[i18n]]",
        "'''\nDE {{.Name}} has {{.Count}} cats.'''",
        "one = 'DE {{.Name}} has one cat.'",
    ] {
        assert!(text.contains(keep), "{keep:?} missing from {text}");
    }
    // a missing action is dropped, not invented: the translation of unit 3 omits its tag
    let mut d = extract(b);
    set(&mut d, 2, "Lesezeit  Min");
    let out = merge(&d);
    assert_eq!(toml_value(&out)["readingTime"]["other"].as_str().unwrap(), "Lesezeit  Min");
    // an invented tag is refused up front
    let mut d = extract(b);
    let bad = vec![Inline::tag("9", TagKind::Standalone, "{{ .X }}")];
    assert!(matches!(d.set_target("u3", bad), Err(Error::InventedTag(_))));
}

#[test]
fn quoting_choices() {
    let src = r##"a = 'plain'
b = "basic"
c = '''
multi
line'''
d = """no lead"""
e = 'path'
"##
    .as_bytes();
    let mut d = extract(src);
    set(&mut d, 0, "l'apostrophe");
    set(&mut d, 1, "tab\there \"q\" back\\slash\u{1}");
    set(&mut d, 2, "zwei\nZeilen ''' drei");
    set(&mut d, 3, "\nstarts with newline and ends with \"");
    set(&mut d, 4, "zeile1\nzeile2");
    let out = merge(&d);
    let text = String::from_utf8(out.clone()).unwrap();
    assert!(text.contains("a = \"l'apostrophe\""), "{text}");
    assert!(text.contains("b = \"tab\\there \\\"q\\\" back\\\\slash\\u0001\""), "{text}");
    assert_eq!(text.matches("\"\"\"").count(), 4, "{text}");
    let v = toml_value(&out);
    assert_eq!(v["a"].as_str().unwrap(), "l'apostrophe");
    assert_eq!(v["b"].as_str().unwrap(), "tab\there \"q\" back\\slash\u{1}");
    assert_eq!(v["c"].as_str().unwrap(), "zwei\nZeilen ''' drei");
    assert_eq!(v["d"].as_str().unwrap(), "\nstarts with newline and ends with \"");
    assert_eq!(v["e"].as_str().unwrap(), "zeile1\nzeile2");
    assert!(text.contains("e = \"zeile1\\nzeile2\""), "{text}");
    // literal kept when it fits; ml literal kept when it fits
    let mut d = extract(src);
    set(&mut d, 0, "schlicht");
    set(&mut d, 2, "mehr\nZeilen");
    let out = merge(&d);
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("a = 'schlicht'") && text.contains("c = '''\nmehr\nZeilen'''"), "{text}");
    // CRLF file: new line breaks inside a multi-line value follow the file
    let crlf = b"x = \"\"\"\r\nalt\r\nzwei\"\"\"\r\ny = 1\r\n";
    let mut d = extract(crlf);
    set(&mut d, 0, "neu\ndrei");
    assert_eq!(merge(&d), b"x = \"\"\"\r\nneu\r\ndrei\"\"\"\r\ny = 1\r\n");
}

#[test]
fn paths_and_tables() {
    let src = br#"
[[fruits]]
name = "apple"
[fruits.physical]
color = "red"
[[fruits.varieties]]
name = "red delicious"
[[fruits]]
name = "banana"
[[fruits.varieties]]
name = "plantain"
[dog."tater.man"]
type.name = "pug"
inline = { a = { b = "deep" } }
nested = [["x1", ["y1"]]]
"#;
    let mut d = extract(src);
    assert_eq!(
        keys(&d),
        vec![
            "fruits[0].name",
            "fruits[0].physical.color",
            "fruits[0].varieties[0].name",
            "fruits[1].name",
            "fruits[1].varieties[0].name",
            "dog.\"tater.man\".type.name",
            "dog.\"tater.man\".inline.a.b",
            "dog.\"tater.man\".nested[0][0]",
            "dog.\"tater.man\".nested[0][1][0]",
        ]
    );
    for i in 0..d.units.len() {
        set(&mut d, i, &format!("T{i}"));
    }
    let v = toml_value(&merge(&d));
    assert_eq!(v["fruits"][1]["varieties"][0]["name"].as_str().unwrap(), "T4");
    assert_eq!(v["dog"]["tater.man"]["nested"].to_string(), "[[\"T7\", [\"T8\"]]]");
    // message tables carry the description as the unit's note
    let h = extract(HUGO.as_bytes());
    let u = &h.units[4];
    assert_eq!(u.context.key.as_deref(), Some("PersonCats.other"));
    assert_eq!(u.context.notes, vec!["The number of cats a person has".to_string()]);
    assert_eq!(h.units[3].context.style.as_deref(), Some("toml:one"));
}

#[test]
fn message_detection() {
    let src = b"[a]\ndescription = \"note A\"\ntranslation = \"Text A\"\n\
[b]\ndescription = \"Shown B\"\ntitle = \"Title B\"\n\
[c]\nDescription = \"note C\"\nOne = \"one C\"\nOther = \"other C\"\n";
    let d = extract(src);
    let got: Vec<(String, String)> = pairs(&d);
    let want = [("toml", "Text A"), ("toml", "Shown B"), ("toml", "Title B"), ("toml:one", "one C"), ("toml:other", "other C")];
    assert_eq!(got, want.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect::<Vec<_>>());
}

fn refused(data: &[u8], needle: &str) -> Error {
    match try_extract(data) {
        Err(e) => {
            assert!(e.to_string().to_lowercase().contains(&needle.to_lowercase()), "{needle:?} not in {e}");
            e
        }
        Ok(_) => panic!("expected a refusal for {:?}", String::from_utf8_lossy(&data[..data.len().min(60)])),
    }
}

#[test]
fn hostile_inputs() {
    assert!(matches!(refused(b"a = \"unterminated\n", "could not be read"), Error::Parse { .. }));
    assert!(matches!(refused(b"a = 1\na = 2\n", "could not be read"), Error::Parse { .. }));
    // decode refusals: code `encoding`, LocOS `_decode` sentences
    assert!(matches!(
        refused(b"a = 'caf\xe9'", "UTF-8"),
        Error::Encoding(m) if m == "This TOML file is not valid UTF-8 (byte 0xe9 at position 8); re-save the file as UTF-8."
    ));
    assert!(matches!(refused(b"\xff\xfea\x00 \x00=\x00", "UTF-8"), Error::Encoding(_)));
    assert!(matches!(
        refused(b"a = '\x00\x00\x00'", "UTF-8"),
        Error::Encoding(m) if m == "This TOML file is not UTF-8 text (TOML files must be UTF-8); re-save the file as UTF-8."
    ));
    let mut deep = b"a = ".to_vec();
    deep.extend(std::iter::repeat_n(b'[', 5000));
    deep.extend(std::iter::repeat_n(b']', 5000));
    assert!(matches!(refused(&deep, "nested"), Error::Limit(e) if e.kind == LimitKind::Depth));
    let mut n70 = b"a = ".to_vec();
    n70.extend(std::iter::repeat_n(b'[', 70));
    n70.extend_from_slice(b"\"x\"");
    n70.extend(std::iter::repeat_n(b']', 70));
    n70.push(b'\n');
    refused(&n70, "nested");
    // more translatable strings than the cap (MAX_DOCUMENT_SEGMENTS, 50000 by default)
    let mut many = String::new();
    for i in 0..=50_000 {
        many.push_str(&format!("k{i} = \"v\"\n"));
    }
    assert!(matches!(refused(many.as_bytes(), "segments"), Error::Limit(e) if e.kind == LimitKind::Units));
    // one string over the length cap
    let long = format!("a = \"{}\"", "x".repeat(bilingua_toml::MAX_UNIT_CHARS + 1));
    assert!(matches!(refused(long.as_bytes(), "longer"), Error::Limit(e) if e.kind == LimitKind::UnitChars));
    // pathological template text stays linear
    let big = format!("a = \"{}\"\n", "x{{".repeat(30000));
    let t0 = std::time::Instant::now();
    let d = extract(big.as_bytes());
    assert_eq!(d.units.len(), 1);
    assert!(d.units[0].codes.keys().all(|k| k.starts_with('$')), "no tags expected");
    assert!(t0.elapsed().as_secs() < 5);
}

#[test]
fn surface_extract_translate_merge() {
    // The API-surface case of the LocOS suite: upload, translate, download; an invalid file is refused.
    let mut d = extract(HUGO.as_bytes());
    translate_all(&mut d, prefix_de);
    let out = merge(&d);
    let v = toml_value(&out);
    assert!(v["home"]["other"].as_str().unwrap().contains("Home"));
    assert!(v["readingTime"]["other"].as_str().unwrap().contains("{{ .Count }}"));
    assert_eq!(v["PersonCats"]["description"].as_str().unwrap(), "The number of cats a person has");
    assert!(out.windows(14).any(|w| w == b"# Hugo i18n (e") && out.windows(27).any(|w| w == b"when = 1979-05-27 07:32:00Z"));
    assert!(try_extract(b"a = \"open\n").is_err());
    // the document survives a JSON round trip (as it does between the engine and an editor)
    let j = serde_json::to_string(&d).unwrap();
    let back: Document = serde_json::from_str(&j).unwrap();
    assert_eq!(merge(&back), out);
}

#[test]
fn many_edits_linear_splice() {
    let n = 5000;
    let src: String = (0..n).map(|i| format!("k{i} = \"Value number {i}\"\n")).collect();
    let mut d = extract(src.as_bytes());
    for i in 0..n {
        set(&mut d, i, &format!("Wert Nummer {i}"));
    }
    let out = merge(&d);
    let v = toml_value(&out);
    assert!((0..n).all(|i| v[&format!("k{i}")].as_str() == Some(&format!("Wert Nummer {i}"))));
}

#[test]
fn extension_and_sniff_lists() {
    // stands in for the LocOS upload accept list and connector map
    assert_eq!(TomlFilter.extensions(), &["toml"]);
}

/// LocOS `_budget` (verify_toml_filter test_hostile_inputs, MAX_DOCUMENT_SEGMENTS=3): the quote and value budgets
/// refuse before the scan, code `limit`; a file under them still reads.
#[test]
fn pre_parse_budgets_with_a_small_segment_cap() {
    let mut opts = ExtractOptions::default();
    opts.limits = Some(Limits::new().with_max_units(3));
    let ex = |b: &[u8]| TomlFilter.extract(b, &opts);
    // 200 apostrophes inside one string: 202 quotes > 40 x 3
    let quotes = format!("a = \"{}\"\n", "'".repeat(200));
    let e = ex(quotes.as_bytes()).unwrap_err();
    assert!(matches!(&e, Error::Limit(l) if l.kind == LimitKind::Units && l.msg.contains("far more strings")), "{e:?}");
    // 100 array values: 100 commas and brackets > 20 x 3
    let values = format!("a = [{}]\n", vec!["1"; 100].join(","));
    let e = ex(values.as_bytes()).unwrap_err();
    assert!(matches!(&e, Error::Limit(l) if l.msg.contains("far more values")), "{e:?}");
    // more strings than the cap: code `limit`, not `unsupported`
    let e = ex(b"a = \"one\"\nb = \"two\"\nc = \"three\"\nd = \"four\"\n").unwrap_err();
    assert!(matches!(&e, Error::Limit(l) if l.kind == LimitKind::Units), "{e:?}");
    // a string over MAX_SEGMENT_CHARS
    let mut o = ExtractOptions::default();
    o.limits = Some(Limits::new().with_max_unit_chars(10));
    let e = TomlFilter.extract(b"a = \"a much longer string\"\n", &o).unwrap_err();
    assert!(matches!(&e, Error::Limit(l) if l.kind == LimitKind::UnitChars), "{e:?}");
    assert_eq!(ex(b"a = \"one\"\nb = \"two\"\n").unwrap().units.len(), 2);
}

/// verify_toml_filter test_translated_round_trip:147 (B-side 10, #814.1): a pair token naming the point code
/// `{{ .Count }}` is dropped, the unknown `x9` stays text.
#[test]
fn wrong_kind_token_naming_a_point_code_is_dropped() {
    use bilingua_model::profile::{locos_set_target, locos_units, tokens::parse_locos_text_for};
    let mut doc = extract(b"readingTime = \"{{ .Count }} min read\"\n");
    let u = locos_units(&doc).remove(0);
    let t = "Lesezeit \u{27E6}x9\u{27E7} \u{27E6}g1\u{27E7}Min\u{27E6}/g1\u{27E7} \u{27E6}x1\u{27E7}";
    let (inl, _) = parse_locos_text_for(&doc.format, t, &u.map);
    locos_set_target(&mut doc, &u.segment_id, inl).unwrap();
    let out = String::from_utf8(merge(&doc)).unwrap();
    assert_eq!(out, "readingTime = \"Lesezeit \u{27E6}x9\u{27E7} Min {{ .Count }}\"\n");
}

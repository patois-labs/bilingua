//! Exact round trips, the file's layout and encodings, eleven languages, the LocOS profile, sniffing and ids.
mod common;
use bilingua_model::*;
use bilingua_richtext::*;
use common::*;
use serde_json::json;

fn all() -> Vec<(&'static str, Box<dyn Filter>, &'static [u8])> {
    vec![("contentful", Box::new(Contentful), CONTENTFUL), ("sanity", Box::new(Sanity), PORTABLE), ("adf", Box::new(Adf), JIRA), ("notion", Box::new(Notion), NOTION)]
}

#[test]
fn untouched_merge_is_the_original_bytes_and_ids_are_stable() {
    for (id, f, data) in all() {
        let d = open(&*f, data);
        assert_eq!(d.format, id);
        assert!(!d.units.is_empty(), "{id}");
        assert_eq!(f.merge(&d).unwrap(), data, "{id}");
        let ids: Vec<&str> = d.units.iter().map(|u| u.id.as_str()).collect();
        let want: Vec<String> = (1..=d.units.len()).map(|n| format!("u{n}")).collect();
        assert_eq!(ids, want, "{id}");
        assert_eq!(open(&*f, data), d, "{id}: two extractions differ");
        // a target equal to the source is not a change either
        let mut same = d.clone();
        for i in 0..same.units.len() {
            let src = same.units[i].source();
            let sid = same.units[i].segments[0].id.clone();
            same.set_target(&sid, src).unwrap();
        }
        assert_eq!(f.merge(&same).unwrap(), data, "{id}");
        // LocOS profile: the walker index
        let lo = f.extract(data, &profile::locos().extract_options()).unwrap();
        for (n, u) in lo.units.iter().enumerate() {
            assert_eq!(u.context.extra[profile::P_INDEX], n.to_string(), "{id}");
        }
        assert_eq!(f.merge_with(&lo, &profile::locos().merge_options(id)).unwrap(), data, "{id}");
    }
}

#[test]
fn pseudo_translation_reads_back_with_the_same_units_and_tags() {
    for (id, f, data) in all() {
        let mut d = open(&*f, data);
        let n = d.units.len();
        d.target_lang = Some("de-DE".into());
        for i in 0..n {
            let mut t = vec![Inline::text("[")];
            t.extend(d.units[i].source());
            t.push(Inline::text("]"));
            let sid = d.units[i].segments[0].id.clone();
            d.set_target(&sid, t).unwrap();
        }
        let out = f.merge(&d).unwrap();
        let back = open(&*f, &out);
        assert_eq!(back.units.len(), n, "{id}");
        for (a, b) in d.units.iter().zip(&back.units) {
            let want = format!("[{}]", plain_text(&a.source()));
            if id != "adf" || !a.source().iter().any(|i| matches!(i, Inline::Tag { .. })) {
                assert_eq!(plain_text(&b.source()), want, "{id} {}", a.id);
            }
            let tags = |u: &Unit| u.source().iter().filter(|i| matches!(i, Inline::Tag { .. })).count();
            assert_eq!(tags(a), tags(b), "{id} {}", a.id);
        }
        json(&out);
    }
}

#[test]
fn byte_order_marks_and_encodings() {
    let src = "[{\"object\":\"block\",\"id\":\"b\",\"type\":\"paragraph\",\"paragraph\":{\"rich_text\":[{\"type\":\"text\",\"text\":{\"content\":\"Caf\u{e9} ouvert\",\"link\":null},\"plain_text\":\"Caf\u{e9} ouvert\"}]}}]\n";
    let f = Notion;
    // UTF-8 BOM
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend_from_slice(src.as_bytes());
    // UTF-16 LE with a BOM
    let mut u16le = vec![0xFF, 0xFE];
    u16le.extend(src.encode_utf16().flat_map(|u| u.to_le_bytes()));
    // Windows-1252
    let w1252: Vec<u8> = src.chars().map(|c| if c == '\u{e9}' { 0xE9 } else { c as u8 }).collect();
    for (name, bytes) in [("bom", bom), ("utf-16le", u16le), ("windows-1252", w1252)] {
        let mut d = open(&f, &bytes);
        assert_eq!(texts(&d), ["Caf\u{e9} ouvert"], "{name}");
        assert_eq!(f.merge(&d).unwrap(), bytes, "{name}: untouched");
        d.set_target("u1", vec![Inline::text("Caf\u{e9} ferm\u{e9}")]).unwrap();
        let out = f.merge(&d).unwrap();
        let back = open(&f, &out);
        assert_eq!(texts(&back), ["Caf\u{e9} ferm\u{e9}"], "{name}");
        if name != "windows-1252" {
            assert_eq!(out[..2.min(out.len())], bytes[..2.min(bytes.len())], "{name}: BOM kept");
        }
    }
    // UTF-8 output on request
    let d = open(&f, src.as_bytes());
    let mut d2 = d.clone();
    d2.set_target("u1", vec![Inline::text("\u{65e5}\u{672c}")]).unwrap();
    let out = f.merge_with(&d2, &MergeOptions::new().with_output_encoding(OutputEncoding::Utf8)).unwrap();
    assert!(std::str::from_utf8(&out).is_ok());
}

fn layouts(value: &serde_json::Value) -> Vec<(&'static str, String)> {
    let two = serde_json::to_string_pretty(value).unwrap();
    let pretty = |unit: &str| {
        two.split('\n')
            .map(|l: &str| {
                let n = l.len() - l.trim_start_matches(' ').len();
                format!("{}{}", unit.repeat(n / 2), &l[n..])
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let compact = serde_json::to_string(value).unwrap();
    let pycompact = compact.replace("\":", "\": ").replace(',', ", ");
    vec![
        ("two", format!("{}\n", pretty("  "))),
        ("four", format!("{}\n", pretty("    "))),
        ("tab", pretty("\t")),
        ("crlf", format!("{}\r\n", pretty("  ").replace('\n', "\r\n"))),
        ("compact", compact),
        ("pycompact", format!("{pycompact}\n")),
        ("lead", format!("\n\n  {}  \n\n", serde_json::to_string(value).unwrap())),
    ]
}

#[test]
fn a_rebuilt_file_keeps_the_layout_of_the_original() {
    let f = Notion;
    let v = json!([{"object": "block", "id": "b", "type": "paragraph", "paragraph": {"rich_text": [
        {"type": "text", "text": {"content": "Hello world", "link": null}, "plain_text": "Hello world"}], "color": "default"}},
        {"object": "block", "id": "e", "type": "paragraph", "paragraph": {"rich_text": []}}]);
    for (name, text) in layouts(&v) {
        let d0 = open(&f, text.as_bytes());
        assert_eq!(f.merge(&d0).unwrap(), text.as_bytes(), "{name}: untouched");
        let mut d = d0.clone();
        d.set_target("u1", vec![Inline::text("XHello world")]).unwrap();
        let out = String::from_utf8(f.merge(&d).unwrap()).unwrap();
        assert_eq!(out, text.replace("Hello world", "XHello world"), "{name}");
    }
}

#[test]
fn escapes_are_written_the_way_the_file_had_them() {
    let f = Notion;
    // ensure_ascii style
    let esc = "[{\"id\": \"b\", \"type\": \"paragraph\", \"paragraph\": {\"rich_text\": [{\"type\": \"text\", \"text\": {\"content\": \"Hello w\\u00f6rld \\ud83d\\ude00\"}, \"plain_text\": \"x\"}]}}]";
    let mut d = open(&f, esc.as_bytes());
    assert_eq!(texts(&d), ["Hello w\u{f6}rld \u{1f600}"]);
    d.set_target("u1", vec![Inline::text("Hallo w\u{f6}rld \u{1f600} \"q\" \\ \n")]).unwrap();
    let out = String::from_utf8(f.merge(&d).unwrap()).unwrap();
    assert!(out.contains("Hallo w\\u00f6rld \\ud83d\\ude00 \\\"q\\\" \\\\ \\n"), "{out}");
    assert_eq!(json(out.as_bytes())[0]["paragraph"]["rich_text"][0]["text"]["content"], "Hallo w\u{f6}rld \u{1f600} \"q\" \\ \n");
    // literal UTF-8 style stays literal
    let lit = esc.replace("\\u00f6", "\u{f6}").replace("\\ud83d\\ude00", "\u{1f600}");
    let mut d = open(&f, lit.as_bytes());
    d.set_target("u1", vec![Inline::text("Hallo w\u{f6}rld")]).unwrap();
    let out = String::from_utf8(f.merge(&d).unwrap()).unwrap();
    assert!(out.contains("Hallo w\u{f6}rld"), "{out}");
    // duplicate keys: the last wins (Python json.loads)
    let dup = "[{\"id\": \"b\", \"type\": \"paragraph\", \"paragraph\": {\"rich_text\": [{\"type\": \"text\", \"text\": {\"content\": \"first\"}}], \"rich_text\": [{\"type\": \"text\", \"text\": {\"content\": \"second\"}}]}}]";
    assert_eq!(texts(&open(&f, dup.as_bytes())), ["second"]);
}

const LANGS: [(&str, &str); 11] = [
    ("en", "Welcome to Lumen Bakery. Fresh bread is ready at six every morning."),
    ("es", "Bienvenido a Panader\u{ed}a Lumen. El pan fresco est\u{e1} listo a las seis de cada ma\u{f1}ana."),
    ("fr", "Bienvenue \u{e0} la Boulangerie Lumen. Le pain frais est pr\u{ea}t \u{e0} six heures chaque matin."),
    ("it", "Benvenuto al Forno Lumen. Il pane fresco \u{e8} pronto ogni mattina alle sei."),
    ("de", "Willkommen in der B\u{e4}ckerei Lumen. Frisches Brot ist jeden Morgen um sechs Uhr fertig."),
    ("nl", "Welkom bij Bakkerij Lumen. Vers brood staat elke ochtend om zes uur klaar."),
    ("pt", "Bem-vindo \u{e0} Padaria Lumen. O p\u{e3}o fresco fica pronto \u{e0}s seis horas todas as manh\u{e3}s."),
    ("ja", "ルーメン・ベーカリーへようこそ。焼きたてのパンは毎朝六時にご用意しています。"),
    ("zh", "欢迎光临流明面包店。新鲜面包每天早上六点出炉。"),
    ("ko", "루멘 베이커리에 오신 것을 환영합니다. 갓 구운 빵은 매일 아침 여섯 시에 준비됩니다."),
    ("ar", "مرحبا بكم في مخبز لومن. الخبز الطازج جاهز كل صباح في السادسة."),
];

#[test]
fn eleven_languages_in_every_format() {
    for ascii in [false, true] {
        let mut docs: Vec<(&str, Box<dyn Filter>, serde_json::Value)> = Vec::new();
        let cf: Vec<_> = LANGS.iter().map(|(_, s)| json!({"nodeType": "paragraph", "data": {}, "content": [{"nodeType": "text", "value": s, "marks": [{"type": "bold"}], "data": {}}] })).collect();
        docs.push(("contentful", Box::new(Contentful), json!({"nodeType": "document", "data": {}, "content": cf})));
        let pt: Vec<_> = LANGS.iter().enumerate().map(|(i, (_, s))| json!({"_type": "block", "_key": format!("k{i}"), "children": [{"_type": "span", "_key": "a", "text": s, "marks": ["em"]}]})).collect();
        docs.push(("sanity", Box::new(Sanity), json!(pt)));
        let ad: Vec<_> = LANGS.iter().map(|(_, s)| json!({"type": "paragraph", "content": [{"type": "text", "text": s, "marks": [{"type": "strong"}]}]})).collect();
        docs.push(("adf", Box::new(Adf), json!({"type": "doc", "version": 1, "content": ad})));
        let nb: Vec<_> = LANGS.iter().map(|(_, s)| json!({"object": "block", "id": "i", "type": "paragraph", "paragraph": {"rich_text": [{"type": "text", "text": {"content": s}, "plain_text": s}]}})).collect();
        docs.push(("notion", Box::new(Notion), json!(nb)));
        for (id, f, v) in docs {
            let mut text = serde_json::to_string_pretty(&v).unwrap();
            if ascii {
                text = text.chars().map(|c| if (c as u32) < 0x80 { c.to_string() } else { let mut b = [0u16; 2]; c.encode_utf16(&mut b).iter().map(|u| format!("\\u{u:04x}")).collect() }).collect();
            }
            let mut d = open(&*f, text.as_bytes());
            assert_eq!(d.units.len(), 11, "{id}");
            assert_eq!(f.merge(&d).unwrap(), text.as_bytes(), "{id} ascii={ascii}: untouched");
            for (u, (_, s)) in d.units.iter().zip(&LANGS) {
                assert_eq!(plain_text(&u.source()), *s, "{id}");
            }
            for i in 0..11 {
                let mut t = vec![Inline::text("«")];
                t.extend(d.units[i].source());
                t.push(Inline::text("»"));
                let sid = d.units[i].segments[0].id.clone();
                d.set_target(&sid, t).unwrap();
            }
            let back = open(&*f, &f.merge(&d).unwrap());
            for (u, (_, s)) in back.units.iter().zip(&LANGS) {
                assert_eq!(plain_text(&u.source()), format!("«{s}»"), "{id} ascii={ascii}");
            }
        }
    }
}

#[test]
fn sniffing_is_strict_and_options_are_declared() {
    let sniffs: Vec<Box<dyn Filter>> = vec![Box::new(Contentful), Box::new(Sanity), Box::new(Adf), Box::new(Notion)];
    for (i, (_, _, data)) in all().into_iter().enumerate() {
        for (j, f) in sniffs.iter().enumerate() {
            assert_eq!(f.sniff(data), i == j, "filter {j} on fixture {i}");
        }
    }
    for f in &sniffs {
        for junk in [&b""[..], b"{}", b"[]", b"[1,2]", b"hello", b"{\"a\":", b"\xff\xfe"] {
            assert!(!f.sniff(junk));
        }
        assert!(f.extensions().iter().all(|e| e.chars().all(|c| c.is_ascii_lowercase())));
    }
    assert!(Adf.format_option_keys().iter().any(|k| k.key == "adf.units"));
    assert!(Contentful.format_option_keys().is_empty());
}

#[test]
fn typed_errors_for_broken_json() {
    for f in [&Contentful as &dyn Filter, &Sanity, &Adf, &Notion] {
        for bad in [&b"{"[..], b"[1,]", b"{\"a\" 1}", b"\"\\ud800\"", b"[\"a\nb\"]", b"{\"nodeType\": \"document\"} x", b"01", b"[1.]"] {
            assert!(matches!(f.extract(bad, &opts()), Err(Error::Parse { .. })), "{:?}", String::from_utf8_lossy(bad));
        }
    }
    let deep = "[".repeat(5000);
    assert!(matches!(Sanity.extract(deep.as_bytes(), &opts()), Err(Error::Parse { .. })));
    let limits = ExtractOptions::new().with_limits(Limits::new().with_max_input_bytes(10));
    assert!(matches!(Contentful.extract(CONTENTFUL, &limits), Err(Error::Limit(_))));
    let few = ExtractOptions::new().with_limits(Limits::new().with_max_units(3));
    assert!(matches!(Contentful.extract(CONTENTFUL, &few), Err(Error::Limit(_))));
}

#[test]
fn merge_checks_the_skeleton() {
    let mut d = open(&Contentful, CONTENTFUL);
    let mut newer = d.clone();
    if let Skeleton::Text { parser, .. } = &mut newer.skeleton {
        parser.as_mut().unwrap().schema += 1;
    }
    assert!(matches!(Contentful.merge(&newer), Err(Error::ParserVersion { .. })));
    d.units[0].segments[0].target = vec![Inline::text("x")];
    assert!(Sanity.merge(&d).is_err());
    d.units.pop();
    assert!(Contentful.merge(&d).is_err(), "the unit count no longer matches the original");
}

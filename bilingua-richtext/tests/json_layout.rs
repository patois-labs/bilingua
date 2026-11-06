//! The JSON reader and the splice writer: bytes outside the translated values stay as they were (any layout, number
//! spellings, escapes, duplicate keys), the lexical edge cases LocOS's `json.loads` reads differently are typed
//! errors, and reading stays linear.
mod common;
use bilingua_model::*;
use bilingua_richtext::*;
use common::*;

const ODD: &str = "[ {\"object\":\"block\" ,  \"id\": \"b1\",\"n\" : 12345678901234567890123456789, \"f\":1.0E+5,\"g\": -0.0e-0,\n\
   \"type\":\"paragraph\",\t\"paragraph\": {\"rich_text\":[ {\"type\":\"text\",\"text\":{ \"content\" :\"Hello w\\u00f6rld\" ,\"link\":null},\n\
        \"plain_text\":\"Hello\\/x \u{e9}\"}], \"color\":\"default\"}},\r\n  {\"object\":\"block\",\"id\":\"b2\",\"type\":\"paragraph\",\"paragraph\":{\"rich_text\":[]}} ]\n\n";

#[test]
fn bytes_outside_the_translated_values_stay() {
    let f = Notion;
    let mut d = open(&f, ODD.as_bytes());
    assert_eq!(texts(&d), ["Hello w\u{f6}rld"]);
    assert_eq!(f.merge(&d).unwrap(), ODD.as_bytes());
    d.set_target("u1", vec![Inline::text("Hallo")]).unwrap();
    let out = String::from_utf8(f.merge(&d).unwrap()).unwrap();
    // only the two strings Notion rewrites changed; the file mixes literal and escaped non-ASCII, so new text is
    // written literally
    let want = ODD.replace("\"Hello w\\u00f6rld\"", "\"Hallo\"").replace("\"Hello\\/x \u{e9}\"", "\"Hallo\"");
    assert_eq!(out, want);
}

#[test]
fn a_rebuilt_container_is_written_in_place_and_the_rest_is_untouched() {
    let f = Adf;
    let src = "{\"type\":\"doc\",  \"version\" : 1,\n \"content\": [\n    {\"type\": \"paragraph\", \"content\": [{\"type\":\"text\",\"text\":\"hi \"},{\"type\":\"text\",\"text\":\"there\",\"marks\":[{\"type\":\"strong\"}]}]},\n    {\"type\":\"rule\" , \"attrs\":{\"x\":1.50}}\n ]}";
    let mut d = open(&f, src.as_bytes());
    assert_eq!(texts(&d), [tk("hi Gg1HthereG/g1H")]);
    set(&mut d, 0, &tk("Gg1HDortG/g1H hallo")).unwrap();
    let out = String::from_utf8(f.merge(&d).unwrap()).unwrap();
    let head = "{\"type\":\"doc\",  \"version\" : 1,\n \"content\": [\n    {\"type\": \"paragraph\", \"content\": ";
    let tail = "},\n    {\"type\":\"rule\" , \"attrs\":{\"x\":1.50}}\n ]}";
    assert!(out.starts_with(head) && out.ends_with(tail), "{out}");
    let v = json(out.as_bytes());
    assert_eq!(v["content"][0]["content"][0]["text"], "Dort");
    assert_eq!(v["content"][0]["content"][1]["text"], " hallo");
    // the one-line container stays on one line
    assert!(!out[head.len()..out.len() - tail.len()].contains('\n'), "{out}");
}

#[test]
fn duplicate_keys_read_and_write_the_last_copy() {
    let f = Notion;
    let src = "[{\"id\":\"b\",\"type\":\"paragraph\",\"paragraph\":{\"rich_text\":[{\"type\":\"text\",\"text\":{\"content\":\"first\"}}],\"rich_text\":[{\"type\":\"text\",\"text\":{\"content\":\"second\"}}]}}]";
    let mut d = open(&f, src.as_bytes());
    assert_eq!(texts(&d), ["second"]);
    d.set_target("u1", vec![Inline::text("zweite")]).unwrap();
    let out = String::from_utf8(f.merge(&d).unwrap()).unwrap();
    assert!(out.contains("\"content\":\"first\""), "{out}");
    assert!(out.contains("\"content\":\"zweite\""), "{out}");
    assert!(!out.contains("second"));
}

#[test]
fn lexical_edge_cases() {
    // Python's json.loads takes these; Bilingua refuses them with a typed error (a lone surrogate cannot be text in
    // Rust, and NaN/Infinity are not JSON)
    for bad in ["[\"\\ud800\"]", "[\"\\udc00x\"]", "[\"\\ud800\\u0041\"]", "[NaN]", "[Infinity]", "[-Infinity]", "[1e]", "[-]", "[01]", "[.5]", "[1.]", "[\"\\x\"]", "[\"\t\"]", "[tru]", "{\"a\":1,}", "[1 2]"] {
        assert!(matches!(Sanity.extract(bad.as_bytes(), &opts()), Err(Error::Parse { .. })), "{bad}");
    }
    // big integers, exponent spellings, a surrogate pair and every escape are read and kept
    let src = "[{\"_type\":\"block\",\"_key\":\"k\",\"n\":[123456789012345678901234567890,1E400,-1.25e-7,0,-0],\"children\":[{\"_type\":\"span\",\"text\":\"a\\\"\\\\\\/\\b\\f\\n\\r\\t\\u00e9\\ud83d\\ude00 b\",\"marks\":[]}]}]";
    let mut d = open(&Sanity, src.as_bytes());
    assert_eq!(texts(&d), ["a\"\\/\u{8}\u{c}\n\r\t\u{e9}\u{1f600} b"]);
    assert_eq!(Sanity.merge(&d).unwrap(), src.as_bytes());
    d.set_target("u1", vec![Inline::text("\u{e9}\u{1f600}\"\n")]).unwrap();
    let out = String::from_utf8(Sanity.merge(&d).unwrap()).unwrap();
    assert!(out.contains("[123456789012345678901234567890,1E400,-1.25e-7,0,-0]"), "{out}");
    // the file wrote non-ASCII as escapes only, so the new text is escaped too
    assert!(out.contains("\"text\":\"\\u00e9\\ud83d\\ude00\\\"\\n\""), "{out}");
    // nesting: 256 levels read, 257 refused
    let ok = format!("[{}0{}]", "[".repeat(255), "]".repeat(255));
    assert!(Sanity.extract(ok.as_bytes(), &opts()).is_ok());
    let deep = format!("[{}0{}]", "[".repeat(257), "]".repeat(257));
    assert!(matches!(Sanity.extract(deep.as_bytes(), &opts()), Err(Error::Parse { .. })));
}

#[test]
fn reading_is_linear() {
    // wide objects (the duplicate-key index), long strings, long arrays and escapes: work proportional to the size
    let mut times = Vec::new();
    for n in [20_000usize, 160_000] {
        let wide = format!("{{\"type\":\"doc\",\"content\":[],{}}}", (0..n).map(|i| format!("\"k{i}\":{i}")).collect::<Vec<_>>().join(","));
        let long = format!("[{{\"_type\":\"block\",\"children\":[{{\"_type\":\"span\",\"text\":\"{}\",\"marks\":[]}}]}}]", "ab\\u00e9 ".repeat(n / 16));
        let arr = format!("[{}]", vec!["{\"x\":[1,2,{\"y\":\"z\"}]}"; n].join(","));
        let t0 = std::time::Instant::now();
        assert!(Adf.extract(wide.as_bytes(), &opts()).is_ok());
        assert_eq!(Sanity.extract(long.as_bytes(), &opts()).unwrap().units.len(), 1);
        assert!(Notion.extract(arr.as_bytes(), &opts()).is_ok());
        times.push(t0.elapsed().as_secs_f64());
    }
    // 8x the input: allow 40x for noise on a shared machine, a quadratic reader would be 64x
    assert!(times[1] < times[0] * 40.0 + 0.5, "{times:?}");
}

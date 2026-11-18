//! `SmallMap` and `TextMap` against `BTreeMap` (random operation sequences), and the unit accessors against the
//! types that will replace the fields (unit model step 1).
//!
//! The accessor tests run the same caller code on `unit.codes_mut()` (a view of today's `BTreeMap` field) and on a
//! `Codes`, and on `ctx.extra_mut()` and a `TextMap`. Code that compiles and behaves the same on both keeps
//! compiling and behaving the same when the fields become `Codes` and `TextMap`.

use bilingua_model::*;
use std::collections::BTreeMap;

/// splitmix64: a small deterministic generator, so a failure names its seed and reruns the same way.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// Keys that collide often and sort in ways a char or locale order would get wrong.
const KEYS: [&str; 14] = ["1", "10", "2", "_o", "_base", "~x", "$raw", "a", "B", "b", "é", "\u{10000}", "", "zz"];

fn key(r: &mut Rng) -> &'static str {
    KEYS[r.below(KEYS.len() as u64) as usize]
}

fn batch(r: &mut Rng) -> Vec<(String, u32)> {
    (0..r.below(6)).map(|_| (key(r).to_string(), r.below(100) as u32)).collect()
}

fn same<V: PartialEq + std::fmt::Debug + serde::Serialize>(s: &SmallMap<V>, b: &BTreeMap<String, V>, seed: u64, step: usize) {
    let sv: Vec<(&str, &V)> = s.iter().collect();
    let bv: Vec<(&str, &V)> = b.iter().map(|(k, v)| (k.as_str(), v)).collect();
    assert_eq!(sv, bv, "seed {seed} step {step}");
    assert_eq!(s.len(), b.len(), "seed {seed} step {step}");
    assert_eq!(s.is_empty(), b.is_empty());
    assert_eq!(serde_json::to_string(s).unwrap(), serde_json::to_string(b).unwrap(), "seed {seed} step {step}");
    assert_eq!(format!("{s:?}"), format!("{b:?}"));
}

#[test]
fn small_map_matches_btreemap_on_random_sequences() {
    for seed in 0..400u64 {
        let mut r = Rng(seed);
        let mut s: SmallMap<u32> = SmallMap::new();
        let mut b: BTreeMap<String, u32> = BTreeMap::new();
        for step in 0..120 {
            let k = key(&mut r);
            let v = r.below(1000) as u32;
            match r.below(100) {
                0..=34 => assert_eq!(s.insert(k, v), b.insert(k.to_string(), v), "seed {seed} step {step}"),
                35..=49 => assert_eq!(s.remove(k), b.remove(k), "seed {seed} step {step}"),
                50..=64 => {
                    assert_eq!(s.get(k), b.get(k));
                    assert_eq!(s.contains_key(k), b.contains_key(k));
                    assert_eq!(s.get_key_value(k), b.get_key_value(k).map(|(k, v)| (k.as_str(), v)));
                    if b.contains_key(k) {
                        assert_eq!(s[k], b[k]);
                    }
                }
                65..=74 => {
                    *s.get_or_insert_with(k, || v) += 1;
                    *b.entry(k.to_string()).or_insert(v) += 1;
                }
                75..=79 => {
                    if let (Some(x), Some(y)) = (s.get_mut(k), b.get_mut(k)) {
                        *x += 7;
                        *y += 7;
                    }
                }
                80..=89 => {
                    let new = batch(&mut r);
                    s.extend(new.clone());
                    b.extend(new);
                }
                90..=93 => {
                    let m = r.below(3) as u32 + 2;
                    s.retain(|_, v| *v % m != 0);
                    b.retain(|_, v| *v % m != 0);
                }
                94..=95 => {
                    for (_, v) in s.iter_mut() {
                        *v = v.wrapping_mul(3);
                    }
                    for v in b.values_mut() {
                        *v = v.wrapping_mul(3);
                    }
                }
                96 => {
                    s.clear();
                    b.clear();
                }
                _ => {
                    // rebuilt three ways, all equal to the reference
                    assert_eq!(SmallMap::from(b.clone()), s);
                    assert_eq!(b.clone().into_iter().collect::<SmallMap<u32>>(), s);
                    assert_eq!(BTreeMap::from(s.clone()), b);
                }
            }
            same(&s, &b, seed, step);
        }
    }
}

#[test]
fn collect_and_deserialize_match_btreemap_with_repeated_keys() {
    for seed in 0..500u64 {
        let mut r = Rng(seed ^ 0xA5A5);
        let items: Vec<(String, u32)> = (0..r.below(30)).map(|_| (key(&mut r).to_string(), r.below(1000) as u32)).collect();
        let s: SmallMap<u32> = items.iter().cloned().collect();
        let b: BTreeMap<String, u32> = items.iter().cloned().collect();
        same(&s, &b, seed, 0);
        // a JSON object with repeated keys, written by hand: last wins in both
        let body: Vec<String> = items.iter().map(|(k, v)| format!("{}:{v}", serde_json::to_string(k).unwrap())).collect();
        let json = format!("{{{}}}", body.join(","));
        let sj: SmallMap<u32> = serde_json::from_str(&json).unwrap();
        let bj: BTreeMap<String, u32> = serde_json::from_str(&json).unwrap();
        same(&sj, &bj, seed, 1);
        // and the round trip is exact
        let out = serde_json::to_string(&sj).unwrap();
        assert_eq!(serde_json::to_string(&serde_json::from_str::<SmallMap<u32>>(&out).unwrap()).unwrap(), out);
    }
}

#[test]
fn text_map_matches_btreemap_on_random_sequences() {
    for seed in 0..300u64 {
        let mut r = Rng(seed ^ 0x7E57);
        let mut t = TextMap::new();
        let mut b: BTreeMap<String, String> = BTreeMap::new();
        for step in 0..100 {
            let k = key(&mut r);
            let v = format!("v{}", r.below(50));
            match r.below(10) {
                0..=3 => assert_eq!(t.insert(k, v.as_str()).map(String::from), b.insert(k.to_string(), v)),
                4..=5 => assert_eq!(t.remove(k).map(String::from), b.remove(k)),
                6 => assert_eq!(t.get_or_insert_with(k, || v.clone()), b.entry(k.to_string()).or_insert(v).as_str()),
                7 => {
                    let new: Vec<(String, String)> = batch(&mut r).into_iter().map(|(k, v)| (k, v.to_string())).collect();
                    t.extend(new.clone());
                    b.extend(new);
                }
                8 => {
                    t.retain(|k, _| k.len() % 2 == 0);
                    b.retain(|k, _| k.len() % 2 == 0);
                }
                _ => {
                    assert_eq!(t.get(k), b.get(k).map(String::as_str));
                    assert_eq!(t.contains_key(k), b.contains_key(k));
                }
            }
            let tv: Vec<(&str, &str)> = t.iter().collect();
            let bv: Vec<(&str, &str)> = b.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            assert_eq!(tv, bv, "seed {seed} step {step}");
            assert_eq!(t.len(), b.len());
            assert_eq!(serde_json::to_string(&t).unwrap(), serde_json::to_string(&b).unwrap(), "seed {seed} step {step}");
            assert_eq!(TextMap::from(b.clone()), t);
        }
    }
}

/// Random operations on any map with the `Codes` API: what a format crate does to a unit's codes.
macro_rules! codes_ops {
    ($m:expr, $r:expr) => {{
        let m = $m;
        let r: &mut Rng = $r;
        let mut log: Vec<String> = Vec::new();
        for _ in 0..80 {
            let k = key(r);
            let v = r.below(100);
            match r.below(10) {
                0..=2 => log.push(format!("{:?}", m.insert(k.to_string(), CodeData::standalone(format!("<x{v}/>"))))),
                3 => log.push(format!("{:?}", m.remove(k))),
                4 => m.get_or_insert_with(k, CodeData::new).set_label(format!("l{v}").as_str()),
                5 => {
                    if let Some(c) = m.get_mut(k) {
                        c.set_open(Some("<b>"));
                    }
                }
                6 => m.extend(vec![(k.to_string(), CodeData::paired("<i>", "</i>")), ("2".to_string(), CodeData::new())]),
                7 => m.retain(|k, c| !k.is_empty() || c.label.is_some()),
                8 => {
                    for (_, c) in m.iter_mut() {
                        c.set_close(None::<&str>);
                    }
                }
                _ => log.push(format!("{:?} {:?} {}", m.get(k), m.get_key_value(k), m.contains_key(k))),
            }
            let seen: Vec<String> = m.iter().map(|(k, c)| format!("{k}={c:?}")).collect();
            log.push(seen.join(";"));
            log.push(format!("{} {} {:?}", m.len(), m.is_empty(), m.keys().collect::<Vec<&str>>()));
        }
        log
    }};
}

#[test]
fn codes_accessor_behaves_like_codes() {
    for seed in 0..200u64 {
        let mut unit = Unit::new("u1", vec![Inline::text("x")]);
        let mut codes = Codes::new();
        let a = codes_ops!(unit.codes_mut(), &mut Rng(seed));
        let b = codes_ops!(&mut codes, &mut Rng(seed));
        assert_eq!(a, b, "seed {seed}");
        assert_eq!(*unit.codes(), codes);
        assert_eq!(codes, *unit.codes());
        assert_eq!(serde_json::to_string(unit.codes()).unwrap(), serde_json::to_string(&codes).unwrap());
        assert_eq!(format!("{:?}", unit.codes()), format!("{codes:?}"));
        assert_eq!(Codes::from(unit.codes()), codes);
    }
}

/// Random operations on any map with the `TextMap` API: what a format crate does to `context.extra`.
macro_rules! text_ops {
    ($m:expr, $r:expr) => {{
        let m = $m;
        let r: &mut Rng = $r;
        let mut log: Vec<String> = Vec::new();
        for _ in 0..80 {
            let k = key(r);
            let v = format!("v{}", r.below(20));
            match r.below(8) {
                0..=2 => log.push(format!("{:?}", m.insert(k, v))),
                3 => log.push(format!("{:?}", m.remove(k))),
                4 => log.push(m.get_or_insert_with(k.to_string(), || "new").to_string()),
                5 => m.extend([(k, "e")]),
                6 => m.retain(|_, v| v != "v3"),
                _ => log.push(format!("{:?} {}", m.get(k), m.contains_key(k))),
            }
            log.push(m.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(";"));
            log.push(format!("{} {:?} {:?}", m.len(), m.keys().collect::<Vec<&str>>(), m.values().collect::<Vec<&str>>()));
        }
        log
    }};
}

#[test]
fn extra_and_hidden_accessors_behave_like_text_map() {
    for seed in 0..200u64 {
        let mut ctx = UnitContext::new();
        let mut unit = Unit::new("u1", vec![]);
        let mut t = TextMap::new();
        let a = text_ops!(ctx.extra_mut(), &mut Rng(seed));
        let h = text_ops!(unit.hidden_map_mut(), &mut Rng(seed));
        let b = text_ops!(&mut t, &mut Rng(seed));
        assert_eq!(a, b, "seed {seed}");
        assert_eq!(h, b, "seed {seed}");
        assert_eq!(*ctx.extra(), t);
        assert_eq!(t, *unit.hidden_map());
        assert_eq!(serde_json::to_string(ctx.extra()).unwrap(), serde_json::to_string(&t).unwrap());
        assert_eq!(TextMap::from(ctx.extra()), t);
    }
}

/// Read-only caller code as a format crate or a consumer writes it. It compiles against `unit.codes()` and against
/// a `&Codes`, so it keeps compiling when `codes()` returns `&Codes`.
macro_rules! read_codes {
    ($c:expr) => {{
        let c = $c;
        let id = String::from("1");
        let mut out = vec![];
        out.push(c["1"].label.as_deref().unwrap_or_default().to_string());
        out.push(c[&id].open.as_deref().unwrap_or_default().to_string());
        out.push(format!("{:?}", c.get(&id).and_then(|x| x.close.as_deref())));
        out.push(format!("{:?}", c.get(id.as_str()).is_some()));
        out.push(format!("{}", c.contains_key("_o")));
        for (k, code) in c {
            out.push(format!("{k}:{}", code.standalone.as_deref().unwrap_or("-")));
        }
        for (k, _) in c.iter().rev() {
            out.push(k.to_string());
        }
        let keys: Vec<&str> = c.keys().collect();
        out.push(keys.join(","));
        out.push(c.values().filter(|x| x.label.is_some()).count().to_string());
        out.push(format!("{} {}", c.len(), c.is_empty()));
        out.push(c.iter().len().to_string());
        let owned: Codes = Codes::from(c);
        out.push(serde_json::to_string(&owned).unwrap());
        out.push(serde_json::to_string(c).unwrap());
        let picked: Codes = c.iter().filter(|(k, _)| !k.starts_with('_')).map(|(k, v)| (k, v.clone())).collect();
        out.push(format!("{picked:?}"));
        out
    }};
}

/// The same for `context.extra` and hidden values.
macro_rules! read_text {
    ($t:expr) => {{
        let t = $t;
        let k = String::from("part");
        let mut out = vec![];
        out.push(t["part"].to_string());
        out.push(t[&k].len().to_string());
        out.push(format!("{:?}", t.get(&k)));
        out.push(format!("{}", t.get("cells") == Some("A1")));
        out.push(format!("{:?}", t.get("plural").map(|p| p.parse::<usize>())));
        for (k, v) in t {
            out.push(format!("{k}={v}"));
        }
        out.push(t.keys().collect::<Vec<_>>().join(","));
        out.push(t.values().collect::<Vec<_>>().join(","));
        out.push(format!("{} {} {}", t.len(), t.is_empty(), t.contains_key("cells")));
        out.push(serde_json::to_string(t).unwrap());
        out.push(format!("{:?}", TextMap::from(t)));
        out
    }};
}

#[test]
fn read_only_callers_compile_and_agree_on_both() {
    let codes: Codes = [
        ("1", CodeData::paired("<b>", "</b>").with_label("bold")),
        ("2", CodeData::standalone("<br/>")),
        ("_o", CodeData::standalone("<w:r/>")),
    ]
    .into();
    let unit = Unit::new("u", vec![]).with_codes(codes.clone());
    assert_eq!(read_codes!(unit.codes()), read_codes!(&codes));

    let extra = TextMap::from([("part", "slide1.xml"), ("cells", "A1"), ("plural", "2")]);
    let ctx = UnitContext::new().with_extra(extra.clone());
    let unit = Unit::new("u", vec![]).with_context(ctx.clone()).with_hidden("part", "x").with_hidden("cells", "A1");
    assert_eq!(read_text!(ctx.extra()), read_text!(&extra));
    let hidden = TextMap::from([("part", "x"), ("cells", "A1")]);
    assert_eq!(read_text!(unit.hidden_map()), read_text!(&hidden));
}

#[test]
fn setters_take_codes_text_maps_and_btreemaps() {
    let mut u = Unit::new("u", vec![]);
    u.set_codes(Codes::from([("1", CodeData::standalone("<x/>"))]));
    assert_eq!(u.codes.len(), 1);
    u.set_codes(BTreeMap::from([("2".to_string(), CodeData::new()), ("1".to_string(), CodeData::new())]));
    assert_eq!(u.codes().keys().collect::<Vec<_>>(), ["1", "2"]);
    let u = u.with_codes(BTreeMap::<String, CodeData>::new());
    assert!(u.codes().is_empty());
    let mut u = u.with_codes([("a", CodeData::new())]);
    u.set_hidden_map(TextMap::from([("raw", "x")]));
    assert_eq!(u.hidden("raw"), Some("x"));
    u.set_hidden_map(BTreeMap::new());
    assert!(u.hidden_map().is_empty());

    let mut ctx = UnitContext::new().with_extra(BTreeMap::from([("story".to_string(), "s1".to_string())]));
    assert_eq!(ctx.extra().get("story"), Some("s1"));
    ctx.set_extra(TextMap::from([("cells", "A1")]));
    assert_eq!(ctx.extra.get("cells").map(String::as_str), Some("A1"));
    assert!(!ctx.extra().contains_key("story"));
    let ctx = ctx.with_extra([("a", "b")]);
    assert_eq!(ctx.extra().iter().collect::<Vec<_>>(), [("a", "b")]);
}

#[test]
fn scalar_accessors_read_and_set_the_fields() {
    let mut ctx = UnitContext::new().with_kind("heading").with_style("Heading 1").with_key("k").with_heading("Intro").with_location("p 3");
    assert_eq!((ctx.kind(), ctx.style(), ctx.key(), ctx.heading(), ctx.location()), (Some("heading"), Some("Heading 1"), Some("k"), Some("Intro"), Some("p 3")));
    ctx.set_kind("cell");
    ctx.set_style(None::<&str>);
    ctx.set_key(String::from("k2"));
    ctx.set_heading(Some("Next"));
    ctx.set_location(None::<String>);
    assert_eq!((ctx.kind(), ctx.style(), ctx.key(), ctx.heading(), ctx.location()), (Some("cell"), None, Some("k2"), Some("Next"), None));

    let mut c = CodeData::new();
    c.set_open("<b>");
    c.set_close(String::from("</b>"));
    c.set_standalone(Some("<x/>"));
    c.set_label("bold");
    assert_eq!(c, CodeData::paired("<b>", "</b>").with_standalone("<x/>").with_label("bold"));
    c.set_label(None::<&str>);
    assert_eq!(c.label, None);
}

#[test]
fn accessors_write_through_to_the_fields() {
    let mut u = Unit::new("u", vec![]);
    u.codes_mut().insert("1", CodeData::standalone("<x/>"));
    u.hidden_map_mut().insert("raw", "v");
    u.context.extra_mut().insert("part", "a.xml");
    assert_eq!(u.codes.get("1").and_then(|c| c.standalone.as_deref()), Some("<x/>"));
    assert_eq!(u.hidden.get("raw").map(String::as_str), Some("v"));
    assert_eq!(u.context.extra.get("part").map(String::as_str), Some("a.xml"));
    // and the JSON is what the fields would write
    let json = serde_json::to_string(&u).unwrap();
    assert!(json.contains(r#""codes":{"1":{"open":null,"close":null,"standalone":"<x/>","label":null}}"#), "{json}");
    assert!(json.contains(r#""extra":{"part":"a.xml"}"#), "{json}");
    assert!(json.contains(r#""hidden":{"raw":"v"}"#), "{json}");
}

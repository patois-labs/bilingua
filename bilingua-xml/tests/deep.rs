//! Deep nesting on a 1 MB stack (wasm's default) in a debug build: the planner, the skeleton emitter and the merge
//! use explicit stacks, so every shape of nesting up to the engine's 1024-level cap extracts and merges, and deeper
//! files are a typed error, never a stack overflow.

use bilingua_model::*;
use bilingua_xml::XmlFilter;

fn pseudo(doc: &mut Document) {
    for u in &mut doc.units {
        let mut t = vec![Inline::text("[")];
        t.extend(u.source());
        t.push(Inline::text("]"));
        u.segments[0].target = t;
    }
}

fn nest(open: &str, close: &str, n: usize, inner: &str) -> String {
    format!("<topic><body><p>{}{inner}{}</p></body></topic>", open.repeat(n), close.repeat(n))
}

/// Shapes that each take a different path through the engine: inline pairs, block containers, nested units
/// (`fn` in `fn`, rendered by the merge one inside the other), translate no / yes switches, and inline / protected
/// alternation (planner work list inside a run).
fn shapes(n: usize) -> Vec<(&'static str, String)> {
    vec![
        ("inline ph", nest("<ph>", "</ph>", n, "x")),
        ("blocks", format!("<topic>{}x{}</topic>", "<div>".repeat(n), "</div>".repeat(n))),
        ("nested fn", nest("a <fn>", "</fn>", n, "z")),
        ("no/yes", nest("a <ph translate=\"no\">b <ph translate=\"yes\">", "</ph></ph>", n / 2, "z")),
        ("ph/codeph", nest("a <ph>b <codeph>c <ph>", "</ph></codeph></ph>", n / 3, "z")),
        ("fn/ph", nest("a <fn>b <ph>", "</ph></fn>", n / 2, "z")),
    ]
}

fn on_small_stack<F: FnOnce() + Send + 'static>(f: F) {
    std::thread::Builder::new().stack_size(1 << 20).spawn(f).unwrap().join().expect("no stack overflow");
}

#[test]
fn every_shape_at_the_cap_extracts_and_merges_on_a_1_mb_stack() {
    on_small_stack(|| {
        let f = XmlFilter::new();
        let o = ExtractOptions::new().with_limits(Limits::new().with_max_xml_depth(1_020)).with_target_lang("de-DE");
        for (name, src) in shapes(1_010) {
            let mut d = f.extract(src.as_bytes(), &o).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(f.merge(&d).unwrap(), src.as_bytes(), "{name}: untouched merge");
            let n = d.units.len();
            pseudo(&mut d);
            let out = f.merge(&d).unwrap_or_else(|e| panic!("{name}: {e}"));
            let back = f.extract(&out, &o).unwrap_or_else(|e| panic!("{name} read back: {e}"));
            assert_eq!(back.units.len(), n, "{name}: units after a translated merge");
        }
    });
}

#[test]
fn past_the_cap_is_a_typed_error_on_a_1_mb_stack() {
    on_small_stack(|| {
        let f = XmlFilter::new();
        let o = ExtractOptions::new().with_limits(Limits::new().with_max_xml_depth(1_000_000));
        for (name, src) in shapes(60_000) {
            let r = f.extract(src.as_bytes(), &o);
            assert!(matches!(r, Err(Error::Limit(ref e)) if e.kind == LimitKind::XmlDepth), "{name}: {:?}", r.err());
        }
    });
}

#[test]
fn unit_keys_stay_short_under_a_deep_chain_of_long_names() {
    // 250 levels of 1 000-character names over 20 000 units: without a cap the keys alone would be 5 GB
    let name = "n".repeat(1000);
    let src = format!("<r>{}{}{}</r>", format!("<{name}>").repeat(250), "<p>x</p>".repeat(20_000), format!("</{name}>").repeat(250));
    let d = XmlFilter::new().extract(src.as_bytes(), &ExtractOptions::new()).unwrap();
    assert_eq!(d.units.len(), 20_000);
    let total: usize = d.units.iter().map(|u| u.context.key.as_deref().map_or(0, str::len)).sum();
    assert!(total <= 20_000 * 520, "{total}");
    let k = d.units[0].context.key.as_deref().unwrap();
    assert!(k.starts_with("/\u{2026}/") && k.ends_with("/p"), "{k}");
    let short = XmlFilter::new().extract(b"<r><a><p>x</p></a></r>", &ExtractOptions::new()).unwrap();
    assert_eq!(short.units[0].context.key.as_deref(), Some("/r/a/p"));
}

#[test]
fn a_skeleton_that_uses_a_unit_twice_is_refused_not_looped() {
    let f = XmlFilter::new();
    let src = b"<topic><body><p>a <fn>b <fn>c</fn></fn></p></body></topic>";
    let d = f.extract(src, &ExtractOptions::new()).unwrap();
    assert_eq!(d.units.len(), 3);
    // a unit whose markup names itself (a cycle)
    let mut cyc = d.clone();
    cyc.units[1].set_hidden("raw", "b \u{0}U1\u{0}");
    assert!(matches!(f.merge(&cyc), Err(Error::Write(_))));
    // fan-out: each unit names the next one twice (2^n work without the guard)
    let many: String = format!("<topic><body><p>{}z{}</p></body></topic>", "a <fn>".repeat(60), "</fn>".repeat(60));
    let mut fan = f.extract(many.as_bytes(), &ExtractOptions::new()).unwrap();
    for i in 0..fan.units.len() - 1 {
        let next = format!("\u{0}U{}\u{0}", i + 1);
        fan.units[i].set_hidden("raw", format!("a {next}{next}"));
    }
    assert!(matches!(f.merge(&fan), Err(Error::Write(_))));
}

//! A generated DITA topic of more than 5 MB. The real timing (release build, against the LocOS filter) is
//! `examples/dita_dump.rs` (`gen` and `time`) and `tests/tools/speed_locos.py`; this test keeps the size, the round
//! trip and a generous time budget honest in every build.

mod gen;

use bilingua_dita::Dita;
use bilingua_model::*;

#[test]
fn five_megabytes_round_trip() {
    let data = gen::topic(5_200_000).into_bytes();
    assert!(data.len() > 5_000_000);
    let f = Dita::new();
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_units(10_000_000).with_max_xml_elements(100_000_000)).with_target_lang("fr");
    let t0 = std::time::Instant::now();
    let mut d = f.extract(&data, &o).unwrap();
    assert!(d.units.len() > 50_000);
    assert_eq!(f.merge(&d).unwrap(), data);
    for u in &mut d.units {
        let mut t = vec![Inline::text("[")];
        t.extend(u.source());
        t.push(Inline::text("]"));
        u.segments[0].target = t;
    }
    let out = f.merge(&d).unwrap();
    let took = t0.elapsed().as_secs_f64();
    let back = f.extract(&out, &o).unwrap();
    assert_eq!(back.units.len(), d.units.len());
    let head = String::from_utf8_lossy(&out[..400]).to_string();
    assert!(head.contains("<topic id=\"big\" xml:lang=\"fr\">"), "{head}");
    eprintln!("5 MB DITA: {} units, extract + merge + translated merge in {took:.2} s (this build)", d.units.len());
    assert!(took < 120.0, "far too slow: {took:.1} s");
}

/// Issue 792: one start tag with tens of thousands of `xmlns:pN` declarations and as many `pN:a` attributes must not
/// make the scan quadratic (libxml2 took 27 s on 40 000).
#[test]
fn many_namespace_declarations_on_one_tag_stay_fast() {
    let n = 40_000;
    let mut s = String::from("<topic id=\"t\"");
    for i in 0..n {
        s.push_str(&format!(" xmlns:p{i}=\"u{i}\""));
    }
    for i in 0..n {
        s.push_str(&format!(" p{i}:a=\"v\""));
    }
    s.push_str("><title>Hi</title></topic>");
    let t = std::time::Instant::now();
    let _ = Dita::new().extract(s.as_bytes(), &ExtractOptions::new());
    assert!(t.elapsed().as_secs() < 5, "took {:?}", t.elapsed());
}

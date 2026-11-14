//! Differential helper for `tests/speed/diff_locos.py`: reads the ARB file named by `ARB_DIFF_IN`, writes the unit
//! texts (as LocOS lists them) to `ARB_DIFF_UNITS` and, when `ARB_DIFF_TARGET` is set, the file merged with every
//! unit translated to "T:" + source for that target language to `ARB_DIFF_MERGED`. Ignored by default; the
//! Python script runs it and compares both outputs with the LocOS filter.

use bilingua_arb::Arb;
use bilingua_model::*;

fn lo(u: &Unit) -> String {
    let mut out = String::new();
    let mut n = 0;
    for i in u.source() {
        match i {
            Inline::Text(t) => out.push_str(&t),
            Inline::Tag { free: true, display, .. } => out.push_str(&display),
            Inline::Tag { .. } => {
                n += 1;
                out.push_str(&format!("\u{27E6}x{n}\u{27E7}"));
            }
        }
    }
    out
}

#[test]
#[ignore = "driven by tests/speed/diff_locos.py"]
fn dump() {
    let Ok(input) = std::env::var("ARB_DIFF_IN") else { return };
    let bytes = std::fs::read(input).unwrap();
    let mut doc = Arb.extract(&bytes, &ExtractOptions::default()).unwrap();
    let units: Vec<String> = doc.units.iter().map(lo).collect();
    std::fs::write(std::env::var("ARB_DIFF_UNITS").unwrap(), serde_json::to_string(&units).unwrap()).unwrap();
    if let Ok(target) = std::env::var("ARB_DIFF_TARGET") {
        doc.target_lang = Some(target);
        for u in doc.units.iter_mut() {
            for s in u.segments.iter_mut() {
                let mut t = vec![Inline::text("T:")];
                t.extend(s.source.clone());
                s.target = merge_text(t);
            }
        }
        std::fs::write(std::env::var("ARB_DIFF_MERGED").unwrap(), Arb.merge(&doc).unwrap()).unwrap();
    }
}

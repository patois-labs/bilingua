//! B-SIDE W6 stf-1 and "fixed cost per call": the phases of a LocOS adapter build (extract under the profile,
//! `locos_units`, the targets read with the format's token rules and set with `locos_set_target`, merge), timed for
//! LocOS's `verify_stf_filter` test_issue_804 file (40 000 Source rows, every one translated, budget 2.0 s for the
//! whole Python build) and for a small file of a few formats (the per-call cost). Prints the phases; in a release
//! build the 40 000-row build must stay well inside LocOS's budget.
use bilingua::profile::tokens;
use bilingua::*;
use std::time::Instant;

fn phases(name: &str, data: &[u8], text_of: impl Fn(usize) -> String) -> [f64; 4] {
    let t0 = Instant::now();
    let opts = profile::locos().extract_options().with_granularity(Granularity::Paragraph);
    let mut doc = extract(name, data, &opts).unwrap();
    let t1 = Instant::now();
    let units = profile::locos_units(&doc);
    let t2 = Instant::now();
    for (i, u) in units.iter().enumerate() {
        let (inl, _, _) = tokens::repair_locos_text_for(&doc.format, &text_of(i), &u.map);
        profile::locos_set_target(&mut doc, &u.segment_id, inl).unwrap();
    }
    let t3 = Instant::now();
    let out = merge_with(&doc, &profile::locos().merge_options(&doc.format)).unwrap();
    let t4 = Instant::now();
    assert!(!out.is_empty());
    let ms = |a: Instant, b: Instant| (b - a).as_secs_f64() * 1e3;
    [ms(t0, t1), ms(t1, t2), ms(t2, t3), ms(t3, t4)]
}

#[test]
fn stf_804_build_is_linear_and_fast() {
    let rows: String = (0..40_000).map(|i| format!("CustomLabel.L{i}\tLabel number {i} text here\n")).collect();
    let big = format!("Language code: en_US\nType: Source\n\n# KEY\tLABEL\n{rows}");
    let p = phases("big.stf", big.as_bytes(), |i| format!("Etikett {i}"));
    let total: f64 = p.iter().sum();
    println!("stf 40000 rows: extract {:.0} ms, locos_units {:.0} ms, set {:.0} ms, merge {:.0} ms, total {total:.0} ms", p[0], p[1], p[2], p[3]);
    if !cfg!(debug_assertions) {
        assert!(total < 500.0, "stf build {total:.0} ms");
    }
}

#[test]
fn small_files_per_call_cost() {
    let catkeys = "1\tenglish\tx-vnd.Speed\t1\n".to_string() + &(0..400).map(|i| format!("Label {i}\tWin\t\tLabel {i}\n")).collect::<String>();
    let sbv: String = (0..400).map(|i| format!("0:00:{:02}.000,0:00:{:02}.500\nCue number {i}\n\n", i % 60, i % 60)).collect();
    let csv = "key,en\n".to_string() + &(0..400).map(|i| format!("k{i},Text number {i}\n")).collect::<String>();
    for (name, data) in [("s.catkeys", catkeys), ("s.sbv", sbv), ("s.csv", csv)] {
        let mut best = [f64::MAX; 4];
        for _ in 0..5 {
            let p = phases(name, data.as_bytes(), |i| format!("Neu {i}"));
            for k in 0..4 {
                best[k] = best[k].min(p[k]);
            }
        }
        println!("{name}: extract {:.2} ms, locos_units {:.2} ms, set {:.2} ms, merge {:.2} ms", best[0], best[1], best[2], best[3]);
    }
}

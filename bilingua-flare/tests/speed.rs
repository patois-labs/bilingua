//! A large generated Flare project (over 5 MB of text), extracted and merged. Run
//! `cargo test --release -p bilingua-flare --test speed -- --nocapture` for the timings. Set `FLARE_SPEED_OUT` to a
//! folder to also save `big.flprjzip`, so the LocOS filter can be timed on the same bytes (`tools/time_locos.py`).
mod common;
use bilingua_flare::Flare;
use bilingua_model::*;
use common::*;
use std::time::Instant;

pub fn big() -> Vec<u8> {
    let words = ["harbor", "lantern", "bread", "window", "morning", "garden", "ticket", "river", "paper", "market"];
    let topics = 50usize;
    let paras = 380usize;
    let mut members: Vec<(String, Vec<u8>)> = vec![("Help.flprj".into(), FLPRJ.as_bytes().to_vec())];
    let mut toc = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<CatapultToc>\n");
    for c in 0..topics {
        let mut s = String::with_capacity(160_000);
        s.push_str(&format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<html xmlns:MadCap=\"http://www.madcapsoftware.com/Schemas/MadCap.xsd\" lang=\"en-us\" xml:lang=\"en-us\">\n<head><title>Topic {c}</title></head>\n<body>\n<h1>Topic {c} of the guide</h1>\n"
        ));
        for p in 0..paras {
            let w = |k: usize| words[(c * 31 + p * 7 + k) % words.len()];
            s.push_str(&format!(
                "<p>Open the <b>{}</b> {} near the <MadCap:xref href=\"t{c}.htm#p{p}\">{}</MadCap:xref> at 9 and bring <MadCap:variable name=\"General.Item\">{}</MadCap:variable> for number {p} of topic {c}, then walk to the <MadCap:conditionalText MadCap:conditions=\"Default.Online\">{}</MadCap:conditionalText> &amp; wait.<MadCap:keyword term=\"{}:{}\" /> <img src=\"i.png\" alt=\"Picture of the {} {p}\" /></p>\n",
                w(1), w(2), w(3), w(4), w(5), w(6), w(7), w(8)
            ));
        }
        s.push_str("</body>\n</html>\n");
        members.push((format!("Content/t{c}.htm"), s.into_bytes()));
        toc.push_str(&format!("  <TocEntry Title=\"Topic {c} of the guide\" Link=\"/Content/t{c}.htm\" />\n"));
    }
    toc.push_str("</CatapultToc>\n");
    members.push(("Project/TOCs/Master.fltoc".into(), toc.into_bytes()));
    let mut var = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<CatapultVariableSet>\n");
    for v in 0..2000 {
        var.push_str(&format!("  <Variable Name=\"V{v}\" EvaluatedDefinition=\"Value {v} here\">Value {v} here</Variable>\n"));
    }
    var.push_str("</CatapultVariableSet>\n");
    members.push(("Project/VariableSets/General.flvar".into(), var.into_bytes()));
    make_zip(&members)
}

#[test]
#[cfg_attr(debug_assertions, ignore = "timed in release: cargo test --release -p bilingua-flare --test speed")]
fn large_project_round_trips_and_is_timed() {
    let bytes = big();
    let (m, _) = members(&bytes);
    let text: usize = m.iter().map(|x| x.1.len()).sum();
    assert!(text >= 5_000_000, "only {text} bytes of content");
    if let Ok(dir) = std::env::var("FLARE_SPEED_OUT") {
        std::fs::write(format!("{dir}/big.flprjzip"), &bytes).unwrap();
    }
    let o = ExtractOptions { target_lang: Some("ru".into()), limits: Some(Limits { max_units: 500_000, ..Limits::default() }), ..Default::default() };
    let t = Instant::now();
    let mut d = Flare.extract(&bytes, &o).unwrap();
    let t_extract = t.elapsed();
    let n = d.units.len();
    assert!(n > 40_000, "{n} units");
    let t = Instant::now();
    assert_eq!(Flare.merge(&d).unwrap(), bytes);
    let t_same = t.elapsed();
    for i in 0..d.units.len() {
        let u = &d.units[i];
        let tgt: Vec<Inline> = u
            .source()
            .iter()
            .map(|x| match x {
                Inline::Text(t) => Inline::text(format!("RU {t}")),
                o => o.clone(),
            })
            .collect();
        let id = u.segments[0].id.clone();
        d.set_target(&id, tgt).unwrap();
    }
    let t = Instant::now();
    let out = Flare.merge(&d).unwrap();
    let t_merge = t.elapsed();
    let again = Flare.extract(&out, &o).unwrap();
    assert_eq!(again.units.len(), n);
    eprintln!(
        "FLARE speed: {} MB of content, {n} units: extract {:.0} ms, untouched merge {:.0} ms, translated merge {:.0} ms",
        text / 1_000_000,
        t_extract.as_secs_f64() * 1000.0,
        t_same.as_secs_f64() * 1000.0,
        t_merge.as_secs_f64() * 1000.0
    );
}

//! `wordfast_bench FILE`: times extract, an untouched merge and a pseudo-translated merge (release build).
use bilingua_model::*;
use bilingua_wordfast::{Txlf, Txml};
use std::time::Instant;

fn main() {
    let path = std::env::args().nth(1).expect("file");
    let bytes = std::fs::read(&path).expect("read");
    let limits = Limits::new().with_max_units(10_000_000).with_max_xml_elements(100_000_000);
    let opts = ExtractOptions::new().with_granularity(Granularity::Paragraph);
    let txlf = path.ends_with(".txlf");
    let t = Instant::now();
    let mut d = if txlf { Txlf { load_targets: false }.extract_with(&bytes, &opts, &limits) } else { Txml { load_targets: false }.extract_with(&bytes, &opts, &limits) }.expect("extract");
    let t_extract = t.elapsed();
    let t = Instant::now();
    let same = if txlf { Txlf::default().merge(&d) } else { Txml::default().merge(&d) }.expect("merge");
    let t_same = t.elapsed();
    assert_eq!(same, bytes);
    for u in d.units.iter_mut() {
        let mut v = vec![Inline::text("T ")];
        v.extend(u.source());
        u.segments[0].target = v;
    }
    let t = Instant::now();
    let out = if txlf { Txlf::default().merge(&d) } else { Txml::default().merge(&d) }.expect("merge");
    let t_merge = t.elapsed();
    if let Some(o) = std::env::args().nth(2) {
        std::fs::write(o, &out).expect("write");
    }
    println!("{{\"bytes\": {}, \"units\": {}, \"extract_s\": {:.3}, \"untouched_merge_s\": {:.3}, \"translated_merge_s\": {:.3}, \"out_bytes\": {}}}", bytes.len(), d.units.len(), t_extract.as_secs_f64(), t_same.as_secs_f64(), t_merge.as_secs_f64(), out.len());
}

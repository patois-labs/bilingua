//! Speed: one generated Contentful document of more than 5 MB, extracted and merged (untouched and fully translated),
//! timed. `scratchpad/speed_locos.py` builds the same document for the Localization OS walker. Run with
//! `cargo test -p bilingua-richtext --release --test speed -- --nocapture` to see the times.

use bilingua_model::*;
use bilingua_richtext::*;
use std::time::Instant;

const PARAS: usize = 28_000;

fn generate() -> String {
    let mut s = String::from("{\"nodeType\": \"document\", \"data\": {}, \"content\": [\n");
    for i in 0..PARAS {
        if i > 0 {
            s.push_str(",\n");
        }
        s.push_str(&format!(
            "{{\"nodeType\": \"paragraph\", \"data\": {{}}, \"content\": [{{\"nodeType\": \"text\", \"value\": \"Paragraph number {i} with \", \"marks\": [], \"data\": {{}}}}, {{\"nodeType\": \"text\", \"value\": \"bold words\", \"marks\": [{{\"type\": \"bold\"}}], \"data\": {{}}}}, {{\"nodeType\": \"text\", \"value\": \" and a \", \"marks\": [], \"data\": {{}}}}, {{\"nodeType\": \"hyperlink\", \"data\": {{\"uri\": \"https://example.com/{i}\"}}, \"content\": [{{\"nodeType\": \"text\", \"value\": \"link\", \"marks\": [], \"data\": {{}}}}]}}, {{\"nodeType\": \"text\", \"value\": \", it is fine.\", \"marks\": [], \"data\": {{}}}}]}}"
        ));
    }
    s.push_str("\n]}\n");
    s
}

#[test]
fn large_file_extract_and_merge_speed() {
    let src = generate();
    assert!(src.len() > 5_000_000, "{} bytes", src.len());
    let f = Contentful;
    let o = ExtractOptions::new().with_source_lang("en").with_target_lang("de").with_granularity(Granularity::Paragraph).with_limits(Limits::new().with_max_units(1_000_000));

    let t0 = Instant::now();
    let mut doc = f.extract(src.as_bytes(), &o).unwrap();
    let t_extract = t0.elapsed();
    assert_eq!(doc.units.len(), PARAS);

    let t1 = Instant::now();
    let same = f.merge(&doc).unwrap();
    let t_untouched = t1.elapsed();
    assert_eq!(same, src.as_bytes());

    let items: Vec<(String, Vec<Inline>)> = doc
        .units
        .iter()
        .map(|u| {
            let mut t = vec![Inline::text("DE ")];
            t.extend(u.source());
            (u.segments[0].id.clone(), t)
        })
        .collect();
    doc.set_targets(items).unwrap();
    let t2 = Instant::now();
    let out = f.merge(&doc).unwrap();
    let t_translated = t2.elapsed();
    assert!(out.len() > src.len());
    println!("SPEED bytes={} units={} extract={:?} merge_untouched={:?} merge_translated={:?}", src.len(), PARAS, t_extract, t_untouched, t_translated);
}

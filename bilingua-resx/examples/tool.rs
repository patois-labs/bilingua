//! Developer tool (not shipped): `dump FILE` prints the units as JSON, `gen PATH MB` writes a large generated
//! file, `time FILE` times extract, untouched merge and a pseudo-translated merge.

use bilingua_model::*;
use bilingua_resx::{Limits, Resx};
use std::time::Instant;

fn shown(v: &[Inline]) -> String {
    v.iter()
        .map(|i| match i {
            Inline::Text(t) => t.clone(),
            Inline::Tag { display, .. } => display.clone(),
        })
        .collect()
}

fn big() -> Limits {
    Limits { max_units: 50_000_000, max_unit_chars: 10_000_000, max_input_bytes: 2 << 30, max_xml_elements: 100_000_000, max_xml_depth: 256 }
}

/// A large resx in the shape Visual Studio writes (made-up Lumen Bakery text).
pub fn generate(min_bytes: usize) -> String {
    let words = ["fresh", "bread", "daily", "order", "pickup", "store", "table", "coffee", "sweet", "morning"];
    let mut s = String::from(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<root>\n  <resheader name=\"resmimetype\">\n    <value>text/microsoft-resx</value>\n  </resheader>\n  <resheader name=\"version\">\n    <value>2.0</value>\n  </resheader>\n",
    );
    let mut i = 0usize;
    while s.len() < min_bytes {
        let w = |k: usize| words[(i * 7 + k * 3) % words.len()];
        match i % 6 {
            0 => s.push_str(&format!("  <data name=\"Menu.Item{i}\" xml:space=\"preserve\">\n    <value>Our {} {} is ready, {{0}} items for {{1:N2}}</value>\n    <comment>note {i}</comment>\n  </data>\n", w(1), w(2))),
            1 => s.push_str(&format!("  <data name=\"Menu.Item{i}\" xml:space=\"preserve\">\n    <value>Caf\u{e9} {} \u{65e5}\u{672c} {} don&apos;t miss &amp; {{0}}</value>\n  </data>\n", w(3), w(4))),
            2 => s.push_str(&format!("  <data name=\"Size{i}\" type=\"System.Drawing.Size, System.Drawing\">\n    <value>{}, {}</value>\n  </data>\n", i % 700, i % 400)),
            3 => s.push_str(&format!("  <data name=\"Menu.Item{i}\" xml:space=\"preserve\">\n    <value>A long text about {}\n    that continues on {} and ends</value>\n  </data>\n", w(1), w(2))),
            4 => s.push_str(&format!("  <data name=\"Menu.Item{i}\">\n    <value><![CDATA[Use <b>{}</b> & {}]]></value>\n  </data>\n", w(8), w(9))),
            _ => s.push_str(&format!("  <data name=\"&gt;&gt;ctl{i}.Name\" xml:space=\"preserve\">\n    <value>ctl{i}</value>\n  </data>\n")),
        }
        i += 1;
    }
    s.push_str("</root>\n");
    s
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let opts = ExtractOptions { granularity: Granularity::Paragraph, ..Default::default() };
    match args.get(1).map(String::as_str) {
        Some("dump") => {
            let bytes = std::fs::read(&args[2]).expect("read");
            match Resx.extract_with(&bytes, &opts, &big()) {
                Ok(d) => {
                    let units: Vec<serde_json::Value> = d
                        .units
                        .iter()
                        .map(|u| {
                            let tags = u.source().iter().filter(|i| matches!(i, Inline::Tag { .. })).count();
                            serde_json::json!({"id": u.id, "text": shown(&u.source()), "tags": tags, "key": u.context.key})
                        })
                        .collect();
                    println!("{}", serde_json::json!({"units": units}));
                }
                Err(e) => println!("{}", serde_json::json!({"error": e.to_string()})),
            }
        }
        Some("gen") => {
            let mb: usize = args[3].parse().expect("MB");
            std::fs::write(&args[2], generate(mb * 1_000_000)).expect("write");
        }
        Some("time") => {
            let bytes = std::fs::read(&args[2]).expect("read");
            let t = Instant::now();
            let mut d = Resx.extract_with(&bytes, &opts, &big()).expect("extract");
            let t_extract = t.elapsed();
            let t = Instant::now();
            let same = Resx.merge(&d).expect("merge") == bytes;
            let t_merge = t.elapsed();
            for u in &mut d.units {
                let mut x = vec![Inline::text("[")];
                x.extend(u.source());
                x.push(Inline::text("]"));
                u.segments[0].target = x;
            }
            let t = Instant::now();
            let out = Resx.merge(&d).expect("merge");
            let t_pseudo = t.elapsed();
            println!(
                "{}",
                serde_json::json!({"bytes": bytes.len(), "units": d.units.len(), "extract_s": t_extract.as_secs_f64(), "merge_untouched_s": t_merge.as_secs_f64(), "identical": same, "merge_pseudo_s": t_pseudo.as_secs_f64(), "out_bytes": out.len()})
            );
        }
        _ => eprintln!("usage: tool dump FILE | gen PATH MB | time FILE"),
    }
}

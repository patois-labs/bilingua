//! Developer tool (not shipped): `dump FILE` prints the units as JSON, `gen PATH MB` writes a large generated
//! file, `time FILE` times extract, untouched merge and a pseudo-translated merge.

use bilingua_model::*;
use bilingua_properties::{Limits, Properties};
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
    { let mut built = Limits::default(); built.max_units = 50_000_000; built.max_value_chars = 10_000_000; built.max_input_bytes = 2 << 30; built }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let opts = ExtractOptions::new().with_granularity(Granularity::Paragraph);
    match args.get(1).map(String::as_str) {
        Some("dump") => {
            let bytes = std::fs::read(&args[2]).expect("read");
            match Properties.extract_with(&bytes, &opts, &big()) {
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
            let mut s = String::from("# Generated Lumen Bakery messages (made up)\n");
            let words = ["fresh", "bread", "daily", "order", "pickup", "store", "table", "coffee", "sweet", "morning"];
            let mut i = 0usize;
            while s.len() < mb * 1_000_000 {
                let w = |k: usize| words[(i * 7 + k * 3) % words.len()];
                match i % 6 {
                    0 => s.push_str(&format!("# note {i}\nmenu.item{i} = Our {} {} is ready, {{0}} items for {{1}}\n", w(1), w(2))),
                    1 => s.push_str(&format!("menu.item{i}: Caf\\u00e9 {} \\u65e5\\u672c {} don''t miss {{0}}\n", w(3), w(4))),
                    2 => s.push_str(&format!("menu.item{i} Total %d {} at %s, 100%% {} {}\n", w(5), w(6), w(7))),
                    3 => s.push_str(&format!("menu.item{i} = A long text about {} \\\n    that continues on {} and ends\n", w(1), w(2))),
                    4 => s.push_str(&format!("menu.item{i}=Tab\\there and new\\nline {}\n", w(8))),
                    _ => s.push_str(&format!("\nmenu.item{i} = {}\n", w(9))),
                }
                i += 1;
            }
            std::fs::write(&args[2], s).expect("write");
        }
        Some("pseudo") => {
            // pseudo FILE OUT: every unit "[source]", merged to OUT
            let bytes = std::fs::read(&args[2]).expect("read");
            let mut d = Properties.extract_with(&bytes, &opts, &big()).expect("extract");
            for u in &mut d.units {
                let mut tg = vec![Inline::text("[")];
                tg.extend(u.source());
                tg.push(Inline::text("]"));
                u.segments[0].target = tg;
            }
            std::fs::write(&args[3], Properties.merge(&d).expect("merge")).expect("write");
        }
        Some("time") => {
            let bytes = std::fs::read(&args[2]).expect("read");
            let t = Instant::now();
            let mut d = Properties.extract_with(&bytes, &opts, &big()).expect("extract");
            let t_extract = t.elapsed();
            let t = Instant::now();
            let same = Properties.merge(&d).expect("merge") == bytes;
            let t_merge = t.elapsed();
            let n = d.units.len();
            for u in &mut d.units {
                let mut tg = vec![Inline::text("[")];
                tg.extend(u.source());
                tg.push(Inline::text("]"));
                u.segments[0].target = tg;
            }
            let t = Instant::now();
            let out = Properties.merge(&d).expect("merge");
            let t_pseudo = t.elapsed();
            println!(
                "{{\"bytes\": {}, \"units\": {}, \"extract_s\": {:.4}, \"merge_untouched_s\": {:.4}, \"merge_translated_s\": {:.4}, \"identical\": {}, \"out_bytes\": {}}}",
                bytes.len(),
                n,
                t_extract.as_secs_f64(),
                t_merge.as_secs_f64(),
                t_pseudo.as_secs_f64(),
                same,
                out.len()
            );
        }
        _ => eprintln!("usage: tool dump FILE | gen PATH MB | time FILE"),
    }
}

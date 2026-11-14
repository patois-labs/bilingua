//! Developer tool (not shipped).
//!
//! * `dump FILE...`     one JSON line per file with the units (text with LocOS tokens, style, note); the filter is
//!   picked by extension (`.dbk`, `.docbook` and `--docbook` = DocBook, everything else DITA)
//! * `gen PATH MB`      writes a large generated DITA topic (same generator as `tests/speed.rs`)
//! * `time FILE`        times extract, untouched merge and a pseudo-translated merge
use bilingua_dita::{Dita, DocBook};
use bilingua_model::profile::tokens::to_locos_text;
use bilingua_model::*;
use std::time::Instant;

#[path = "../tests/gen/mod.rs"]
mod gen;

fn big() -> ExtractOptions {
    ExtractOptions::new()
        .with_limits(Limits::new().with_max_units(50_000_000).with_max_unit_chars(10_000_000).with_max_input_bytes(2 << 30).with_max_xml_elements(100_000_000))
        .with_granularity(Granularity::Paragraph)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("gen") => {
            let mb: usize = args[3].parse().expect("MB");
            std::fs::write(&args[2], gen::topic(mb * 1_000_000)).expect("write");
        }
        Some("time") => {
            let f = Dita::new();
            let bytes = std::fs::read(&args[2]).expect("read");
            let t0 = Instant::now();
            let mut d = f.extract(&bytes, &big()).expect("extract");
            let t1 = Instant::now();
            assert_eq!(f.merge(&d).expect("merge"), bytes);
            let t2 = Instant::now();
            for u in &mut d.units {
                let mut t = vec![Inline::text("[")];
                t.extend(u.source());
                t.push(Inline::text("]"));
                u.segments[0].target = t;
            }
            let out = f.merge(&d).expect("merge");
            let t3 = Instant::now();
            println!(
                "Bilingua dita: {:.1} MB, {} units, extract {:.0} ms, untouched merge {:.0} ms, translated merge {:.0} ms (extract+translated merge {:.0} ms), output {} bytes",
                bytes.len() as f64 / 1e6,
                d.units.len(),
                (t1 - t0).as_secs_f64() * 1e3,
                (t2 - t1).as_secs_f64() * 1e3,
                (t3 - t2).as_secs_f64() * 1e3,
                ((t1 - t0) + (t3 - t2)).as_secs_f64() * 1e3,
                out.len()
            );
        }
        _ => {
            let force_db = args.iter().any(|a| a == "--docbook");
            for path in args.iter().skip(1).filter(|a| *a != "dump" && *a != "--docbook") {
                let bytes = std::fs::read(path).expect("read");
                let ext = path.rsplit('.').next().unwrap_or_default().to_ascii_lowercase();
                let f: Box<dyn Filter> = if force_db || ext == "dbk" || ext == "docbook" { Box::new(DocBook::new()) } else { Box::new(Dita::new()) };
                let line = match f.extract(&bytes, &ExtractOptions::default()) {
                    Ok(d) => {
                        let units: Vec<serde_json::Value> = d
                            .units
                            .iter()
                            .map(|u| serde_json::json!({"text": to_locos_text(&u.source()).0, "style": u.context.style, "note": u.notes.first().map(|n| n.text.clone())}))
                            .collect();
                        serde_json::json!({"file": path, "units": units})
                    }
                    Err(e) => serde_json::json!({"file": path, "error": e.to_string()}),
                };
                println!("{line}");
            }
        }
    }
}

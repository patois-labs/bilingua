//! Developer tool (not shipped).
//!
//! * `dump FILE...`     one JSON line per file with the units (text with LocOS tokens, style, note)
//! * `gen PATH MB`      writes a large generated XML file (same generator as `tests/speed.rs`)
//! * `time FILE`        times extract, untouched merge and a pseudo-translated merge
use bilingua_model::locos::to_locos_text;
use bilingua_model::*;
use bilingua_xml::XmlFilter;
use std::time::Instant;

fn generate(min_bytes: usize) -> String {
    let words = ["fresh", "bread", "daily", "order", "pickup", "store", "table", "coffee", "sweet", "morning"];
    let mut s = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- Generated Lumen Bakery catalog (made up) -->\n<catalog xmlns:its=\"http://www.w3.org/2005/11/its\" its:version=\"2.0\" xml:lang=\"en\">\n  <its:rules version=\"2.0\">\n    <its:translateRule selector=\"//price\" translate=\"no\"/>\n    <its:translateRule selector=\"//item/@label\" translate=\"yes\"/>\n    <its:withinTextRule selector=\"//b | //i | //br\" withinText=\"yes\"/>\n    <its:locNoteRule selector=\"//desc\"><its:locNote>Keep it short.</its:locNote></its:locNoteRule>\n  </its:rules>\n",
    );
    let mut i = 0usize;
    while s.len() < min_bytes {
        let w = |k: usize| words[(i * 7 + k * 3) % words.len()];
        s.push_str(&format!(
            "  <item sku=\"B-{i}\" label=\"Our {} {}\">\n    <name>{} {}</name>\n    <desc>A <b>{}</b> loaf, baked <i>{}</i> every day &amp; sold {{0}} times.<br/>Caf\u{e9} \u{65e5}\u{672c} order before noon, {} please.</desc>\n    <!-- item {i} -->\n    <price currency=\"EUR\">{}.50</price>\n  </item>\n",
            w(1),
            w(2),
            w(3),
            w(4),
            w(5),
            w(6),
            w(7),
            i % 90
        ));
        i += 1;
    }
    s.push_str("</catalog>\n");
    s
}

fn big() -> ExtractOptions {
    ExtractOptions::new().with_limits(Limits::new().with_max_units(50_000_000).with_max_unit_chars(10_000_000).with_max_input_bytes(2 << 30).with_max_xml_elements(100_000_000)).with_granularity(Granularity::Paragraph)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let f = XmlFilter::new();
    match args.get(1).map(String::as_str) {
        Some("gen") => {
            let mb: usize = args[3].parse().expect("MB");
            std::fs::write(&args[2], generate(mb * 1_000_000)).expect("write");
        }
        Some("time") => {
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
                "Bilingua xml: {:.1} MB, {} units, extract {:.0} ms, untouched merge {:.0} ms, translated merge {:.0} ms (extract+merge {:.0} ms), output {} bytes",
                bytes.len() as f64 / 1e6,
                d.units.len(),
                (t1 - t0).as_secs_f64() * 1e3,
                (t2 - t1).as_secs_f64() * 1e3,
                (t3 - t2).as_secs_f64() * 1e3,
                (t3 - t0).as_secs_f64() * 1e3,
                out.len()
            );
        }
        _ => {
            for path in args.iter().skip(1).filter(|a| *a != "dump") {
                let bytes = std::fs::read(path).expect("read");
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

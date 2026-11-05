//! Developer tool (not shipped): `dump FILE` prints the units as JSON, `gen PATH MB` writes a large generated file,
//! `time FILE` times extract, untouched merge and a pseudo-translated merge.

use bilingua_model::profile::tokens::to_locos_text;
use bilingua_model::*;
use bilingua_ttml::{Limits, Ttml};
use std::time::Instant;

fn big() -> Limits {
    Limits::new().with_max_units(50_000_000).with_max_unit_chars(10_000_000).with_max_input_bytes(2 << 30).with_max_lines(100_000_000).with_max_xml_elements(100_000_000)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let f = Ttml;
    let opts = ExtractOptions::new().with_granularity(Granularity::Paragraph);
    match args.get(1).map(String::as_str) {
        Some("dump") => {
            let bytes = std::fs::read(&args[2]).expect("read");
            match f.extract_with(&bytes, &opts, &big()) {
                Ok(d) => {
                    let units: Vec<String> = d.units.iter().map(|u| to_locos_text(&u.source()).0).collect();
                    println!("{}", serde_json::json!({"units": units}));
                }
                Err(e) => println!("{}", serde_json::json!({"error": e.to_string()})),
            }
        }
        Some("gen") => {
            let mb: usize = args[3].parse().expect("MB");
            let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<tt xmlns=\"http://www.w3.org/ns/ttml\" xmlns:tts=\"http://www.w3.org/ns/ttml#styling\" xmlns:ttm=\"http://www.w3.org/ns/ttml#metadata\" xml:lang=\"en\">\n  <head><metadata><ttm:title>Generated Lumen Bakery captions (made up)</ttm:title></metadata><styling><style xml:id=\"s1\" tts:color=\"white\"/></styling></head>\n  <body><div>\n");
            let words = ["fresh", "bread", "daily", "order", "pickup", "store", "table", "coffee", "sweet", "morning"];
            let mut i = 0usize;
            while s.len() < mb * 1_000_000 {
                let w = |k: usize| words[(i * 7 + k * 3) % words.len()];
                let t = i * 3;
                let (h, m, sec) = (t / 3600, t / 60 % 60, t % 60);
                let b = format!("begin=\"{h:02}:{m:02}:{sec:02}.000\" end=\"{h:02}:{m:02}:{:02}.500\" style=\"s1\"", sec);
                match i % 5 {
                    0 => s.push_str(&format!("    <p {b}>Our {} {} is ready,<br/>{{0}} items for {{1}}</p>\n", w(1), w(2))),
                    1 => s.push_str(&format!("    <p {b}>Caf\u{e9} {} \u{65e5}\u{672c} <span tts:fontStyle=\"italic\">{}</span> &amp; more</p>\n", w(3), w(4))),
                    2 => s.push_str(&format!("    <p {b}>\n      A long caption about {}\n      that continues on {}\n    </p>\n", w(1), w(2))),
                    3 => s.push_str(&format!("    <p {b}><span tts:color=\"red\">{}</span><set begin=\"1s\"/> {} <!-- n{i} --></p>\n", w(5), w(6))),
                    _ => s.push_str(&format!("    <p {b}>Total %d {} at %s, 100% {}</p>\n", w(6), w(7))),
                }
                i += 1;
            }
            s.push_str("  </div></body>\n</tt>\n");
            std::fs::write(&args[2], s).expect("write");
        }
        Some("time") => {
            let bytes = std::fs::read(&args[2]).expect("read");
            let t = Instant::now();
            let mut d = f.extract_with(&bytes, &opts, &big()).expect("extract");
            let t_extract = t.elapsed();
            let t = Instant::now();
            let same = f.merge(&d).expect("merge") == bytes;
            let t_merge = t.elapsed();
            let n = d.units.len();
            for u in &mut d.units {
                let mut tg = vec![Inline::text("[")];
                tg.extend(u.source());
                tg.push(Inline::text("]"));
                u.segments[0].target = tg;
            }
            d.target_lang = Some("de".into());
            let t = Instant::now();
            let out = f.merge(&d).expect("merge");
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
        _ => eprintln!("usage: ttml_dump dump FILE | gen PATH MB | time FILE"),
    }
}

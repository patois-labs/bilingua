//! Developer tool (not shipped): `dump [--attrs] FILE` prints the units as JSON, `gen PATH MB` writes a large
//! generated file, `time [--attrs] FILE` times extract, untouched merge and a pseudo-translated merge.

use bilingua_model::*;
use bilingua_xaml::{Limits, Xaml};
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
    Limits::new().with_max_units(50_000_000).with_max_unit_chars(10_000_000).with_max_input_bytes(2 << 30).with_max_lines(100_000_000).with_max_xml_elements(100_000_000)
}

fn main() {
    let mut args: Vec<String> = std::env::args().collect();
    let attrs = args.iter().any(|a| a == "--attrs");
    args.retain(|a| a != "--attrs");
    let f = if attrs { Xaml::with_view_attributes() } else { Xaml::new() };
    let opts = ExtractOptions::new().with_granularity(Granularity::Paragraph);
    match args.get(1).map(String::as_str) {
        Some("dump") => {
            let bytes = std::fs::read(&args[2]).expect("read");
            match f.extract_with(&bytes, &opts, &big()) {
                Ok(d) => {
                    let units: Vec<String> = d.units.iter().map(|u| shown(&u.source())).collect();
                    println!("{}", serde_json::json!({"units": units}));
                }
                Err(e) => println!("{}", serde_json::json!({"error": e.to_string()})),
            }
        }
        Some("gen") => {
            let mb: usize = args[3].parse().expect("MB");
            let mut s = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n<!-- Generated Lumen Bakery strings (made up) -->\r\n<ResourceDictionary xmlns=\"http://schemas.microsoft.com/winfx/2006/xaml/presentation\"\r\n    xmlns:x=\"http://schemas.microsoft.com/winfx/2006/xaml\"\r\n    xmlns:sys=\"clr-namespace:System;assembly=mscorlib\">\r\n");
            let words = ["fresh", "bread", "daily", "order", "pickup", "store", "table", "coffee", "sweet", "morning"];
            let mut i = 0usize;
            while s.len() < mb * 1_000_000 {
                let w = |k: usize| words[(i * 7 + k * 3) % words.len()];
                match i % 6 {
                    0 => s.push_str(&format!("    <sys:String x:Key=\"k{i}\">Our {} {} is ready, {{0}} items for {{1}}</sys:String>\r\n", w(1), w(2))),
                    1 => s.push_str(&format!("    <sys:String x:Key=\"k{i}\">Caf\u{e9} {} \u{65e5}\u{672c} {} don't miss &amp; {{0}}</sys:String>\r\n", w(3), w(4))),
                    2 => s.push_str(&format!("    <x:String x:Key=\"k{i}\">Total %d {} at %s, 100% {}</x:String>\r\n", w(6), w(7))),
                    3 => s.push_str(&format!("    <sys:String x:Key=\"k{i}\">\r\n        A long text about {}\r\n        that continues on {}\r\n    </sys:String>\r\n", w(1), w(2))),
                    4 => s.push_str(&format!("    <sys:String x:Key=\"k{i}\">{{Binding Path={}}}</sys:String>\r\n    <SolidColorBrush x:Key=\"b{i}\" Color=\"#FF0078D7\"/>\r\n", w(8))),
                    _ => s.push_str(&format!("    <!-- c{i} -->\r\n    <sys:String x:Key=\"k{i}\">{}</sys:String>\r\n", w(9))),
                }
                i += 1;
            }
            s.push_str("</ResourceDictionary>\r\n");
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
        _ => eprintln!("usage: tool dump [--attrs] FILE | gen PATH MB | time [--attrs] FILE"),
    }
}

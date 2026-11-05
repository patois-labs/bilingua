//! Developer tool (not shipped): `dump FILE` prints the units as JSON (LocOS token text), `gen PATH MB` writes a
//! large generated stylesheet, `time FILE` times extract, untouched merge and a pseudo-translated merge.

use bilingua_model::*;
use bilingua_xslt::{Limits, Xslt};
use std::time::Instant;

fn big() -> Limits {
    Limits::new().with_max_units(50_000_000).with_max_unit_chars(10_000_000).with_max_input_bytes(2 << 30).with_max_lines(100_000_000).with_max_xml_elements(100_000_000)
}

/// A stylesheet of at least `min_bytes` bytes: literal text with inline markup, value-of points, attributes,
/// `xsl:text`, entity references, comments, expressions and non-ASCII text.
pub fn generate(min_bytes: usize) -> String {
    let words = ["fresh", "bread", "daily", "order", "pickup", "store", "table", "coffee", "sweet", "morning"];
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<!DOCTYPE xsl:stylesheet [\r\n  <!ENTITY nbsp \"&#160;\">\r\n]>\r\n<xsl:stylesheet version=\"1.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\">\r\n  <xsl:output method=\"html\"/>\r\n");
    let mut i = 0usize;
    while s.len() < min_bytes {
        let w = |k: usize| words[(i * 7 + k * 3) % words.len()];
        s.push_str(&format!("  <xsl:template match=\"item{i}\">\r\n    <div class=\"c{i}\" title=\"Box {} {{@id}}\">\r\n", w(0)));
        s.push_str(&format!("      <h2>Our {} <b>{}</b> is ready, <xsl:value-of select=\"name{i}\"/>!</h2>\r\n", w(1), w(2)));
        s.push_str(&format!("      <p>Caf\u{e9} {} \u{65e5}\u{672c} &amp; {{0}}&nbsp;<a href=\"{{@u}}\" title=\"Open {}\">link {}</a> done.<!-- c{i} --></p>\r\n", w(3), w(4), w(5)));
        s.push_str(&format!("      <xsl:if test=\"count(x) &gt; {i}\"><xsl:text>A long text about {}</xsl:text></xsl:if>\r\n", w(1)));
        s.push_str(&format!("      <img src=\"{{@src}}\" alt=\"Photo of {}\"/>\r\n    </div>\r\n  </xsl:template>\r\n", w(2)));
        i += 1;
    }
    s.push_str("</xsl:stylesheet>\r\n");
    s
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let opts = ExtractOptions::new().with_granularity(Granularity::Paragraph);
    match args.get(1).map(String::as_str) {
        Some("dump") => {
            let bytes = std::fs::read(&args[2]).expect("read");
            match Xslt.extract_with(&bytes, &opts, &big()) {
                Ok(d) => {
                    let units: Vec<String> = d.units.iter().map(|u| locos::to_locos_text(&u.source()).0).collect();
                    println!("{}", serde_json::json!({"units": units}));
                }
                Err(e) => println!("{}", serde_json::json!({"error": e.to_string()})),
            }
        }
        Some("gen") => {
            let mb: usize = args[3].parse().expect("MB");
            std::fs::write(&args[2], generate(mb * 1_000_000)).expect("write");
        }
        Some("pseudo") => {
            let bytes = std::fs::read(&args[2]).expect("read");
            let mut d = Xslt.extract_with(&bytes, &opts, &big()).expect("extract");
            d.target_lang = args.get(4).cloned();
            for u in &mut d.units {
                let mut tg = vec![Inline::text("[")];
                tg.extend(u.source());
                tg.push(Inline::text("]"));
                u.segments[0].target = tg;
            }
            std::fs::write(&args[3], Xslt.merge(&d).expect("merge")).expect("write");
        }
        Some("time") => {
            let bytes = std::fs::read(&args[2]).expect("read");
            let t = Instant::now();
            let mut d = Xslt.extract_with(&bytes, &opts, &big()).expect("extract");
            let t_extract = t.elapsed();
            let t = Instant::now();
            let same = Xslt.merge(&d).expect("merge") == bytes;
            let t_merge = t.elapsed();
            for u in &mut d.units {
                let mut tg = vec![Inline::text("[")];
                tg.extend(u.source());
                tg.push(Inline::text("]"));
                u.segments[0].target = tg;
            }
            let t = Instant::now();
            let out = Xslt.merge(&d).expect("pseudo merge");
            let t_pseudo = t.elapsed();
            println!(
                "{{\"bytes\": {}, \"units\": {}, \"extract_ms\": {}, \"merge_ms\": {}, \"pseudo_merge_ms\": {}, \"identical\": {}, \"out_bytes\": {}}}",
                bytes.len(),
                d.units.len(),
                t_extract.as_millis(),
                t_merge.as_millis(),
                t_pseudo.as_millis(),
                same,
                out.len()
            );
        }
        _ => eprintln!("usage: xslt_tool dump FILE | gen PATH MB | time FILE | pseudo FILE OUT [LANG]"),
    }
}

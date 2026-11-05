//! Developer tool (not shipped): `ttx_dump dump [--locos] [--locked] FILE` prints the units as JSON (LocOS tokens),
//! `ttx_dump gen PATH MB` writes a large generated UTF-16 file, `ttx_dump time [--locos] FILE` times extract,
//! untouched merge and a pseudo-translated merge.

use bilingua_model::profile::tokens::to_locos_text;
use bilingua_model::*;
use bilingua_ttx::{Limits, Ttx};
use std::time::Instant;

fn big() -> Limits {
    Limits::new().with_max_units(50_000_000).with_max_unit_chars(10_000_000).with_max_input_bytes(2 << 30).with_max_lines(100_000_000).with_max_xml_elements(100_000_000)
}

fn main() {
    let mut args: Vec<String> = std::env::args().collect();
    let locos = args.iter().any(|a| a == "--locos");
    let locked = args.iter().any(|a| a == "--locked");
    args.retain(|a| !a.starts_with("--"));
    let f = Ttx { skip_translated: locos };
    let opts = ExtractOptions::new().with_granularity(Granularity::Paragraph).with_include_locked(locked);
    match args.get(1).map(String::as_str) {
        Some("dump") => {
            let bytes = std::fs::read(&args[2]).expect("read");
            match f.extract_with(&bytes, &opts, &big()) {
                Ok(d) => {
                    let units: Vec<serde_json::Value> = d
                        .units
                        .iter()
                        .map(|u| {
                            let s = &u.segments[0];
                            serde_json::json!({
                                "id": u.id, "key": u.context.key, "locked": !u.translate,
                                "text": to_locos_text(&s.source).0, "target": to_locos_text(&s.target).0, "state": s.state,
                            })
                        })
                        .collect();
                    println!("{}", serde_json::json!({"units": units, "warnings": d.warnings.iter().map(|w| w.message.clone()).collect::<Vec<_>>()}));
                }
                Err(e) => println!("{}", serde_json::json!({"error": e.to_string()})),
            }
        }
        Some("gen") => {
            let mb: usize = args[3].parse().expect("MB");
            let mut s = String::from("<?xml version='1.0'?>\r\n<TRADOStag Version=\"2.0\"><FrontMatter>\r\n<ToolSettings CreationDate=\"20071015T101500Z\" CreationTool=\"TRADOS TagEditor\" CreationToolVersion=\"7.0.0.615\"></ToolSettings>\r\n<UserSettings DataType=\"HTML\" O-Encoding=\"UTF-8\" SettingsName=\"HTML\" SourceLanguage=\"EN-US\" TargetLanguage=\"DE-DE\" SourceDocumentPath=\"big.htm\"></UserSettings>\r\n</FrontMatter><Body><Raw>\r\n");
            let words = ["fresh", "bread", "daily", "order", "pickup", "store", "table", "coffee", "sweet", "morning"];
            let mut i = 0usize;
            // UTF-16 doubles the size on disk, so the character count is half of the file size
            while s.len() < mb * 500_000 {
                let w = |k: usize| words[(i * 7 + k * 3) % words.len()];
                s.push_str("<ut Type=\"start\" Style=\"external\" RightEdge=\"angle\" DisplayText=\"p\">&lt;p&gt;</ut>");
                match i % 4 {
                    0 => s.push_str(&format!("<Tu MatchPercent=\"0\"><Tuv Lang=\"EN-US\">Our {} {} is ready, <ut Type=\"start\" RightEdge=\"angle\" DisplayText=\"b\">&lt;b&gt;</ut>{}<ut Type=\"end\" LeftEdge=\"angle\" DisplayText=\"b\">&lt;/b&gt;</ut> items.</Tuv><Tuv Lang=\"DE-DE\"></Tuv></Tu>", w(1), w(2), w(3))),
                    1 => s.push_str(&format!("<Tu MatchPercent=\"0\"><Tuv Lang=\"EN-US\">Caf\u{e9} {} \u{65e5}\u{672c} {} &amp; more<ut DisplayText=\"br\">&lt;br&gt;</ut><df Bold=\"on\">{}</df> </Tuv></Tu>", w(3), w(4), w(5))),
                    2 => s.push_str(&format!("<Tu MatchPercent=\"100\" Origin=\"manual\"><Tuv Lang=\"EN-US\">Already done {}.</Tuv><Tuv Lang=\"DE-DE\">Schon erledigt {}.</Tuv></Tu>", w(6), w(7))),
                    _ => s.push_str(&format!("<Tu MatchPercent=\"0\"><Tuv Lang=\"EN-US\">A long text about {}\r\n that continues on {}</Tuv><Tuv Lang=\"DE-DE\"/></Tu>", w(1), w(2))),
                }
                s.push_str("<ut Type=\"end\" Style=\"external\" LeftEdge=\"angle\" DisplayText=\"p\">&lt;/p&gt;</ut>\r\n");
                i += 1;
            }
            s.push_str("</Raw></Body></TRADOStag>\r\n");
            let mut v = vec![0xFF, 0xFE];
            for u in s.encode_utf16() {
                v.extend_from_slice(&u.to_le_bytes());
            }
            std::fs::write(&args[2], v).expect("write");
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
                if !u.translate {
                    continue;
                }
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
        _ => eprintln!("usage: ttx_dump dump [--locos] [--locked] FILE | gen PATH MB | time [--locos] FILE"),
    }
}

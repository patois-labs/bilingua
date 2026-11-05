//! `dump FILE...`: prints {file: {"units": [text, ...]}} with placeholders shown in place (for the LocOS comparison),
//! plus extract+merge timing with `--time N FILE`. Not shipped.
use bilingua_model::*;
use bilingua_strings::Strings;
use std::time::Instant;

fn shown(v: &[Inline]) -> String {
    v.iter().map(|i| match i { Inline::Text(t) => t.clone(), Inline::Tag { display, .. } => display.clone() }).collect()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--time") {
        let n: usize = args[1].parse().unwrap();
        let bytes = std::fs::read(&args[2]).unwrap();
        let t = Instant::now();
        let mut units = 0;
        for _ in 0..n {
            let d = Strings.extract(&bytes, &ExtractOptions::default()).unwrap();
            units = d.units.len();
        }
        let ex = t.elapsed().as_secs_f64() / n as f64;
        let d = Strings.extract(&bytes, &ExtractOptions::default()).unwrap();
        let t = Instant::now();
        for _ in 0..n {
            let out = Strings.merge(&d).unwrap();
            assert_eq!(out, bytes);
        }
        let mg = t.elapsed().as_secs_f64() / n as f64;
        println!("{{\"units\": {units}, \"extract_s\": {ex:.6}, \"merge_s\": {mg:.6}, \"bytes\": {}}}", bytes.len());
        return;
    }
    if args.first().map(String::as_str) == Some("--pseudo") {
        // pseudo-translate every unit as "[source]" and write the merged file
        let bytes = std::fs::read(&args[1]).unwrap();
        let mut d = Strings.extract(&bytes, &ExtractOptions::default()).unwrap();
        for u in d.units.clone() {
            let mut t = vec![Inline::text("[")];
            t.extend(u.source());
            t.push(Inline::text("]"));
            d.set_target(&u.segments[0].id, t).unwrap();
        }
        std::fs::write(&args[2], Strings.merge(&d).unwrap()).unwrap();
        return;
    }
    let mut out = serde_json::Map::new();
    for f in args {
        let bytes = std::fs::read(&f).unwrap();
        let v = match Strings.extract(&bytes, &ExtractOptions::default()) {
            Ok(d) => serde_json::json!({"units": d.units.iter().map(|u| shown(&u.source())).collect::<Vec<_>>()}),
            Err(e) => serde_json::json!({"error": e.to_string()}),
        };
        out.insert(f, v);
    }
    println!("{}", serde_json::Value::Object(out));
}

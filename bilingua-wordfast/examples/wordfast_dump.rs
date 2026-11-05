//! `wordfast_dump FILE [--targets] [--locked]`: one JSON line per unit (LocOS-token text, target), for the oracles.
use bilingua_model::*;
use bilingua_wordfast::{Txlf, Txml};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = &args[0];
    let bytes = std::fs::read(path).expect("read");
    let opts = ExtractOptions::new().with_include_locked(args.iter().any(|a| a == "--locked")).with_granularity(Granularity::Paragraph);
    let targets = args.iter().any(|a| a == "--targets");
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let doc = if ext == "txlf" { Txlf { load_targets: targets }.extract(&bytes, &opts) } else { Txml { load_targets: targets }.extract(&bytes, &opts) };
    match doc {
        Err(e) => println!("{}", serde_json::json!({ "error": e.to_string() })),
        Ok(d) => {
            for u in &d.units {
                let (text, _) = profile::tokens::to_locos_text(&u.source());
                let tgt = if u.segments.iter().any(|s| !s.target.is_empty()) { Some(profile::tokens::to_locos_text(&u.output()).0) } else { None };
                println!("{}", serde_json::json!({ "id": u.id, "text": text, "target": tgt, "translate": u.translate, "key": u.context.key, "state": format!("{:?}", u.segments[0].state) }));
            }
        }
    }
}

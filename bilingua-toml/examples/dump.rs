//! `cargo run -p bilingua-toml --example dump -- OUT.json FILE...` writes {file: {"units": [LocOS-token text]}}
//! for the oracle comparison (`tools/compare.py`).
use bilingua_model::profile::tokens::to_locos_text;
use bilingua_model::{ExtractOptions, Filter};
use bilingua_toml::TomlFilter;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut map = serde_json::Map::new();
    for f in &args[1..] {
        let bytes = std::fs::read(f).unwrap();
        let v = match TomlFilter.extract(&bytes, &ExtractOptions::default()) {
            Ok(d) => {
                let u: Vec<String> = d.units.iter().map(|u| to_locos_text(&u.source()).0).collect();
                // the same file translated: every unit becomes "DE " + its source, tags kept
                let mut t = d.clone();
                for unit in t.units.iter_mut().filter(|u| u.translate) {
                    let mut v = vec![bilingua_model::Inline::text("DE ")];
                    v.extend(unit.source());
                    unit.segments[0].target = bilingua_model::merge_text(v);
                }
                let merged = TomlFilter.merge(&t).map(|b| String::from_utf8_lossy(&b).to_string()).unwrap_or_default();
                serde_json::json!({ "units": u, "merged": merged })
            }
            Err(e) => serde_json::json!({ "error": e.to_string() }),
        };
        map.insert(f.clone(), v);
    }
    std::fs::write(&args[0], serde_json::to_string(&map).unwrap()).unwrap();
}

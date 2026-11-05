//! `cargo run -p bilingua-toml --example dump -- OUT.json FILE...` writes {file: {"units": [LocOS-token text]}}
//! for the oracle comparison (`oracle/corpus/toml/compare.py`).
use bilingua_model::locos::to_locos_text;
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
                serde_json::json!({ "units": u })
            }
            Err(e) => serde_json::json!({ "error": e.to_string() }),
        };
        map.insert(f.clone(), v);
    }
    std::fs::write(&args[0], serde_json::to_string(&map).unwrap()).unwrap();
}

//! `email_dump OUT.json FILE...` writes {file: {"units": [text, ...]}} (or {"error": ...}) in the shape of
//! `oracle/locos_dump.py`, with the units as LocOS shows them; `email_dump FILE` prints one message's units.

use bilingua_email::Email;
use bilingua_model::*;
use std::collections::BTreeMap;

fn units(path: &str) -> Result<Vec<String>, String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    let d = Email.extract(&data, &ExtractOptions::default()).map_err(|e| e.to_string())?;
    Ok(d.units.iter().map(|u| bilingua_model::profile::tokens::to_locos_text(&u.source()).0).collect())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 1 {
        match units(&args[0]) {
            Ok(u) => u.iter().for_each(|t| println!("{t}")),
            Err(e) => println!("ERROR: {e}"),
        }
        return;
    }
    let mut out: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    for f in &args[1..] {
        let name = std::path::Path::new(f).file_name().unwrap().to_string_lossy().to_string();
        out.insert(
            name,
            match units(f) {
                Ok(u) => serde_json::json!({ "units": u }),
                Err(e) => serde_json::json!({ "error": e }),
            },
        );
    }
    std::fs::write(&args[0], serde_json::to_string(&out).unwrap()).unwrap();
}

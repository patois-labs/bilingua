//! Prints the units of MIF files as JSON lines: `mif_dump FILE...` -> {"file", "style", "text"} with every inline
//! code replaced by U+00A4 (the marker the Okapi oracle uses). Used by tests/tools/okapi_compare.py.
use bilingua_mif::Mif;
use bilingua_model::*;

fn main() {
    for path in std::env::args().skip(1) {
        let data = std::fs::read(&path).expect("read");
        match Mif.extract(&data, &ExtractOptions::default()) {
            Ok(doc) => {
                for u in &doc.units {
                    let text: String = u
                        .source()
                        .iter()
                        .map(|i| match i {
                            Inline::Text(t) => t.clone(),
                            Inline::Tag { .. } => "\u{a4}".to_string(),
                        })
                        .collect();
                    println!(
                        "{}",
                        serde_json::json!({"file": path, "style": u.context.style, "text": text})
                    );
                }
            }
            Err(e) => println!("{}", serde_json::json!({"file": path, "error": e.to_string()})),
        }
    }
}

//! `cargo run -p bilingua-json5 --example dump -- FILE...`: one JSON line per file with the units (key, text, notes,
//! translatable) and whether an untouched merge is byte-identical, for comparing with the Localization OS filter.

use bilingua_json5::Json5Filter;
use bilingua_model::*;
use serde_json::json;

fn main() {
    let f = Json5Filter::default();
    let o = ExtractOptions { include_locked: true, ..Default::default() };
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).unwrap_or_default();
        let v = match f.extract(&bytes, &o) {
            Err(e) => json!({"file": path, "error": e.to_string()}),
            Ok(d) => {
                let units: Vec<_> = d
                    .units
                    .iter()
                    .map(|u| {
                        let text: String = u
                            .source()
                            .iter()
                            .map(|i| match i {
                                Inline::Text(t) => t.clone(),
                                Inline::Tag { display, .. } => display.clone(),
                            })
                            .collect();
                        json!([u.context.key.clone().unwrap_or_default(), text, u.context.notes.join("\n"), u.translate])
                    })
                    .collect();
                let same = f.merge(&d).map(|b| b == bytes).unwrap_or(false);
                // The fixed pseudo-translation the differential test applies on both sides.
                let mut t = d.clone();
                let mut n = 0;
                for u in &mut t.units {
                    if u.translate {
                        u.segments[0].target = vec![Inline::text(format!("T{n} it's \"q\" \\ \n \u{e9}\u{2028} {{x}}"))];
                        n += 1;
                    }
                }
                let translated = f.merge(&t).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
                json!({"file": path, "same": same, "units": units, "translated": translated})
            }
        };
        println!("{v}");
    }
}

//! `cargo run -p bilingua-qtts --example qtts_dump -- FILE [FILE ...]` prints the units of each file as JSON lines
//! (`{"file", "units": [{"i", "text"}]}`), the text in the Localization OS token form, for the differential test.
use bilingua_model::*;
use bilingua_qtts::QtTs;

fn main() {
    for f in std::env::args().skip(1) {
        let bytes = std::fs::read(&f).unwrap();
        let opts = ExtractOptions { source_lang: Some("en".into()), ..Default::default() };
        match QtTs.extract(&bytes, &opts) {
            Ok(d) => {
                let units: Vec<serde_json::Value> = d
                    .units
                    .iter()
                    .map(|u| {
                        let (t, _) = locos::to_locos_text(&u.source());
                        serde_json::json!({"i": u.context.extra.get("p_index").and_then(|p| p.parse::<usize>().ok()), "text": t})
                    })
                    .collect();
                println!("{}", serde_json::json!({"file": f, "units": units}));
            }
            Err(e) => println!("{}", serde_json::json!({"file": f, "error": e.to_string()})),
        }
    }
}

//! `cargo run -p bilingua-odf --example odf_dump -- FILE [FILE ...]` prints the units of each file as JSON lines
//! (`{"file", "units": [{"i", "text"}]}`), the text in the Localization OS token form, for the differential tests.
//! `ODF_COMMENTS=1` also lists the comment paragraphs.
use bilingua_model::*;
use bilingua_odf::*;

fn main() {
    let comments = std::env::var("ODF_COMMENTS").is_ok();
    for f in std::env::args().skip(1) {
        let bytes = std::fs::read(&f).unwrap();
        let ext = f.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
        let o = OdfOptions { translate_comments: comments };
        let filter: Box<dyn Filter> = match ext.as_str() {
            "ods" | "ots" => Box::new(Ods::with_options(o)),
            "odp" | "otp" => Box::new(Odp::with_options(o)),
            "odg" | "otg" => Box::new(Odg::with_options(o)),
            _ => Box::new(Odt::with_options(o)),
        };
        let opts = ExtractOptions::new().with_source_lang("en");
        match filter.extract(&bytes, &opts) {
            Ok(d) => {
                let units: Vec<serde_json::Value> = d
                    .units
                    .iter()
                    .map(|u| {
                        let (t, _) = profile::tokens::to_locos_text(&u.source());
                        serde_json::json!({"i": u.context.extra.get("p_index").and_then(|p| p.parse::<usize>().ok()), "text": t})
                    })
                    .collect();
                println!("{}", serde_json::json!({"file": f, "units": units}));
            }
            Err(e) => println!("{}", serde_json::json!({"file": f, "error": e.to_string()})),
        }
    }
}

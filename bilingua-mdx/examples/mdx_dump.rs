//! Prints the units of an MDX file as JSON, the way LocOS shows them (one token per inline code):
//!   cargo run -p bilingua-mdx --example mdx_dump -- FILE [MERGED_OUT]
//! With MERGED_OUT, every unit is pseudo translated and the merged file written there.
use bilingua_mdx::Mdx;
use bilingua_model::profile::tokens::to_locos_text;
use bilingua_model::*;

fn main() {
    let mut args = std::env::args().skip(1);
    let file = args.next().expect("usage: mdx_dump FILE [MERGED_OUT]");
    let bytes = std::fs::read(&file).expect("read");
    match Mdx.extract(&bytes, &ExtractOptions::default()) {
        Err(e) => println!("{{\"error\":{:?}}}", e.to_string()),
        Ok(mut doc) => {
            let units: Vec<String> = doc.units.iter().map(|u| to_locos_text(&u.source()).0).collect();
            let untouched = Mdx.merge(&doc).map(|b| b == bytes).unwrap_or(false);
            println!("{}", serde_json::json!({ "units": units, "untouched_merge_ok": untouched }));
            if let Some(out) = args.next() {
                for u in &mut doc.units {
                    let mut t = vec![Inline::text("\u{ab}")];
                    t.extend(u.source());
                    u.segments[0].target = t;
                }
                std::fs::write(out, Mdx.merge(&doc).expect("merge")).expect("write");
            }
        }
    }
}

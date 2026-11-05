//! `vsdx_dump FILE [--comments]`: prints the units of a Visio file as JSON, the text the way LocOS tokens write it.
use bilingua_model::*;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let bytes = std::fs::read(&args[0]).expect("read file");
    let opts = ExtractOptions::default();
    let doc = if args.iter().any(|a| a == "--comments") {
        bilingua_vsdx::Vsdx.extract_with_comments(&bytes, &opts)
    } else {
        bilingua_vsdx::Vsdx.extract(&bytes, &opts)
    };
    if let Some(k) = args.iter().position(|a| a == "--merge") {
        // pseudo translate every unit ("ZZ " in front) and write the merged file: --merge OUT [--lang CODE]
        let mut d = doc.expect("extract");
        if let Some(l) = args.iter().position(|a| a == "--lang") {
            d.target_lang = Some(args[l + 1].clone());
        }
        for u in &mut d.units {
            let mut t = vec![Inline::text("ZZ ")];
            t.extend(u.source());
            u.segments[0].target = merge_text(t);
        }
        std::fs::write(&args[k + 1], bilingua_vsdx::Vsdx.merge(&d).expect("merge")).unwrap();
        return;
    }
    match doc {
        Ok(d) => {
            let units: Vec<_> = d
                .units
                .iter()
                .map(|u| serde_json::json!({"id": u.id, "text": locos::to_locos_text(&u.source()).0, "style": u.context.style}))
                .collect();
            println!("{}", serde_json::json!({"units": units}));
        }
        Err(e) => println!("{}", serde_json::json!({"error": e.to_string()})),
    }
}

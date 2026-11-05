//! `htmlhelp_dump FILE [TARGET] [OUT]`: prints the units of an HTML Help file (hhc, hhk or hhp by extension), and with a
//! target language writes the file back with every unit translated as "[TARGET] text" to OUT (default: stdout).
use bilingua_htmlhelp::*;
use bilingua_model::*;
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = args.first().expect("usage: htmlhelp_dump FILE [TARGET] [OUT]");
    let bytes = std::fs::read(path).expect("read");
    let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
    let f: Box<dyn Filter> = match ext.as_str() {
        "hhk" => Box::new(HhkFilter),
        "hhp" => Box::new(HhpFilter),
        _ => Box::new(HhcFilter),
    };
    let mut doc = f.extract(&bytes, &ExtractOptions::default()).unwrap_or_else(|e| panic!("{e}"));
    for u in &doc.units {
        println!("{}\t{}", u.id, plain_text(&u.source()));
    }
    if let Some(t) = args.get(1) {
        doc.target_lang = Some(t.clone());
        for u in &mut doc.units {
            let text = format!("[{t}] {}", plain_text(&u.source()));
            u.segments[0].target = vec![Inline::text(text)];
        }
        let out = f.merge(&doc).unwrap_or_else(|e| panic!("{e}"));
        match args.get(2) {
            Some(p) => std::fs::write(p, out).expect("write"),
            None => std::io::stdout().write_all(&out).expect("write"),
        }
    }
}

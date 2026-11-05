//! `adoc_dump FILE [LANG|-] [FILL]`: prints the units of an AsciiDoc file (LocOS token text), then the file merged with
//! every unit prefixed "FR " (and the target language, when given). A debugging aid.
use bilingua_adoc::Adoc;
use bilingua_model::*;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("usage: adoc_dump FILE [LANG|-] [FILL]");
    let lang = args.next().filter(|l| l != "-");
    let fill = args.next();
    let data = std::fs::read(&path).expect("read");
    let mut doc = Adoc.extract(&data, &ExtractOptions::default()).expect("extract");
    for (i, u) in doc.units.iter().enumerate() {
        println!("{i}\t{}\t{:?}", locos::to_locos_text(&u.source()).0.replace('\n', "\\n"), u.context.style);
    }
    doc.target_lang = lang;
    let ids: Vec<(String, Vec<Inline>)> = doc.units.iter().filter(|u| u.translate).map(|u| (u.segments[0].id.clone(), u.source())).collect();
    for (id, src) in ids {
        let t: Vec<Inline> = match &fill {
            Some(f) => src.into_iter().map(|i| if matches!(i, Inline::Text(_)) { Inline::text(f.clone()) } else { i }).collect(),
            None => {
                let mut t = vec![Inline::text("FR ")];
                t.extend(src);
                t
            }
        };
        doc.set_target(&id, t).unwrap();
    }
    let out = Adoc.merge(&doc).expect("merge");
    println!("-----\n{}", String::from_utf8_lossy(&out));
}

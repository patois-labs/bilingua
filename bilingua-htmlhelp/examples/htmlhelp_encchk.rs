//! Gate scratch: for a charset label, merge a one-entry sitemap that declares it with the translation `a` + C for every
//! character C from U+0080 to U+FFFF (and a sample above), and print `CODEPOINT HEX-OF-VALUE` or `CODEPOINT ERR`.
use bilingua_htmlhelp::HhcFilter;
use bilingua_model::*;
use std::io::Write;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let label = &a[1];
    let src = format!("<meta charset={label}><OBJECT type=\"text/sitemap\"><param name=\"Name\" value=\"a\"></OBJECT>");
    let doc0 = HhcFilter.extract(src.as_bytes(), &ExtractOptions::default()).unwrap();
    let id = doc0.units[0].segments[0].id.clone();
    let mut out = std::io::BufWriter::new(std::fs::File::create(&a[2]).unwrap());
    let extra = [0x10000u32, 0x1F600, 0x20000, 0x2A6D6, 0x10FFFD];
    for u in (0x80u32..=0xFFFF).chain(extra) {
        let Some(c) = char::from_u32(u) else { continue };
        let mut doc = doc0.clone();
        doc.set_target(&id, vec![Inline::text(format!("a{c}"))]).unwrap();
        match HhcFilter.merge(&doc) {
            Ok(b) => {
                let start = b.windows(7).position(|w| w == b"value=\"").unwrap() + 7;
                let end = b.windows(11).rposition(|w| w == b"\"></OBJECT>").unwrap();
                let hex: String = b[start..end].iter().map(|x| format!("{x:02x}")).collect();
                writeln!(out, "{u:x} {hex}").unwrap();
            }
            Err(_) => writeln!(out, "{u:x} ERR").unwrap(),
        }
    }
}

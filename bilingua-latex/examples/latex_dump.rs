//! `latex_dump FILE`: prints the units of a LaTeX file, one per line, with codes as LocOS tokens.
use bilingua_latex::Latex;
use bilingua_model::*;

fn main() {
    let path = std::env::args().nth(1).expect("usage: latex_dump FILE");
    let data = std::fs::read(&path).expect("read");
    match Latex::default().extract(&data, &ExtractOptions::default()) {
        Ok(doc) => {
            for u in &doc.units {
                println!("{}\t{}", u.id, locos::to_locos_text(&u.source()).0.replace('\n', "\\n"));
            }
        }
        Err(e) => println!("ERROR {e}"),
    }
}

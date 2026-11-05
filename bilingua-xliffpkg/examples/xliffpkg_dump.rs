//! `cargo run -p bilingua-xliffpkg --example xliffpkg_dump -- FILE`: prints one JSON line per segment
//! (id, LocOS-style text, state, locked). Used to compare with the LocOS filter.
use bilingua_model::{profile::tokens::to_locos_text, ExtractOptions};

fn main() {
    let path = std::env::args().nth(1).expect("file");
    let bytes = std::fs::read(&path).expect("read");
    let f = bilingua_xliffpkg::filters().remove(0);
    let doc = f.extract(&bytes, &ExtractOptions::default());
    match doc {
        Err(e) => println!("{{\"error\":{:?}}}", e.to_string()),
        Ok(d) => {
            for u in &d.units {
                for s in &u.segments {
                    let (t, _) = to_locos_text(&s.source);
                    let (tt, _) = to_locos_text(&s.target);
                    println!("{{\"id\":{:?},\"text\":{:?},\"target\":{:?},\"state\":\"{:?}\",\"locked\":{}}}", s.id, t, tt, s.state, !u.translate);
                }
            }
        }
    }
}

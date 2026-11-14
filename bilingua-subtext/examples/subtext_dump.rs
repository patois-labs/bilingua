use bilingua_model::*;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = std::fs::read(&a[2]).unwrap();
    let f: &dyn Filter = if a[1] == "sbv" { &bilingua_subtext::Sbv } else { &bilingua_subtext::Sub };
    let doc = f.extract(&data, &ExtractOptions::default()).unwrap();
    if let Skeleton::Text { encoding, .. } = &doc.skeleton {
        println!("encoding {encoding}");
    }
    for u in &doc.units {
        println!("{} {:?}", u.id, plain_text(&u.source()));
    }
}

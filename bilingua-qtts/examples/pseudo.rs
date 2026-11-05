//! `cargo run -p bilingua-qtts --example pseudo -- FILE OUT [TARGET_LANG]` translates every unit to `T:` plus its
//! source (tags kept) and writes the merged file, for the differential test against the LocOS filter.
use bilingua_model::*;
use bilingua_qtts::QtTs;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let bytes = std::fs::read(&a[0]).unwrap();
    let opts = ExtractOptions { source_lang: Some("en".into()), target_lang: a.get(2).cloned(), ..Default::default() };
    let mut d = QtTs.extract(&bytes, &opts).unwrap();
    for u in d.units.iter_mut().filter(|u| u.translate) {
        let mut t = vec![Inline::text("T:")];
        t.extend(u.source());
        u.segments[0].target = merge_text(t);
    }
    std::fs::write(&a[1], QtTs.merge(&d).unwrap()).unwrap();
}

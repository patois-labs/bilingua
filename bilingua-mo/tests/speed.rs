//! One large generated catalog (over 5 MB of text), extracted and merged. Run `cargo test --release -p bilingua-mo
//! --test speed -- --nocapture` for the timings. Set `MO_SPEED_OUT` to a path to also save the file, so the LocOS
//! filter can be timed on the same bytes (`tools/speed_locos.py`).

mod common;
use bilingua_model::*;
use bilingua_mo::Mo;
use common::*;
use std::time::Instant;

pub fn generate() -> Vec<u8> {
    let words = ["harbor", "lantern", "bread", "window", "morning", "garden", "ticket", "river", "paper", "market"];
    let mut p: Vec<(Vec<u8>, Vec<u8>)> = vec![(Vec::new(), HEADER_FR.as_bytes().to_vec())];
    for n in 0..34_000usize {
        let w = |k: usize| words[(n * 7 + k) % words.len()];
        let id = format!("Open the {} {} near the {} at {{0}} and bring {} for {} \"{}\" sentence {n} of the guide, in full.", w(1), w(2), w(3), w(4), w(5), w(6));
        let tr = format!("Ouvrez le {} {} pr\u{e8}s du {} \u{e0} {{0}} et apportez {} pour {} \u{ab}{}\u{bb} phrase {n} du guide, en entier.", w(1), w(2), w(3), w(4), w(5), w(6));
        let (id, tr) = if n % 6 == 0 { (format!("{id}\0{id} (many) %d"), format!("{tr}\0{tr} (plusieurs) %d")) } else { (id, tr) };
        let key = if n % 11 == 0 { format!("menu\u{4}{id}") } else { id };
        p.push((key.into_bytes(), tr.into_bytes()));
    }
    p.sort();
    pack_with(&p, &Pack { hash_size: 0, ..Default::default() })
}

#[test]
fn large_file_round_trips_and_is_timed() {
    let bytes = generate();
    assert!(bytes.len() >= 5_000_000, "only {} bytes", bytes.len());
    if let Ok(path) = std::env::var("MO_SPEED_OUT") {
        std::fs::write(path, &bytes).unwrap();
    }
    let t0 = Instant::now();
    let mut d = Mo.extract(&bytes, &opts(Some("fr"))).unwrap();
    let t_extract = t0.elapsed();
    assert_eq!(d.units.len(), 34_000 + 34_000 / 6 + 1);
    let t1 = Instant::now();
    let same = Mo.merge(&d).unwrap();
    let t_merge = t1.elapsed();
    assert_eq!(same, bytes);
    // Translate everything and merge again.
    pseudo(&mut d);
    let t2 = Instant::now();
    let out = Mo.merge(&d).unwrap();
    let t_merge_tr = t2.elapsed();
    let back = Mo.extract(&out, &opts(None)).unwrap();
    assert_eq!(back.units.len(), d.units.len());
    println!(
        "mo speed: {} bytes, {} units; extract {:?}, untouched merge {:?}, translated merge {:?}; extract+merge {:?}",
        bytes.len(),
        d.units.len(),
        t_extract,
        t_merge,
        t_merge_tr,
        t_extract + t_merge_tr
    );
}

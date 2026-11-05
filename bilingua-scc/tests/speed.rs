//! Speed: one generated file of at least 5 MB, extracted and merged with every caption translated.
//! Run in release for the real number:
//!   cargo test -p bilingua-scc --release --test speed -- --nocapture
//! Set BILINGUA_SCC_SPEED_OUT to a path to keep the generated file (tools/speed_locos.py reads it).
mod common;
use bilingua_model::*;
use bilingua_scc::SccFilter;
use std::time::Instant;

fn pair(a: u8, b: u8) -> String {
    let p = |x: u8| if x.count_ones() & 1 == 0 { x | 0x80 } else { x };
    format!("{:02x}{:02x}", p(a), p(b))
}

fn text_words(s: &str) -> Vec<String> {
    let b = s.as_bytes();
    b.chunks(2).map(|c| pair(c[0], *c.get(1).unwrap_or(&0))).collect()
}

pub fn generate() -> String {
    let words = ["Hello", "world", "caption", "Bonjour", "again", "night", "the", "quick", "brown", "fox", "jumps", "over", "lazy", "dog"];
    let mut s = String::with_capacity(6_500_000);
    s.push_str("Scenarist_SCC V1.0\r\n\r\n");
    let mut i = 0usize;
    while s.len() < 5_300_000 {
        let tc = format!("{:02}:{:02}:{:02}:{:02}", (i / 108000) % 24, (i / 1800) % 60, (i / 30) % 60, i % 30);
        let row = |k: usize| -> String {
            let mut t = String::new();
            for j in 0..4 {
                if j > 0 {
                    t.push(' ');
                }
                t.push_str(words[(i * 7 + k * 3 + j) % words.len()]);
            }
            t
        };
        let mut w: Vec<String> = vec!["94ae", "94ae", "9420", "9420"].into_iter().map(String::from).collect();
        match i % 3 {
            0 => {
                w.extend(["9440", "9440"].map(String::from));
                w.extend(text_words(&row(0)));
                w.extend(["94e0", "94e0"].map(String::from));
                w.extend(text_words(&row(1)));
            }
            1 => {
                w.extend(["9470", "9470"].map(String::from));
                w.extend(text_words(&row(2)));
                w.extend(["91ae", "91ae"].map(String::from));
                w.extend(text_words(&format!("{} {}", row(3), i)));
            }
            _ => {
                w.extend(["9470", "9470"].map(String::from));
                w.extend(text_words("caf\u{0}"));
                w.extend(["92a4", "92a4"].map(String::from));
                w.extend(text_words(&row(4)));
            }
        }
        w.extend(["942f", "942f"].map(String::from));
        s.push_str(&format!("{tc}\t{}\r\n\r\n", w.join(" ")));
        i += 1;
    }
    s
}

#[test]
fn large_file() {
    let text = generate();
    assert!(text.len() >= 5_000_000);
    if let Ok(p) = std::env::var("BILINGUA_SCC_SPEED_OUT") {
        std::fs::write(p, &text).unwrap();
    }
    let opts = ExtractOptions { limits: Some(Limits { max_units: 10_000_000, ..Limits::default() }), ..Default::default() };
    let t0 = Instant::now();
    let mut doc = SccFilter.extract(text.as_bytes(), &opts).unwrap();
    let t_extract = t0.elapsed();
    let n = doc.units.len();
    let t1 = Instant::now();
    for (i, u) in doc.units.iter_mut().enumerate() {
        u.segments[0].target = vec![Inline::text(format!("Salut le monde, num\u{e9}ro {i}"))];
    }
    let out = SccFilter.merge_with(&doc, &MergeOptions::locos()).unwrap();
    let t_merge = t1.elapsed();
    let t2 = Instant::now();
    let untouched = SccFilter.merge(&SccFilter.extract(text.as_bytes(), &opts).unwrap()).unwrap();
    let t_untouched = t2.elapsed();
    assert_eq!(untouched, text.as_bytes());
    println!(
        "scc speed: {:.1} MB, {n} captions, extract {:.0} ms, translate+merge {:.0} ms, parse+untouched merge {:.0} ms, total {:.0} ms",
        text.len() as f64 / 1e6,
        t_extract.as_secs_f64() * 1e3,
        t_merge.as_secs_f64() * 1e3,
        t_untouched.as_secs_f64() * 1e3,
        (t_extract + t_merge).as_secs_f64() * 1e3
    );
    assert!(out.len() > 1_000_000);
}

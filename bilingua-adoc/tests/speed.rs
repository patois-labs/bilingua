//! Speed: one generated file of at least 5 MB, extracted and merged with every unit translated. Run in release for
//! the real number:
//!   cargo test -p bilingua-adoc --release --test speed -- --nocapture
//! Set BILINGUA_ADOC_SPEED_OUT to a path to keep the generated file (tests/tools/speed_locos.py times LocOS on it).
mod common;
use bilingua_adoc::Adoc;
use bilingua_model::*;
use std::time::Instant;

pub fn generate() -> String {
    let mut s = String::with_capacity(5_400_000);
    s.push_str("= Lumen Bakery Handbook\nJane Doe <jane@example.com>\nv1.2, 2026-10-01\n:product: Lumen Bakery\n:toc:\n:description: A generated handbook.\n\n");
    let mut i = 0usize;
    while s.len() < 5_200_000 {
        s.push_str(&format!(
            "== Chapter {i} ==\n\nThe *{i}th* paragraph of the {{product}} handbook has `inline code`, a link to https://example.com/{i}[our site^] and\nsome _emphasis_ that continues on a second line.\n\nNOTE: Read the link:guide-{i}.html[guide] before you begin.\n\n.Options for {i}\n* Pick up in store, see <<map-{i},the map>>\n** Nested item with kbd:[Ctrl+C]\n. First step +\ncontinued after a hard break\n\nCPU:: The brain of the computer.\n\n[source,python]\n----\nprint(\"never translated {i}\")  # *not* a mark\n----\n\n[cols=\"1,2\"]\n|===\n|Name |Description\n\n|Widget {i}\n|A small device with a \\| pipe.\n|===\n\n"
        ));
        i += 1;
    }
    s
}

#[test]
fn large_file() {
    let text = generate();
    let data = text.into_bytes();
    assert!(data.len() >= 5_000_000, "{}", data.len());
    if let Ok(p) = std::env::var("BILINGUA_ADOC_SPEED_OUT") {
        std::fs::write(p, &data).unwrap();
    }
    let limits = Limits::new().with_max_units(10_000_000).with_max_lines(100_000_000);
    let opts = ExtractOptions::new().with_limits(limits).with_target_lang("de-DE");
    let t0 = Instant::now();
    let mut doc = Adoc.extract(&data, &opts).unwrap();
    let t_extract = t0.elapsed();
    let n = doc.units.len();
    assert!(n > 20_000);
    assert_eq!(Adoc.merge(&doc).unwrap(), data);
    let t1 = Instant::now();
    for (i, u) in doc.units.iter_mut().enumerate() {
        let src = u.source();
        let mut t = vec![Inline::text(format!("T{i} "))];
        t.extend(src);
        u.segments[0].target = t;
    }
    let out = Adoc.merge(&doc).unwrap();
    let t_merge = t1.elapsed();
    assert!(out.len() > 1_000_000);
    let back = Adoc.extract(&out, &opts).unwrap();
    assert_eq!(back.units.len(), n);
    println!(
        "adoc speed: {:.1} MB, {n} units, extract {:.0} ms, translate+merge {:.0} ms, total {:.0} ms",
        data.len() as f64 / 1e6,
        t_extract.as_secs_f64() * 1e3,
        t_merge.as_secs_f64() * 1e3,
        (t_extract + t_merge).as_secs_f64() * 1e3
    );
}

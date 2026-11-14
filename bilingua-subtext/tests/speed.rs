//! Speed: one generated file of more than 5 MB per dialect, extracted and merged with every unit translated. Run in
//! release for the real number:
//!   cargo test -p bilingua-subtext --release --test speed -- --nocapture
//! Set BILINGUA_SUBTEXT_SPEED_DIR to a folder to keep the generated files (tests/tools/speed_locos.py times LocOS on them).
mod common;
use bilingua_model::*;
use common::*;
use std::time::Instant;

pub fn generate(kind: &str) -> String {
    let mut s = String::with_capacity(5_600_000);
    let mut i = 0usize;
    if kind == "sv2" {
        s.push_str("[INFORMATION]\r\n[TITLE]Speed\r\n[END INFORMATION]\r\n[SUBTITLE]\r\n");
    }
    while s.len() < 5_300_000 {
        let (a, b) = (i * 3, i * 3 + 2);
        match kind {
            "sbv" => s.push_str(&format!(
                "{}:{:02}:{:02}.{:03},{}:{:02}:{:02}.{:03}\nThis is subtitle number {i} with {{0}} and %s,\nspread over two lines for the test.\n\n",
                a / 3600, a / 60 % 60, a % 60, i % 1000, b / 3600, b / 60 % 60, b % 60, i % 1000
            )),
            "sv2" => s.push_str(&format!(
                "{:02}:{:02}:{:02}.{:02},{:02}:{:02}:{:02}.{:02}\r\nThis is subtitle number {i} with {{0}} and %s,[br]spread over two lines for the test.\r\n\r\n",
                a / 3600, a / 60 % 60, a % 60, i % 100, b / 3600, b / 60 % 60, b % 60, i % 100
            )),
            _ => s.push_str(&format!("{{{}}}{{{}}}{{y:i}}This is subtitle number {i} with {{0}}|{{c:$0000FF}}spread over two lines|{{y:b}}and a third.\n", i * 50, i * 50 + 40)),
        }
        i += 1;
    }
    s
}

#[test]
fn large_files() {
    for (kind, fmt) in [("sbv", "sbv"), ("sv2", "sub"), ("mdvd", "sub")] {
        let text = generate(kind);
        assert!(text.len() > 5_000_000, "{}", text.len());
        let data = text.as_bytes().to_vec();
        if let Ok(dir) = std::env::var("BILINGUA_SUBTEXT_SPEED_DIR") {
            std::fs::write(format!("{dir}/speed_{kind}.{}", if fmt == "sbv" { "sbv" } else { "sub" }), &data).unwrap();
        }
        let limits = Limits::new().with_max_units(10_000_000).with_max_lines(100_000_000);
        let o = ExtractOptions::new().with_limits(limits);
        let f = filter(fmt);
        let t0 = Instant::now();
        let mut doc = f.extract(&data, &o).unwrap();
        let t_extract = t0.elapsed();
        let n = doc.units.len();
        assert!(n > 30_000, "{n}");
        assert_eq!(f.merge(&doc).unwrap(), data, "an untouched merge is the original");
        let t1 = Instant::now();
        for (i, u) in doc.units.iter_mut().enumerate() {
            let mut t = vec![Inline::text(format!("T{i} "))];
            t.extend(u.source());
            u.segments[0].target = t;
        }
        let out = f.merge_with(&doc, &bilingua_model::profile::locos().merge_options("xml")).unwrap();
        let t_merge = t1.elapsed();
        assert!(out.len() > 5_000_000);
        let back = f.extract(&out, &o).unwrap();
        assert_eq!(back.units.len(), n);
        println!(
            "subtext speed {kind}: {:.1} MB, {n} units; extract {:.2} s, translated merge {:.2} s",
            data.len() as f64 / 1e6,
            t_extract.as_secs_f64(),
            t_merge.as_secs_f64()
        );
    }
}

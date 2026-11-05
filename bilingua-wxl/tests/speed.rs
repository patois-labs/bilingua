//! One large generated file (over 5 MB), extracted and merged. Run `cargo test --release -p bilingua-wxl --test
//! speed -- --nocapture` for the timings. Set `WXL_SPEED_OUT` to a path to also save the file, so the LocOS
//! filter can be timed on the same bytes.

mod common;
use bilingua_model::*;
use bilingua_wxl::Wxl;
use common::*;
use std::time::Instant;

pub fn generate() -> Vec<u8> {
    let mut s = String::with_capacity(8_000_000);
    s.push_str("\u{feff}<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n<WixLocalization Culture=\"en-us\" Codepage=\"1252\" xmlns=\"http://wixtoolset.org/schemas/v4/wxl\">\r\n  <!-- Generated: Larchmoor Reader installer -->\r\n");
    let words = ["harbor", "lantern", "bread", "window", "morning", "garden", "ticket", "river", "paper", "market"];
    for n in 0..45_000usize {
        let w = |k: usize| words[(n * 7 + k) % words.len()];
        match n % 3 {
            0 => s.push_str(&format!(
                "  <String Id=\"Msg{n}\" Value=\"Open the {} {} near the {} in [INSTALLFOLDER] and bring {} for &quot;{}&quot; caf&#233; &amp; sentence number {n} of the guide.\" />\r\n",
                w(1), w(2), w(3), w(4), w(5)
            )),
            1 => s.push_str(&format!(
                "  <String Id=\"Msg{n}\" Overridable=\"yes\">Click Next to install [ProductName] with the {} {} before the {} of the guide, item {n}.</String>\r\n",
                w(1), w(2), w(3)
            )),
            _ => s.push_str(&format!("  <UI Dialog=\"Dlg{}\" Control=\"Ctl{n}\" Text=\"The {} is ready to install, number {n}\" />\r\n", n % 50, w(6))),
        }
        if n % 1000 == 0 {
            s.push_str(&format!("  <String Id=\"Guid{n}\" Value=\"{{6F2C1E7A-0000-4000-8000-{n:012}}}\" Localizable=\"no\" />\r\n"));
        }
    }
    s.push_str("</WixLocalization>\r\n");
    s.into_bytes()
}

#[test]
fn large_file_round_trips_and_is_timed() {
    let bytes = generate();
    assert!(bytes.len() >= 5_000_000, "only {} bytes", bytes.len());
    if let Ok(path) = std::env::var("WXL_SPEED_OUT") {
        std::fs::write(path, &bytes).unwrap();
    }
    // The default unit limit is 50 000; this file has 45 000.
    let t0 = Instant::now();
    let mut d = Wxl.extract(&bytes, &opts(false)).unwrap();
    let t_extract = t0.elapsed();
    assert_eq!(d.units.len(), 45_000);
    let t1 = Instant::now();
    let same = Wxl.merge(&d).unwrap();
    let t_merge = t1.elapsed();
    assert_eq!(same, bytes);
    // Translate everything and merge again.
    translate_all(&mut d, |s| format!("\u{ab}{s}\u{bb}"));
    let t2 = Instant::now();
    let out = Wxl.merge(&d).unwrap();
    let t_merge_tr = t2.elapsed();
    let back = Wxl.extract(&out, &opts(false)).unwrap();
    assert_eq!(back.units.len(), d.units.len());
    println!(
        "wxl speed: {} bytes, {} units; extract {:?}, untouched merge {:?}, translated merge {:?}; extract+merge {:?}",
        bytes.len(),
        d.units.len(),
        t_extract,
        t_merge,
        t_merge_tr,
        t_extract + t_merge
    );
}

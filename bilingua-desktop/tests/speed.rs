//! One large generated file per filter (over 5 MB), extracted and merged. Run
//! `cargo test --release -p bilingua-desktop --test speed -- --nocapture` for the timings. Set `DESKTOP_SPEED_OUT`
//! to a folder to also save the two files (`speed.desktop`, `speed.catkeys`), so the LocOS filters can be timed on
//! the same bytes (`tests/tools/speed_locos.py`).

mod common;
use bilingua_desktop::*;
use bilingua_model::*;
use common::*;
use std::time::Instant;

const WORDS: [&str; 10] = ["harbor", "lantern", "bread", "window", "morning", "garden", "ticket", "river", "paper", "market"];

pub fn gen_desktop() -> Vec<u8> {
    let mut s = String::with_capacity(6_000_000);
    s.push_str("[Desktop Entry]\nType=Application\nName=Larchmoor Reader\nExec=reader %F\n\n");
    for n in 0..9_500usize {
        let w = |k: usize| WORDS[(n * 7 + k) % WORDS.len()];
        s.push_str(&format!(
            "[Desktop Action act{n}]\n# the {} action\nName=Open the {} {} near the {}\nName[de]=\u{d6}ffne den {}\nComment=Open the {} and bring {} for caf\u{e9} \\s sentence number {n} of the guide, with a long enough description to be realistic. Extra words: harbor lantern bread window morning garden ticket river paper market, and then some more words to make this line a little longer.\nKeywords={};{};{}\\;x;\nX-Notes=harbor lantern bread window morning garden ticket river paper market harbor lantern bread window morning garden ticket river paper market\nMimeType=text/plain;text/x-notes;application/x-notes;\nExec=reader --act {n}\nIcon=reader-{n}\n\n",
            w(0), w(1), w(2), w(3), w(1), w(4), w(5), w(6), w(7), w(8)
        ));
    }
    s.into_bytes()
}

pub fn gen_catkeys() -> Vec<u8> {
    let mut s = String::with_capacity(6_000_000);
    s.push_str("1\tenglish\tx-vnd.Larchmoor-Reader\t123456789\n");
    for n in 0..40_000usize {
        let w = |k: usize| WORDS[(n * 7 + k) % WORDS.len()];
        s.push_str(&format!(
            "Open the {} {} near the {} and bring {} for \\\"caf\u{e9}\\\" number {n} of the guide.\tWindow{}\tcomment {}\tOpen the {}\n",
            w(1), w(2), w(3), w(4), n % 50, w(5), w(6)
        ));
    }
    s.into_bytes()
}

#[test]
fn large_files_round_trip_and_are_timed() {
    let dk = gen_desktop();
    let ck = gen_catkeys();
    assert!(dk.len() >= 5_000_000, "desktop only {} bytes", dk.len());
    assert!(ck.len() >= 5_000_000, "catkeys only {} bytes", ck.len());
    if let Ok(dir) = std::env::var("DESKTOP_SPEED_OUT") {
        std::fs::write(format!("{dir}/speed.desktop"), &dk).unwrap();
        std::fs::write(format!("{dir}/speed.catkeys"), &ck).unwrap();
    }
    for (name, bytes) in [("desktop", &dk), ("catkeys", &ck)] {
        let fname = format!("x.{name}");
        let o = opts(Some("de-DE"));
        let t0 = Instant::now();
        let mut d = if name == "desktop" { DesktopFilter.extract(bytes, &o) } else { CatkeysFilter.extract(bytes, &o) }.unwrap();
        let t_extract = t0.elapsed();
        let n = d.units.len();
        let t1 = Instant::now();
        let same = merge(&d);
        let t_merge = t1.elapsed();
        assert_eq!(&same, bytes);
        pseudo(&mut d);
        let t2 = Instant::now();
        let out = merge(&d);
        let t_merge_tr = t2.elapsed();
        assert_eq!(extract_any(&fname, &out).unwrap().units.len(), n);
        println!(
            "{name} speed: {} bytes, {n} units; extract {t_extract:?}, untouched merge {t_merge:?}, translated (de-DE) merge {t_merge_tr:?}; extract+merge {:?}",
            bytes.len(),
            t_extract + t_merge_tr
        );
    }
}

//! Speed: one large generated file (7+ MB). Run `cargo test -p bilingua-strings --release --test speed -- --nocapture`
//! for the timing printed; the LocOS filter on the same shape of file is timed by the report's script.
mod common;
use bilingua_model::*;
use bilingua_strings::Strings;
use common::*;
use std::time::Instant;

fn big(n: usize) -> String {
    let mut s = String::new();
    for i in 0..n {
        s.push_str(&format!(
            "/* Label {i} shown in the settings panel */\n\"screen.item.{i}.title\" = \"Hello %@, you have %1$@ new messages in folder {i} and \\\"quoted\\\" text\\nsecond line caf\\U00e9\";\n\n"
        ));
    }
    s
}

#[test]
fn large_file_round_trips_fast() {
    let text = big(45_000);
    assert!(text.len() > 5_000_000);
    for bytes in [text.clone().into_bytes(), {
        let mut v = vec![0xFF, 0xFE];
        for u in text.encode_utf16() {
            v.extend_from_slice(&u.to_le_bytes());
        }
        v
    }] {
        let t = Instant::now();
        let mut d = Strings.extract(&bytes, &ExtractOptions::default()).unwrap();
        let t_extract = t.elapsed();
        assert_eq!(d.units.len(), 45_000);
        let t = Instant::now();
        assert_eq!(Strings.merge(&d).unwrap(), bytes);
        let t_merge = t.elapsed();
        set_nth(&mut d, 44_999, "Hallo %@, du hast %1$@ neue Nachrichten");
        let out = Strings.merge(&d).unwrap();
        assert_ne!(out, bytes);
        println!("{} bytes: extract {t_extract:?}, merge {t_merge:?}", bytes.len());
        assert!(t_extract.as_secs() < 60 && t_merge.as_secs() < 60);
    }
}

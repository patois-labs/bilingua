//! Speed: one large generated plist and one large stringsdict (5 MB and more of text). Run
//! `cargo test -p bilingua-plist --release --test speed -- --nocapture` for the timings. Set
//! `BILINGUA_PLIST_SPEED_OUT=<dir>` to also write the two files there, so the LocOS filter can be timed on the same bytes.
mod common;
use bilingua_model::*;
use common::*;
use std::time::Instant;

fn big_plist(n: usize) -> String {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n");
    for i in 0..n {
        s.push_str(&format!(
            "\t<key>screen.item.{i}.title</key>\n\t<string>Hello %@, you have %1$@ new messages in folder {i} &amp; &quot;quoted&quot; text, caf&#xE9; and a longer tail so that each value is about as long as a real UI string</string>\n\t<key>screen.item.{i}.count</key>\n\t<integer>{i}</integer>\n"
        ));
    }
    s.push_str("</dict>\n</plist>\n");
    s
}

fn big_stringsdict(n: usize) -> String {
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\">\n<dict>\n");
    for i in 0..n {
        s.push_str(&format!(
            "\t<key>entry_{i}</key>\n\t<dict>\n\t\t<key>NSStringLocalizedFormatKey</key>\n\t\t<string>%#@items@</string>\n\t\t<key>items</key>\n\t\t<dict>\n\t\t\t<key>NSStringFormatSpecTypeKey</key>\n\t\t\t<string>NSStringPluralRuleType</string>\n\t\t\t<key>NSStringFormatValueTypeKey</key>\n\t\t\t<string>lld</string>\n\t\t\t<key>one</key>\n\t\t\t<string>%lld item in folder {i} of your account</string>\n\t\t\t<key>other</key>\n\t\t\t<string>%lld items in folder {i} of your account</string>\n\t\t</dict>\n\t</dict>\n"
        ));
    }
    s.push_str("</dict>\n</plist>\n");
    s
}

fn timed(f: F, name: &str, text: &str, units: usize) {
    let bytes = text.as_bytes();
    assert!(bytes.len() > 5_000_000, "{name}: {} bytes", bytes.len());
    let t = Instant::now();
    let mut d = f.filter().extract(bytes, &ExtractOptions { target_lang: Some("ru".into()), ..Default::default() }).unwrap();
    let t_extract = t.elapsed();
    assert_eq!(d.units.len(), units);
    let t = Instant::now();
    assert_eq!(merged(f, &d), bytes);
    let t_merge = t.elapsed();
    if f == F::Sd {
        // every unit translated, and the plural fill for Russian on every variable
        d = prefixed(&d, "Hallo ");
    } else {
        let last = d.units.len() - 1;
        set_nth(&mut d, last, "Hallo %@, du hast %1$@ neue Nachrichten");
    }
    let t = Instant::now();
    let out = merged(f, &d);
    let t_changed = t.elapsed();
    assert_ne!(out, bytes);
    println!("{name}: {} bytes, {units} units: extract {t_extract:?}, merge untouched {t_merge:?}, merge translated {t_changed:?}", bytes.len());
    assert!(t_extract.as_secs() < 60 && t_merge.as_secs() < 60 && t_changed.as_secs() < 60);
}

#[test]
fn large_files_round_trip_fast() {
    let plist = big_plist(30_000);
    let sd = big_stringsdict(12_000);
    if let Ok(dir) = std::env::var("BILINGUA_PLIST_SPEED_OUT") {
        std::fs::write(std::path::Path::new(&dir).join("speed.plist"), &plist).unwrap();
        std::fs::write(std::path::Path::new(&dir).join("speed.stringsdict"), &sd).unwrap();
    }
    timed(F::Plist, "plist", &plist, 30_000);
    timed(F::Sd, "stringsdict", &sd, 24_000);
    // UTF-16 doubles the bytes and takes the same path
    let mut u16 = vec![0xFF, 0xFE];
    for u in plist.encode_utf16() {
        u16.extend_from_slice(&u.to_le_bytes());
    }
    let t = Instant::now();
    let d = ex(F::Plist, &u16);
    assert_eq!(merged(F::Plist, &d), u16);
    println!("plist utf-16: {} bytes: extract + merge {:?}", u16.len(), t.elapsed());
}

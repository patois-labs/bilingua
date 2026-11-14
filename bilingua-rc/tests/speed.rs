//! Speed: one generated file of at least 5 MB (UTF-16 with a BOM, as Visual Studio saves it), extracted and merged
//! with every unit translated. Run in release for the real number:
//!   cargo test -p bilingua-rc --release --test speed -- --nocapture
//! Set BILINGUA_RC_SPEED_OUT to a path to keep the generated file (tests/tools/speed_locos.py times LocOS on it).
mod common;
use bilingua_model::*;
use bilingua_rc::Rc;
use std::time::Instant;

pub fn generate() -> String {
    let mut s = String::with_capacity(6_000_000);
    s.push_str("// Lumen Bakery generated\r\n#include \"resource.h\"\r\nLANGUAGE LANG_ENGLISH, SUBLANG_ENGLISH_US\r\n#pragma code_page(1252)\r\n\r\n");
    let mut i = 0usize;
    while s.len() < 2_700_000 {
        s.push_str(&format!(
            "IDR_MENU{i} MENU\r\nBEGIN\r\n    POPUP \"&File {i}\"\r\n    BEGIN\r\n        MENUITEM \"&Open...\\tCtrl+O\",       ID_O{i}\r\n        MENUITEM SEPARATOR\r\n        MENUITEM \"E&xit\",                   ID_X{i}\r\n    END\r\nEND\r\n\r\n"
        ));
        s.push_str(&format!(
            "IDD_D{i} DIALOGEX 0, 0, 220, 90\r\nSTYLE DS_SETFONT | WS_POPUP\r\nCAPTION \"Choose a pickup time {i}\"\r\nFONT 9, \"Segoe UI\", 400, 0, 0x1\r\nBEGIN\r\n    LTEXT           \"Your order {i} will be ready in 15 minutes.\",IDC_STATIC,7,7,206,10\r\n    CONTROL         \"&Send me a text\",IDC_N{i},\"Button\",BS_AUTOCHECKBOX | WS_TABSTOP,7,42,150,10\r\n    EDITTEXT        IDC_E{i},60,22,150,12\r\n    DEFPUSHBUTTON   \"OK\",IDOK,163,69,50,14\r\nEND\r\n\r\n"
        ));
        s.push_str(&format!(
            "STRINGTABLE\r\nBEGIN\r\n    IDS_G{i}   \"Welcome back, %s! You have %d orders ({i}).\"\r\n    IDS_C{i}   \"Copying %1 to %2!s!...\"\r\n    IDS_Q{i}   \"Say \"\"fresh\"\" to the baker\\nand wait\"\r\n    IDS_J{i}   L\"\\x65E5\\x672C\\x8A9E text {i}\"\r\nEND\r\n\r\n"
        ));
        i += 1;
    }
    s
}

#[test]
fn large_file() {
    let text = generate();
    assert!(text.len() >= 2_500_000);
    // UTF-16 doubles the size on disk: 5 MB and more
    let data = common::utf16le(&text, true);
    assert!(data.len() >= 5_000_000, "{}", data.len());
    if let Ok(p) = std::env::var("BILINGUA_RC_SPEED_OUT") {
        std::fs::write(p, &data).unwrap();
    }
    let limits = Limits::new().with_max_units(10_000_000).with_max_lines(100_000_000);
    let opts = ExtractOptions::new().with_limits(limits).with_target_lang("de-DE");
    let t0 = Instant::now();
    let mut doc = Rc.extract(&data, &opts).unwrap();
    let t_extract = t0.elapsed();
    let n = doc.units.len();
    assert!(n > 20_000);
    // an untouched merge gives the exact bytes
    assert_eq!(Rc.merge(&doc).unwrap(), data);
    let t1 = Instant::now();
    for (i, u) in doc.units.iter_mut().enumerate() {
        let src = u.source();
        let mut t = vec![Inline::text(format!("T{i} \"q\" "))];
        t.extend(src);
        u.segments[0].target = t;
    }
    let out = Rc.merge(&doc).unwrap();
    let t_merge = t1.elapsed();
    assert!(out.len() > 1_000_000);
    let back = Rc.extract(&out, &opts).unwrap();
    assert_eq!(back.units.len(), n);
    println!(
        "rc speed: {:.1} MB, {n} units, extract {:.0} ms, translate+merge {:.0} ms, total {:.0} ms",
        data.len() as f64 / 1e6,
        t_extract.as_secs_f64() * 1e3,
        t_merge.as_secs_f64() * 1e3,
        (t_extract + t_merge).as_secs_f64() * 1e3
    );
}

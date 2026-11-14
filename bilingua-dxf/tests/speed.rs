//! Speed: one generated drawing of at least 5 MB (AutoCAD 2018, UTF-8), extracted and merged with every unit
//! translated. Run in release for the real number:
//!   cargo test -p bilingua-dxf --release --test speed -- --nocapture
//! Set BILINGUA_DXF_SPEED_OUT to a path to keep the generated file (tests/tools/speed_locos.py times LocOS on it).
mod common;
use bilingua_dxf::Dxf;
use bilingua_model::*;
use std::time::Instant;

pub fn generate() -> Vec<u8> {
    let mut s = String::with_capacity(6_000_000);
    let mut h = 100u32;
    let add = |s: &mut String, pairs: &[(i32, String)]| {
        for (c, v) in pairs {
            s.push_str(&format!("{c:>3}\r\n{v}\r\n"));
        }
    };
    s.push_str("  0\r\nSECTION\r\n  2\r\nHEADER\r\n  9\r\n$ACADVER\r\n  1\r\nAC1032\r\n  9\r\n$DWGCODEPAGE\r\n  3\r\nANSI_1252\r\n  0\r\nENDSEC\r\n  0\r\nSECTION\r\n  2\r\nENTITIES\r\n");
    let mut i = 0usize;
    while s.len() < 5_200_000 {
        h += 1;
        let hx = format!("{h:X}");
        add(&mut s, &[(0, "TEXT".into()), (5, hx.clone()), (8, "Walls Text".into()), (10, "1.0".into()), (20, "2.0".into()), (40, "2.5".into()), (1, format!("Room {i}: baking area \u{e9} \u{65e5}\u{672c}")), (7, "Standard".into())]);
        add(&mut s, &[(0, "MTEXT".into()), (5, hx.clone()), (8, "Notes".into()), (10, "1.0".into()), (3, "The contractor shall verify all dimensions on site before fabrication. ".repeat(3)), (1, format!("{{\\fArial|b1|i0;Note {i}}}\\Pcheck \\H2.5x;height\\~now %%c50 \\S1/2; inch"))]);
        add(&mut s, &[(0, "ATTRIB".into()), (5, hx.clone()), (8, "0".into()), (1, format!("Ground floor plan {i}")), (2, "TITLE".into())]);
        add(&mut s, &[(0, "DIMENSION".into()), (5, hx), (2, "*D1".into()), (1, format!("<> typical {i}")), (3, "Standard".into())]);
        add(&mut s, &[(0, "LINE".into()), (8, "0".into()), (10, "0.0".into()), (20, "0.0".into()), (11, "10.0".into()), (21, "10.0".into())]);
        i += 1;
    }
    s.push_str("  0\r\nENDSEC\r\n  0\r\nEOF\r\n");
    s.into_bytes()
}

#[test]
fn large_file() {
    let data = generate();
    assert!(data.len() >= 5_000_000, "{}", data.len());
    // BILINGUA_DXF_SPEED_OUT keeps the file; by default it goes to the temp folder next to the timing line
    let path = std::env::var("BILINGUA_DXF_SPEED_OUT").map(std::path::PathBuf::from).unwrap_or_else(|_| std::env::temp_dir().join("bilingua-dxf-speed.dxf"));
    std::fs::write(&path, &data).unwrap();
    let limits = Limits::new().with_max_units(10_000_000);
    let opts = ExtractOptions::new().with_limits(limits).with_target_lang("de-DE");
    let t0 = Instant::now();
    let mut doc = Dxf.extract(&data, &opts).unwrap();
    let t_extract = t0.elapsed();
    let n = doc.units.len();
    assert!(n > 20_000);
    assert_eq!(Dxf.merge(&doc).unwrap(), data);
    let t1 = Instant::now();
    for (i, u) in doc.units.iter_mut().enumerate() {
        let mut t = vec![Inline::text(format!("T{i} \"q\" "))];
        t.extend(u.source());
        u.segments[0].target = t;
    }
    let out = Dxf.merge(&doc).unwrap();
    let t_merge = t1.elapsed();
    let back = Dxf.extract(&out, &opts).unwrap();
    assert_eq!(back.units.len(), n);
    let line = format!(
        "dxf speed: {:.1} MB, {n} units, extract {:.0} ms, translate+merge {:.0} ms, total {:.0} ms",
        data.len() as f64 / 1e6,
        t_extract.as_secs_f64() * 1e3,
        t_merge.as_secs_f64() * 1e3,
        (t_extract + t_merge).as_secs_f64() * 1e3
    );
    println!("{line}");
    let _ = std::fs::write(path.with_extension("txt"), line);
}

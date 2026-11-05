//! Gate scratch: for a charset label and every 1- and 2-byte sequence (lead >= 0x80), extract a one-entry sitemap that
//! declares that charset and print `HEX<TAB>tag<TAB>decoded-as-hex-codepoints` or `HEX<TAB>ERR`.
use bilingua_htmlhelp::HhcFilter;
use bilingua_model::*;
use std::io::Write;

fn run(label: &str, seq: &[u8], out: &mut impl Write) {
    let mut f = format!("<meta charset={label}><OBJECT type=\"text/sitemap\"><param name=\"Name\" value=\"a").into_bytes();
    f.extend_from_slice(seq);
    f.extend_from_slice(b"\"></OBJECT>");
    let hex: String = seq.iter().map(|b| format!("{b:02x}")).collect();
    match HhcFilter.extract(&f, &ExtractOptions::default()) {
        Ok(d) => {
            let t = plain_text(&d.units[0].source());
            let cps: Vec<String> = t.chars().skip(1).map(|c| format!("{:x}", c as u32)).collect();
            let tag = match &d.skeleton {
                Skeleton::Text { encoding, .. } => encoding.clone(),
                _ => String::new(),
            };
            writeln!(out, "{hex}\t{tag}\t{}", cps.join(" ")).unwrap();
        }
        Err(_) => writeln!(out, "{hex}\tERR").unwrap(),
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let label = &a[1];
    let three = a.get(3).is_some_and(|s| s == "3");
    let mut out = std::io::BufWriter::new(std::fs::File::create(&a[2]).unwrap());
    for l in 0x80u8..=0xFF {
        run(label, &[l], &mut out);
        for t in 0x00u8..=0xFF {
            if t < 0x20 || t == b'"' || t == b'<' || t == b'>' || t == b'&' {
                continue;
            }
            run(label, &[l, t], &mut out);
        }
    }
    if three {
        for t in 0xA1u8..=0xFE {
            for u in 0xA1u8..=0xFE {
                run(label, &[0x8F, t, u], &mut out);
            }
        }
    }
}

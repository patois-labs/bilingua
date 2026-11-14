//! Regression test for the 0.9.1 security audit: a RESX file that starts with two byte-order marks is refused.
//! The decoder takes one; quick-xml skipped the second without counting it, so every span was 3 bytes early
//! (a panic inside a multi-byte character, or the wrong bytes replaced on merge).

use bilingua_model::*;
use bilingua_resx::Resx;

const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

fn resx(lead: &str, value: &str) -> Vec<u8> {
    format!("{lead}<root><resheader name=\"resmimetype\"><value>text/microsoft-resx</value></resheader><data name=\"a\"><value>{value}</value></data></root>").into_bytes()
}

fn with_boms(n: usize, body: &[u8]) -> Vec<u8> {
    let mut v = BOM.repeat(n);
    v.extend_from_slice(body);
    v
}

#[test]
fn a_second_byte_order_mark_is_refused() {
    let hello = "\u{41F}\u{440}\u{438}\u{432}\u{435}\u{442}";
    // a line break before the root (the offset fell inside the BOM), 2-byte characters (inside a letter), and
    // plain ASCII (no panic, but the value's span was `ue>He`)
    for body in [resx("\n", "Hello"), resx("", hello), resx("", "Hello"), resx("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n", hello)] {
        for boms in [2, 3] {
            let r = std::panic::catch_unwind(|| Resx.extract(&with_boms(boms, &body), &ExtractOptions::new()));
            match r {
                Ok(Err(Error::Parse { format, msg })) => {
                    assert_eq!(format, "resx");
                    assert!(msg.contains("byte-order mark"), "{msg}");
                }
                Ok(Err(e)) => panic!("expected a parse error, got {e}"),
                Ok(Ok(d)) => panic!("{boms} BOMs were accepted ({} units)", d.units.len()),
                Err(_) => panic!("{boms} BOMs made the parser panic"),
            }
        }
    }
}

/// No BOM and one BOM read as before: the value is the unit, the untouched merge is the input, and a translation
/// lands in the value and nowhere else.
#[test]
fn zero_and_one_byte_order_mark_read_as_before() {
    let hello = "\u{41F}\u{440}\u{438}\u{432}\u{435}\u{442}";
    for value in ["Hello", hello] {
        for boms in [0, 1] {
            let input = with_boms(boms, &resx("\n", value));
            let mut d = Resx.extract(&input, &ExtractOptions::new()).expect("extract");
            assert_eq!(d.units.len(), 1);
            assert_eq!(plain_text(&d.units[0].source()), value);
            assert_eq!(Resx.merge(&d).expect("merge"), input);
            let id = d.units[0].segments[0].id.clone();
            d.set_target(&id, vec![Inline::text("Hola")]).unwrap();
            let out = Resx.merge(&d).expect("merge");
            assert_eq!(out, with_boms(boms, &resx("\n", "Hola")));
        }
    }
}

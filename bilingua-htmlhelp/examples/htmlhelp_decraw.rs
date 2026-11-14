//! Writes, for every ANSI codec HTML Help may use, what encoding_rs makes of each byte sequence the decoder in
//! `src/enc.rs` tries: every byte 0x80..0xFF alone, every pair with a lead 0x80..0xFF, the EUC-JP triples `8F xx yy`
//! and the GB18030 four-byte sequences. One line per sequence: `ID HEX CODEPOINT` when the sequence is exactly one
//! character, `ID HEX -` otherwise. `tests/tools/gen_decdiff.py` compares this with Python's codecs to produce
//! `src/decdiff.rs`. ID as in `htmlhelp_encmap`.
use bilingua_model::encoding_rs::Encoding;
use std::io::Write;

const IDS: &[(u16, &str)] = &[
    (874, "windows-874"),
    (932, "shift_jis"),
    (936, "gbk"),
    (949, "euc-kr"),
    (950, "big5"),
    (1250, "windows-1250"),
    (1251, "windows-1251"),
    (1252, "windows-1252"),
    (1253, "windows-1253"),
    (1254, "windows-1254"),
    (1255, "windows-1255"),
    (1256, "windows-1256"),
    (1257, "windows-1257"),
    (1258, "windows-1258"),
    (3001, "shift_jis"),
    (3002, "gbk"),
    (3003, "big5"),
    (3004, "euc-kr"),
    (3005, "iso-8859-2"),
    (3006, "iso-8859-5"),
    (3007, "iso-8859-7"),
    (3008, "windows-1254"),
    (3009, "iso-8859-15"),
    (3010, "koi8-r"),
    (3011, "koi8-u"),
    (3012, "euc-jp"),
    (3013, "gb18030"),
];

fn one(e: &'static Encoding, seq: &[u8]) -> Option<char> {
    let (s, bad) = e.decode_without_bom_handling(seq);
    if bad {
        return None;
    }
    let mut it = s.chars();
    let c = it.next()?;
    it.next().is_none().then_some(c)
}

fn line(out: &mut impl Write, id: u16, e: &'static Encoding, seq: &[u8]) {
    let hex: String = seq.iter().map(|b| format!("{b:02x}")).collect();
    match one(e, seq) {
        Some(c) => writeln!(out, "{id} {hex} {:x}", c as u32).unwrap(),
        None => writeln!(out, "{id} {hex} -").unwrap(),
    }
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: htmlhelp_decraw OUT");
    let mut out = std::io::BufWriter::new(std::fs::File::create(path).unwrap());
    for (id, label) in IDS {
        let e = Encoding::for_label(label.as_bytes()).unwrap();
        for l in 0x80u8..=0xFF {
            line(&mut out, *id, e, &[l]);
            if *id >= 1250 && *id < 3000 || matches!(*id, 874 | 3005..=3011) {
                continue; // single-byte code pages: no pairs
            }
            for t in 0u8..=0xFF {
                line(&mut out, *id, e, &[l, t]);
            }
        }
        if *id == 3012 {
            for t in 0xA1u8..=0xFE {
                for u in 0u8..=0xFF {
                    line(&mut out, *id, e, &[0x8F, t, u]);
                }
            }
        }
        if *id == 3013 {
            for a in 0x81u8..=0xFE {
                for b in 0x30u8..=0x39 {
                    for c in 0x81u8..=0xFE {
                        for d in 0x30u8..=0x39 {
                            line(&mut out, *id, e, &[a, b, c, d]);
                        }
                    }
                }
            }
        }
    }
}

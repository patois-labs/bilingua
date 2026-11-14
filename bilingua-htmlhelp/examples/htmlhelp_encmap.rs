//! Writes, for every ANSI codec HTML Help may use, the bytes encoding_rs gives each character U+0080..U+FFFF, one line
//! per character: `ID CODEPOINT HEXBYTES` (or `-` when it cannot encode it). `tests/tools/gen_encdiff.py` compares
//! this with Python's codecs to produce `src/encdiff.rs`. ID is the Windows code page number, or 3001.. for the
//! Python codecs that are not a code page (see `PY_CODECS` in `src/enc.rs`).
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

fn main() {
    let path = std::env::args().nth(1).expect("usage: htmlhelp_encmap OUT");
    let mut out = std::io::BufWriter::new(std::fs::File::create(path).expect("create"));
    for (id, label) in IDS {
        let e = Encoding::for_label(label.as_bytes()).unwrap();
        for u in 0x80u32..=0xFFFF {
            let Some(c) = char::from_u32(u) else { continue };
            let mut b = [0u8; 4];
            let (bytes, _, bad) = e.encode(c.encode_utf8(&mut b));
            if bad {
                writeln!(out, "{id} {u:x} -").unwrap();
            } else {
                let hex: String = bytes.iter().map(|x| format!("{x:02x}")).collect();
                writeln!(out, "{id} {u:x} {hex}").unwrap();
            }
        }
    }
}

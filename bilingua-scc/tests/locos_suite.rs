//! The LocOS suite `tests/verify_scc_filter.py`, ported one to one: its 8 test functions become 8 Rust tests
//! (`test_pipeline` is a web upload and export through the app; its filter-level content is covered by
//! `pipeline_filter_level` here and the refusal text is the same). Fixtures are the hand-written hex of the suite.
mod common;
use bilingua_model::*;
use bilingua_scc::SccFilter;
use common::*;

const LINES: [&str; 9] = [
    "Scenarist_SCC V1.0",
    "",
    // pop-on: ENM, RCL, PAC row 14 col 0 "HELLO", PAC row 15 "WORLD", EOC
    "00:00:01:00\t94ae 94ae 9420 9420 9440 9440 c845 4c4c 4f80 94e0 94e0 574f 524c c480 942f 942f",
    "",
    "00:00:03:00\t942c 942c",
    "",
    // roll-up 2: RU2, CR, PAC row 15, "I say " + italics mid-row + "hi! "
    "00:00:04:00\t9425 9425 94ad 94ad 9470 9470 4920 7361 7920 91ae 91ae 68e9 a120",
    // roll-up continues: CR, PAC, "U" + extended U-umlaut, "nd", special music note
    "00:00:05:00\t94ad 94ad 9470 9470 d580 92a4 92a4 6e64 9137 9137",
    // paint-on (drop-frame timecode): RDC, PAC row 1 indent 8 + tab offset 2, "caf" + e-acute
    "00:00:06;02\t9429 9429 9154 9154 97a2 97a2 e361 e6dc",
];
const LINES2: [&str; 2] = [
    // channel 2 (CC2) text: kept verbatim, not a unit
    "00:00:07:00\t1c20 1c20 1c70 1c70 c1c2",
    // backspace inside the text: kept verbatim
    "00:00:08:00\t9420 9420 9470 9470 c1c2 94a1 94a1 43c4 942f 942f",
];

fn scc() -> String {
    let mut v: Vec<&str> = LINES.to_vec();
    v.extend(LINES2);
    v.join("\r\n") + "\r\n"
}

fn texts(data: &[u8]) -> Vec<(usize, String)> {
    indexed(data)
}

fn s(i: usize, t: &str) -> (usize, String) {
    (i, t.to_string())
}

#[test]
fn registered() {
    assert_eq!(SccFilter.id(), "scc");
    assert_eq!(SccFilter.extensions(), &["scc"]);
    assert!(SccFilter.sniff(scc().as_bytes()));
}

#[test]
fn identity() {
    let base = scc();
    let mut bom = b"\xef\xbb\xbf".to_vec();
    bom.extend(base.as_bytes());
    for data in [base.as_bytes().to_vec(), base.replace("\r\n", "\n").into_bytes(), bom] {
        assert_identity(&data);
    }
}

#[test]
fn decode() {
    let data = scc().into_bytes();
    assert_eq!(
        texts(&data),
        vec![s(0, "HELLO\nWORLD"), s(1, "I say \u{27e6}x1\u{27e7}hi!"), s(2, "\u{dc}nd\u{266a}"), s(3, "caf\u{e9}")]
    );
    let doc = extract(&data).unwrap();
    // two lines are kept verbatim (LocOS: skeleton["scc_lines_kept_verbatim"] == 2)
    assert!(doc.warnings.iter().any(|w| w.kind == "scc_lines_kept_verbatim" && w.message.starts_with("2 ")), "{:?}", doc.warnings);
}

#[test]
fn translate() {
    let data = scc().into_bytes();
    let out = build(&data, &[(0, "BONJOUR\nMONDE"), (3, "th\u{e9}")]);
    let o: Vec<String> = String::from_utf8(out.clone()).unwrap().lines().map(String::from).collect();
    let src: Vec<&str> = scc().leak().lines().collect();
    assert_eq!(o.len(), src.len());
    for (a, b) in o.iter().zip(&src) {
        if !(a.starts_with("00:00:01:00") || a.starts_with("00:00:06;02")) {
            assert_eq!(a, b);
        }
    }
    assert_eq!(
        line_of(&out, "00:00:01:00"),
        "00:00:01:00\t94ae 94ae 9420 9420 9440 9440 c24f ce4a 4fd5 5280 94e0 94e0 cd4f cec4 4580 942f 942f"
    );
    assert_eq!(texts(&out)[0], s(0, "BONJOUR\nMONDE"));
    assert_eq!(texts(&out)[3], s(3, "th\u{e9}"));
    // extended and special characters: fallback + extended code, doubled specials
    let out = build(&data, &[(2, "\u{dc}ber \u{266a} \u{ab}oui\u{bb}")]);
    assert_eq!(texts(&out)[2], s(2, "\u{dc}ber \u{266a} \u{ab}oui\u{bb}"));
    assert!(line_of(&out, "00:00:05:00").contains("d580 92a4 92a4"));
    assert!(line_of(&out, "00:00:05:00").contains("9137 9137"));
    // the mid-row token takes the place of the space before it
    let out = build(&data, &[(1, "Je dis \u{27e6}x1\u{27e7}salut !")]);
    assert_eq!(texts(&out)[1], s(1, "Je dis \u{27e6}x1\u{27e7}salut !"));
    assert!(line_of(&out, "00:00:04:00").contains("4ae5 2064 e973 91ae 91ae"), "{}", line_of(&out, "00:00:04:00"));
    // typographic apostrophe and dashes map to 608 equivalents
    let out = build(&data, &[(3, "l\u{2019}\u{e9}t\u{e9} \u{2013} oui\u{2026}")]);
    assert_eq!(texts(&out)[3], s(3, "l'\u{e9}t\u{e9} - oui..."));
}

#[test]
fn layout() {
    let data = scc().into_bytes();
    // one long line re-wrapped into the caption's two rows, balanced
    let out = build(&data, &[(0, "BONJOUR TOUT LE MONDE, COMMENT ALLEZ-VOUS CE SOIR ?")]);
    let t = &texts(&out)[0].1;
    let rows: Vec<&str> = t.split('\n').collect();
    assert!(rows.len() == 2 && rows.iter().all(|r| r.chars().count() <= 32), "{rows:?}");
    // four rows needed: rows 12-15, bottom row kept at 15
    let long = ["Voici une phrase vraiment longue"; 4].join(" ");
    let out = build(&data, &[(0, long.as_str())]);
    let ln = line_of(&out, "00:00:01:00");
    for pac in ["1340", "13e0", "9440", "94e0"] {
        assert!(ln.contains(&format!("{pac} {pac}")), "{pac} {ln}");
    }
    assert_eq!(texts(&out)[0].1.split('\n').count(), 4);
    // paint-on row at column 10 (indent 8 + tab 2): a 30-cell row moves left to column 2
    let out = build(&data, &[(3, &"x".repeat(30))]);
    let ln = line_of(&out, "00:00:06;02");
    assert!(ln.starts_with("00:00:06;02\t9429 9429 91d0 91d0 97a2 97a2 "), "{ln}");
    // roll-up: a second row is added with a carriage return on the same base row
    let out = build(&data, &[(1, "Une phrase assez longue pour deux lignes de roll-up")]);
    let ln = line_of(&out, "00:00:04:00");
    assert_eq!(ln.matches("94ad 94ad").count(), 2, "{ln}");
    assert_eq!(ln.matches("9470 9470").count(), 2, "{ln}");
    assert_eq!(texts(&out)[1].1.split('\n').count(), 2);
}

#[test]
fn refusals() {
    let data = scc().into_bytes();
    let err = try_build(&data, &[s(0, "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442} \u{43c}\u{438}\u{440}"), s(3, "\u{65e5}\u{672c}\u{8a9e}")])
        .expect_err("Cyrillic and CJK must be refused");
    let msg = err.to_string();
    assert!(msg.contains("caption 1 at 00:00:01:00") && msg.contains("caption 4 at 00:00:06;02"), "{msg}");
    let err = try_build(&data, &[s(1, &"word ".repeat(40))]).expect_err("overflow must be refused");
    assert!(err.to_string().contains("does not fit in 2 rows of 32"), "{err}");
}

#[test]
fn hostile() {
    let bad: Vec<Vec<u8>> = vec![
        b"00:00:01:00\t9420 9420\n".to_vec(),
        b"Scenarist_SCC V1.0\n\n00:00:01:00\t9420 94zz\n".to_vec(),
        b"Scenarist_SCC V1.0\n\nnot a caption line\n".to_vec(),
        [b"Scenarist_SCC V1.0\n\n00:00:01:00\t".to_vec(), b"9420 ".repeat(5000), b"\n".to_vec()].concat(),
    ];
    for data in &bad {
        let e = extract(data).expect_err("must refuse");
        assert!(matches!(e, Error::Parse { .. } | Error::Limit(_)), "{e}");
    }
    // the segment cap (LocOS MAX_DOCUMENT_SEGMENTS = 2)
    let limits = Limits { max_units: 2, ..Limits::default() };
    let opts = ExtractOptions { limits: Some(limits), ..Default::default() };
    let e = SccFilter.extract(scc().as_bytes(), &opts).expect_err("segment cap must refuse");
    assert!(matches!(e, Error::Limit(ref l) if l.kind == LimitKind::Units), "{e}");
}

#[test]
fn pipeline_filter_level() {
    // test_pipeline in LocOS fills every unit with "SALUT " through the web app and exports; here the same at
    // filter level: every unit translated, the verbatim lines untouched, a non-608 fill refuses with its caption.
    let data = scc().into_bytes();
    let mapping: Vec<(usize, String)> = (0..4).map(|i| (i, "SALUT ".to_string() + &i.to_string())).collect();
    let out = try_build(&data, &mapping).unwrap();
    let t = texts(&out);
    assert_eq!(t.len(), 4);
    assert!(t.iter().all(|(_, x)| x.starts_with("SALUT")));
    assert_eq!(line_of(&out, "00:00:07:00"), line_of(&data, "00:00:07:00"));
    assert_eq!(line_of(&out, "00:00:08:00"), line_of(&data, "00:00:08:00"));
    let mapping: Vec<(usize, String)> = (0..4).map(|i| (i, "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442} ".to_string())).collect();
    let err = try_build(&data, &mapping).unwrap_err().to_string();
    assert!(err.contains("caption 1 at 00:00:01:00"), "{err}");
}

#[test]
fn parity_table() {
    // _word(0x14,0x20) == "9420", (0x14,0x2f) == "942f", (0x11,0x37) == "9137": pinned through the encoder
    let data = scc().into_bytes();
    let out = build(&data, &[(2, "\u{266a}")]);
    assert!(line_of(&out, "00:00:05:00").contains("9137 9137"));
    assert!(line_of(&build(&data, &[(0, "A\nB")]), "00:00:01:00").contains("942f 942f"));
}

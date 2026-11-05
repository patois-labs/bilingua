//! The LocOS DXF suite (`tests/verify_dxf_filter.py`, 10 test functions) ported case by case, with the same
//! fixtures. Test names carry the LocOS function name.
mod common;
use bilingua_dxf::Dxf;
use bilingua_model::encoding_rs::{SHIFT_JIS, WINDOWS_1251};
use bilingua_model::*;
use common::*;

fn long() -> String {
    "The contractor shall verify all dimensions on site before fabrication. ".repeat(5).trim().to_string()
}

fn chunks() -> [String; 2] {
    let l = long();
    [l[..250].to_string(), l[250..].to_string()]
}

const MT: &str = r"{\fArial|b1|i0|c0|p34;Bold note} and \H2.5x;tall\Pnext line with \S1/2; inch and \U+00E9t\~e";

fn sample_pairs(ver: &str, cp: &str) -> Vec<(i32, String)> {
    let c = chunks();
    let mut v = header(ver, cp);
    v.extend(p(&[
        (0, "SECTION"), (2, "TABLES"), (0, "TABLE"), (2, "LAYER"), (5, "2"), (0, "LAYER"), (5, "10"), (2, "Walls Text"),
        (70, "0"), (62, "7"), (6, "CONTINUOUS"), (0, "ENDTAB"), (0, "ENDSEC"),
        (0, "SECTION"), (2, "BLOCKS"), (0, "BLOCK"), (5, "20"), (8, "0"), (2, "TITLEBLOCK"), (70, "2"), (1, "xref path text"),
        (0, "ATTDEF"), (5, "21"), (8, "0"), (1, "Untitled"), (3, "Enter drawing title"), (2, "TITLE"),
        (0, "TEXT"), (5, "22"), (8, "0"), (1, "Patois Labs Engineering"),
        (0, "ENDBLK"), (5, "23"), (0, "ENDSEC"),
        (0, "SECTION"), (2, "ENTITIES"),
        (0, "TEXT"), (5, "30"), (8, "Walls Text"), (10, "1.0"), (1, "Living room"), (7, "Standard"),
        (0, "TEXT"), (5, "31"), (8, "Walls Text"), (1, "1200"),
        (0, "TEXT"), (5, "32"), (8, "Walls Text"), (1, "%%c50 pipe, 45%%d bend"),
        (0, "MTEXT"), (5, "33"), (8, "Notes"),
    ]));
    v.push((3, c[0].clone()));
    v.push((1, c[1].clone()));
    v.extend(p(&[(7, "Standard"), (1001, "ACAD"), (1000, "do not translate me"), (0, "MTEXT"), (5, "34"), (8, "Notes")]));
    v.push((1, MT.to_string()));
    v.extend(p(&[
        (0, "INSERT"), (5, "35"), (2, "TITLEBLOCK"), (66, "1"),
        (0, "ATTRIB"), (5, "36"), (1, "Ground floor plan"), (2, "TITLE"),
        (0, "SEQEND"), (5, "37"),
        (0, "DIMENSION"), (5, "38"), (2, "*D1"), (1, "<> typical"), (3, "Standard"),
        (0, "DIMENSION"), (5, "39"), (2, "*D2"), (1, "<>"), (3, "Standard"),
        (0, "MULTILEADER"), (5, "3A"), (300, "CONTEXT_DATA{"), (304, r"Main\Pentrance"), (301, "}"),
        (0, "ENDSEC"),
        (0, "SECTION"), (2, "OBJECTS"), (0, "DICTIONARY"), (5, "C"), (3, "ACAD_GROUP"), (350, "D"), (0, "ENDSEC"),
        (0, "EOF"),
    ]));
    v
}

fn sample() -> Vec<u8> {
    dxf(&sample_pairs("AC1032", "ANSI_1252"), "\r\n", utf8)
}

fn pairs_of(b: &[u8]) -> Vec<(i32, String)> {
    let s = String::from_utf8(b.to_vec()).unwrap();
    let mut lines: Vec<&str> = Vec::new();
    let mut cur = 0;
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\r' || bytes[i] == b'\n' {
            lines.push(&s[cur..i]);
            if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
                i += 1;
            }
            cur = i + 1;
        }
        i += 1;
    }
    if cur < s.len() {
        lines.push(&s[cur..]);
    }
    (0..lines.len() / 2).map(|k| (lines[2 * k].trim().parse().unwrap(), lines[2 * k + 1].to_string())).collect()
}

fn find(hay: &[u8], needle: &[u8]) -> usize {
    hay.windows(needle.len()).position(|w| w == needle).unwrap()
}

fn has(ps: &[(i32, String)], c: i32, v: &str) -> bool {
    ps.iter().any(|(a, b)| *a == c && b == v)
}

fn tx(s: &[(usize, &str)]) -> Vec<(usize, String)> {
    m(s)
}

#[test]
fn test_registered() {
    assert_eq!(Dxf.id(), "dxf");
    assert!(Dxf.extensions().contains(&"dxf"));
    assert!(Dxf.sniff(&sample()));
    assert!(!Dxf.sniff(b"hello\nworld\n"));
}

#[test]
fn test_parse_units() {
    let d = extract(&sample()).unwrap();
    let got: Vec<(String, String)> = d.units.iter().map(|u| (u.context.style.clone().unwrap(), token_text(u))).collect();
    let g = |i: usize| (got[i].0.as_str(), got[i].1.as_str());
    assert_eq!(g(0), ("dxf:attdef", "Untitled"));
    assert_eq!(g(1), ("dxf:prompt", "Enter drawing title"));
    assert_eq!(g(2), ("dxf:text", "Patois Labs Engineering"));
    assert_eq!(g(3), ("dxf:text", "Living room"));
    assert_eq!(g(4), ("dxf:text", "⟦x1⟧50 pipe, 45⟦x2⟧ bend"));
    assert_eq!(g(5), ("dxf:mtext", long().as_str()));
    assert_eq!(g(6).0, "dxf:mtext");
    assert_eq!(g(6).1, "⟦g1⟧⟦x2⟧Bold note⟦/g1⟧ and ⟦x3⟧tall⟦x4⟧next line with ⟦x5⟧ inch and \u{e9}t\u{a0}e");
    assert_eq!(g(7), ("dxf:attrib", "Ground floor plan"));
    assert_eq!(g(8), ("dxf:dimension", "⟦x1⟧ typical"));
    assert_eq!(g(9), ("dxf:mleader", "Main⟦x1⟧entrance"));
    assert_eq!(got.len(), 10, "{got:?}"); // "1200", bare "<>", layer/block names, xdata, objects: never units
    let all: String = got.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join(" ");
    for never in ["Walls Text", "TITLEBLOCK", "xref path", "do not translate", "ACAD_GROUP", "Standard"] {
        assert!(!all.contains(never), "{never}");
    }
}

#[test]
fn test_identity_round_trip() {
    for nl in ["\r\n", "\n"] {
        for (ver, cp, enc) in [("AC1032", "ANSI_1252", utf8 as fn(&str) -> Vec<u8>), ("AC1015", "ANSI_1252", cp1252 as fn(&str) -> Vec<u8>)] {
            let data = dxf(&sample_pairs(ver, cp), nl, enc);
            assert_identity(&data);
            let doc = extract(&data).unwrap();
            assert_eq!(Dxf.merge(&doc).unwrap(), data);
        }
    }
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend(sample());
    assert_identity(&bom);
}

#[test]
fn test_translated_round_trip() {
    let data = sample();
    let before = units(&data);
    let tr: Vec<(usize, String)> = vec![
        (0, "Sans titre".into()),
        (1, "Saisir le titre du dessin".into()),
        (3, "Séjour".into()),
        (4, "⟦x1⟧50 tuyau, coude 45⟦x2⟧".into()),
        (5, "L'entrepreneur doit vérifier toutes les cotes sur place avant la fabrication. ".repeat(7)),
        (6, "⟦g1⟧⟦x2⟧Note en gras⟦/g1⟧ et ⟦x3⟧grand⟦x4⟧ligne suivante ⟦x5⟧ pouce".into()),
        (7, "Plan du rez-de-chaussée".into()),
        (8, "⟦x1⟧ typique".into()),
        (9, "Entrée⟦x1⟧principale".into()),
    ];
    let out = build(&data, &tr);
    let back = units(&out);
    assert_eq!(back.len(), before.len());
    for (k, v) in &tr {
        assert_eq!(&back[*k], v, "unit {k}");
    }
    let a = pairs_of(&data);
    let b = pairs_of(&out);
    let strip = |ps: &[(i32, String)]| -> Vec<(i32, String)> { ps.iter().filter(|(c, _)| ![1, 3, 304].contains(c)).cloned().collect() };
    assert_eq!(strip(&a), strip(&b));
    assert!(has(&b, 1, "Patois Labs Engineering") && has(&b, 1, "1200") && has(&b, 3, "Standard"));
    assert!(has(&b, 1, "xref path text") && has(&b, 1000, "do not translate me") && has(&b, 3, "ACAD_GROUP"));
    assert!(has(&b, 1, r"{\fArial|b1|i0|c0|p34;Note en gras} et \H2.5x;grand\Pligne suivante \S1/2; pouce"));
    assert!(has(&b, 304, r"Entrée\Pprincipale") && has(&b, 1, "<> typique"));
    // MTEXT re-chunked: group 3 pieces of <= 250 chars then a final group 1 under 250
    let i = b.iter().position(|x| *x == (5, "33".to_string())).unwrap();
    let mut chunk: Vec<(i32, String)> = Vec::new();
    for (code, val) in &b[i + 1..] {
        if *code == 1 || *code == 3 {
            chunk.push((*code, val.clone()));
        } else if !chunk.is_empty() {
            break;
        }
    }
    let (last, rest) = chunk.split_last().unwrap();
    assert!(rest.iter().all(|(c, _)| *c == 3) && last.0 == 1, "{chunk:?}");
    assert!(chunk.iter().all(|(_, v)| v.chars().count() <= 250) && last.1.chars().count() < 250);
    assert_eq!(chunk.iter().map(|(_, v)| v.as_str()).collect::<String>(), tr[4].1);
}

#[test]
fn test_short_mtext_collapses_chunks() {
    let data = sample();
    let out = build(&data, &tx(&[(5, "Verify dimensions.")]));
    let b = pairs_of(&out);
    let i = b.iter().position(|x| *x == (5, "33".to_string())).unwrap();
    assert_eq!(b[i + 1..i + 4], p(&[(8, "Notes"), (1, "Verify dimensions."), (7, "Standard")])[..]);
    // only the MTEXT lines changed: everything before it is a byte prefix
    let pos = find(&data, chunks()[0].as_bytes());
    assert_eq!(out[..pos - 5], data[..pos - 5]);
    let tail = &data[find(&data, b"\r\n  7\r\nStandard\r\n1001")..];
    assert!(out.ends_with(tail));
}

#[test]
fn test_inline_tokens_and_escaping() {
    let data = sample();
    // a dropped closing brace token drops its opener too; literal braces/backslash escaped
    let out = build(&data, &tx(&[(6, "⟦g1⟧⟦x2⟧Gras { a\\b } x^y\nnext")]));
    let b = pairs_of(&out);
    let k = b.iter().position(|x| *x == (5, "34".to_string())).unwrap();
    assert_eq!(b[k + 2].1, r"\fArial|b1|i0|c0|p34;Gras \{ a\\b \} x^ y\Pnext");
    // TEXT: newline becomes a space, %% is tripled per percent, caret escaped
    let out = build(&data, &tx(&[(3, "Salon\n100%% ^ok")]));
    assert!(has(&pairs_of(&out), 1, "Salon 100%%%%%% ^ ok"));
    // unknown tokens dropped, known ones kept
    let out = build(&data, &tx(&[(4, "⟦x9⟧tuyau ⟦x1⟧50")]));
    assert!(has(&pairs_of(&out), 1, "tuyau %%c50"));
}

fn enc1251(s: &str) -> Vec<u8> {
    WINDOWS_1251.encode(s).0.into_owned()
}

#[test]
fn test_legacy_code_page_and_unicode_escape() {
    let mut pr = header("AC1015", "ANSI_1251");
    pr.extend(p(&[
        (0, "SECTION"), (2, "ENTITIES"), (0, "TEXT"), (5, "1"), (1, "Гостиная"), (0, "TEXT"), (5, "2"),
        (1, r"Caf\U+00E9 \U+4E2D"), (0, "MTEXT"), (5, "3"), (1, r"Plan\M+18A8Aend"), (0, "ENDSEC"), (0, "EOF"),
    ]));
    let data = dxf(&pr, "\r\n", enc1251);
    let d = extract(&data).unwrap();
    assert!(matches!(&d.skeleton, Skeleton::Text { encoding, .. } if encoding.ends_with("cp1251")));
    let kanji = SHIFT_JIS.decode(&[0x8A, 0x8A]).0.into_owned();
    assert_eq!(units(&data), vec!["Гостиная".to_string(), "Café 中".to_string(), format!("Plan{kanji}end")]);
    assert_eq!(build(&data, &tx(&[(0, "Гостиная")])), data);
    let out = build(&data, &tx(&[(0, "リビング Комната"), (1, "Кафе")]));
    assert!(find_opt(&out, &enc1251("Комната")).is_some());
    assert!(find_opt(&out, b"\\U+30EA\\U+30D3\\U+30F3\\U+30B0").is_some());
    assert_eq!(units(&out)[..2], ["リビング Комната".to_string(), "Кафе".to_string()]);
    // long Japanese MTEXT in a legacy page: re-chunked without splitting a \U+ escape
    let jp = "日本語のテキスト".repeat(30);
    let out = build(&data, &tx(&[(2, &jp)]));
    for (c, v) in pairs_of_enc(&out) {
        if c == 1 || c == 3 {
            assert!(v.chars().count() <= 250, "{v}");
            assert!(!partial_escape(&v), "{v}");
        }
    }
    assert_eq!(units(&out)[2], jp);
    // non-BMP: surrogate pair escapes, decoded back
    let out = build(&data, &tx(&[(1, "Cafe \u{1f600}")]));
    assert!(find_opt(&out, b"\\U+D83D\\U+DE00").is_some());
    assert_eq!(units(&out)[1], "Cafe \u{1f600}");
}

fn find_opt(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn pairs_of_enc(b: &[u8]) -> Vec<(i32, String)> {
    // the escapes keep the text ASCII-safe in cp1251 except Cyrillic; decode as Windows-1251
    let s = WINDOWS_1251.decode(b).0.into_owned();
    pairs_of(s.as_bytes())
}

/// `\\(U\+?[0-9A-F]{0,3})?$`
fn partial_escape(v: &str) -> bool {
    let Some(k) = v.rfind('\\') else { return false };
    let r = &v[k + 1..];
    if r.is_empty() {
        return true;
    }
    match r.strip_prefix('U') {
        Some(t) => {
            let t = t.strip_prefix('+').unwrap_or(t);
            t.len() <= 3 && t.chars().all(|c| matches!(c, '0'..='9' | 'A'..='F'))
        }
        None => false,
    }
}

#[test]
fn test_encoding_selection() {
    let body = p(&[(0, "SECTION"), (2, "ENTITIES"), (0, "TEXT"), (1, "Größe"), (0, "ENDSEC"), (0, "EOF")]);
    let tag = |d: &[u8]| match extract(d).unwrap().skeleton {
        Skeleton::Text { encoding, .. } => encoding,
        _ => unreachable!(),
    };
    // R12 with no header: cp1252 unless valid UTF-8
    let r12 = dxf(&body, "\r\n", cp1252);
    assert_eq!(units(&r12), ["Größe"]);
    assert!(tag(&r12).ends_with("cp1252"));
    let u8d = dxf(&body, "\r\n", utf8);
    assert_eq!(units(&u8d), ["Größe"]);
    assert!(tag(&u8d).ends_with("utf-8"));
    // AC1021+ is UTF-8 whatever $DWGCODEPAGE says
    let mut h = header("AC1021", "ANSI_1251");
    h.extend(body.clone());
    assert_eq!(units(&dxf(&h, "\r\n", utf8)), ["Größe"]);
    // invalid UTF-8 in an AC1021 value: refused
    let mut h = header("AC1027", "");
    h.extend(body.clone());
    match extract(&dxf(&h, "\r\n", cp1252)) {
        Err(Error::Encoding(m)) => assert!(m.contains("utf-8"), "{m}"),
        other => panic!("{other:?}"),
    }
    // unknown code page refused
    let mut h = header("AC1015", "ANSI_9999");
    h.extend(body);
    match extract(&dxf(&h, "\r\n", utf8)) {
        Err(Error::Encoding(m)) => assert!(m.contains("ANSI_9999"), "{m}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_hostile_inputs() {
    let binary = b"AutoCAD Binary DXF\r\n\x1a\x00\x00\x00SECTION\x00";
    match extract(binary) {
        Err(Error::Unsupported(m)) => assert!(m.contains("binary")),
        other => panic!("{other:?}"),
    }
    for (bad, why) in [
        (&b"hello world\nthis is not dxf\n"[..], "not a dxf"),
        (&b"  0\r\nSECTION\r\n  2\r\nENTITIES\r\nnot-a-code\r\nx\r\n"[..], "line 5"),
        (&b"%PDF-1.7\n"[..], "not a dxf"),
    ] {
        match extract(bad) {
            Err(e @ Error::Parse { .. }) => assert!(e.to_string().contains(why), "{e}"),
            other => panic!("accepted: {other:?}"),
        }
    }
    let o = |max_units| ExtractOptions { limits: Some(Limits { max_units, ..Limits::default() }), ..Default::default() };
    let mut many = p(&[(0, "SECTION"), (2, "ENTITIES")]);
    for _ in 0..5 {
        many.extend(p(&[(0, "TEXT"), (1, "word")]));
    }
    many.push((0, "EOF".into()));
    assert!(matches!(Dxf.extract(&dxf(&many, "\r\n", utf8), &o(3)), Err(Error::Limit(e)) if e.kind == LimitKind::Units));
    let mut flood = b"  0\n".to_vec();
    flood.extend(b"999\nx\n".repeat(1300));
    match Dxf.extract(&flood, &o(3)) {
        Err(Error::Limit(e)) => assert!(e.msg.contains("group codes")),
        other => panic!("{other:?}"),
    }
    // unbalanced braces and a dangling backslash never crash, and round-trip identically
    let odd = dxf(&p(&[(0, "SECTION"), (2, "ENTITIES"), (0, "MTEXT"), (1, "{\\fArial;open } } { \\"), (0, "ENDSEC"), (0, "EOF")]), "\r\n", utf8);
    let t = units(&odd);
    assert_eq!(t.len(), 1);
    assert_eq!(build(&odd, &[(0, t[0].clone())]), odd);
    let rebuilt = build(&odd, &[(0, t[0].replace("open", "ouvert"))]);
    assert_eq!(units(&rebuilt), vec![t[0].replace("open", "ouvert")]);
    // an empty / text-free drawing has no units
    let empty = dxf(&p(&[(0, "SECTION"), (2, "ENTITIES"), (0, "TEXT"), (1, "  "), (0, "ENDSEC"), (0, "EOF")]), "\r\n", utf8);
    assert!(units(&empty).is_empty());
}

#[test]
fn test_end_to_end_upload_export() {
    // LocOS: upload, save one segment, export. Here: one translated unit splices one value.
    let data = sample();
    let i = units(&data).iter().position(|t| t == "Living room").unwrap();
    let out = build(&data, &tx(&[(i, "Wohnzimmer")]));
    let want: Vec<u8> = {
        let from = b"\r\nLiving room\r\n";
        let at = find(&data, from);
        let mut v = data[..at].to_vec();
        v.extend_from_slice(b"\r\nWohnzimmer\r\n");
        v.extend_from_slice(&data[at + from.len()..]);
        v
    };
    assert_eq!(out, want);
}

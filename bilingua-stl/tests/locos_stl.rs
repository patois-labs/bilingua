//! The LocOS suite `tests/verify_stl_filter.py` ported one to one (11 test functions of 11), on the same hand-built
//! fixture file.

mod common;
use bilingua_model::*;
use bilingua_stl::{language_code, Stl};
use common::*;

fn subs() -> Vec<Vec<u8>> {
    let dh_end = |s: &[u8]| cat(&[DH, s, END]);
    vec![
        t(1, &cat(&[&dh_end(b"Hello there,"), b"\x8a\x8a", &dh_end(b"how are you?")])),
        tti(&Tti { sn: 2, vp: 22, tci: [10, 0, 4, 0], tco: [10, 0, 6, 0], ..Default::default() }, &dh_end(b"Caf\xc2e cr\xc8eme")),
        tti(&Tti { sn: 3, tci: [10, 0, 7, 0], tco: [10, 0, 9, 0], ..Default::default() }, &dh_end(b"Red \x01alert\x07 now")),
        tti(&Tti { sn: 4, cf: 1, ..Default::default() }, b"Translator note: check the name"),
        tti(&Tti { sn: 4, ebn: 0xFE, ..Default::default() }, b"user data block"),
        tti(&Tti { sn: 5, ebn: 0x00, ..Default::default() }, &cat(&[DH, b"A long subtitle in two ", END])),
        t(5, &cat(&[b"\x8a\x8a", DH, b"extension blocks.", END])),
        t(6, b"\x80Italic\x81 open text $5 #1"),
    ]
}

fn stl() -> Vec<u8> {
    let s = subs();
    let mut v = gsi_default(s.len());
    for b in &s {
        v.extend_from_slice(b);
    }
    v
}

fn nsubs() -> usize {
    subs().len()
}

fn build_plain(data: &[u8], map: &[(usize, &str)]) -> Result<Vec<u8>, Error> {
    let m: Vec<(usize, String)> = map.iter().map(|(k, s)| (*k, s.to_string())).collect();
    build(data, &m)
}

fn unit_texts(data: &[u8]) -> Vec<(usize, String)> {
    texts(&ex(data)).into_iter().enumerate().collect()
}

fn write_msg(r: Result<Vec<u8>, Error>) -> String {
    match r {
        Err(Error::Write(m)) => m,
        other => panic!("expected a refusal, got {:?}", other.map(|b| b.len())),
    }
}

#[test]
fn registered() {
    assert!(Stl.extensions().contains(&"stl"));
    assert_eq!(Stl.id(), "stl");
    assert!(Stl.sniff(&stl()));
    assert!(!Stl.sniff(b"solid cube\nfacet normal 0 0 1\n"));
}

#[test]
fn identity() {
    let data = stl();
    let doc = ex(&data);
    assert_eq!(Stl.merge(&doc).unwrap(), data);
    assert_eq!(build_plain(&data, &[]).unwrap(), data);
    // same-text targets
    let mut d2 = ex(&data);
    for k in 0..d2.units.len() {
        let src = locos_text(&d2.units[k]);
        set(&mut d2, k, &src);
    }
    assert_eq!(Stl.merge_with(&d2, &MergeOptions::locos()).unwrap(), data);
    assert_eq!(Stl.merge(&d2).unwrap(), data);
}

#[test]
fn decode() {
    let got = unit_texts(&stl());
    let want = vec![
        (0, "Hello there,\nhow are you?"),
        (1, "Caf\u{e9} cr\u{eb}me"),
        (2, "Red \u{27E6}x1\u{27E7}alert \u{27E6}x2\u{27E7} now"),
        (3, "A long subtitle in two\nextension blocks."),
        (4, "Italic\u{27E6}x1\u{27E7} open text $5 #1"),
    ];
    let want: Vec<(usize, String)> = want.into_iter().map(|(i, s)| (i, s.to_string())).collect();
    assert_eq!(got, want);
}

#[test]
fn translate() {
    let data = stl();
    let out = build_plain(&data, &[(0, "Bonjour,\ncomment \u{e7}a va ?"), (1, "Caf\u{e9} cr\u{e8}me br\u{fb}l\u{e9}e")]).unwrap();
    let (ob, sb) = (blocks(&out), blocks(&data));
    assert_eq!(ob.len(), sb.len());
    assert_eq!(out[..1024], data[..1024]);
    assert_eq!(ob[0][..16], sb[0][..16]);
    assert_eq!(ob[1][..16], sb[1][..16]);
    assert!(ob[0][16..].starts_with(&cat(&[DH, b"Bonjour,", END, b"\x8a\x8a", DH, b"comment \xcbca va ?"])));
    let needle = b"cr\xc1eme br\xc3ul\xc2ee";
    assert!(ob[1].windows(needle.len()).any(|w| w == needle));
    assert_eq!(ob[2..], sb[2..]);
    let got = unit_texts(&out);
    assert_eq!(got[0], (0, "Bonjour,\ncomment \u{e7}a va ?".to_string()));
    assert_eq!(got[1], (1, "Caf\u{e9} cr\u{e8}me br\u{fb}l\u{e9}e".to_string()));
    // inline teletext colour codes survive as tokens; the code takes the space's cell
    let out = build_plain(&data, &[(2, "Alerte \u{27E6}x1\u{27E7}rouge \u{27E6}x2\u{27E7}maintenant")]).unwrap();
    let want = cat(&[DH, b"Alerte\x01rouge\x07maintenant", END]);
    assert!(blocks(&out)[2].windows(want.len()).any(|w| w == want.as_slice()), "{:?}", blocks(&out)[2]);
    // open-subtitle italics (0x80/0x81) are non-spacing tokens
    let out = build_plain(&data, &[(4, "Kursiv\u{27E6}x1\u{27E7} offener Text 5 $")]).unwrap();
    assert!(blocks(&out)[7][16..].starts_with(b"\x80Kursiv\x81 offener Text 5 $\x8f"));
}

#[test]
fn reflow_and_extension() {
    let data = stl();
    let long = "This translation is much longer than the original line and wraps";
    let out = build_plain(&data, &[(1, long)]).unwrap();
    let b = blocks(&out)[1];
    assert_eq!(b[13], 20, "one row became two: VP moves up one double-height row");
    let t = unit_texts(&out)[1].1.clone();
    let rows: Vec<&str> = t.split('\n').collect();
    assert!(rows.len() == 2 && rows.iter().all(|r| r.chars().count() <= 40), "{rows:?}");
    // a subtitle that needs more TF bytes gets an extension block; TNB follows
    let big = "\u{c7}a \u{e9}t\u{e9} tr\u{e8}s \u{e9}t\u{e9} \u{e7}a \u{e9}t\u{e9} tr\u{e8}s \u{e9}t\u{e9} \u{e7}a \u{e9}t\u{e9}\n\u{e7}a \u{e9}t\u{e9} tr\u{e8}s \u{e9}t\u{e9} \u{e7}a \u{e9}t\u{e9} tr\u{e8}s \u{e9}t\u{e9} \u{e7}a \u{e9}t\u{e9}";
    let out = build_plain(&data, &[(0, big)]).unwrap();
    let ob = blocks(&out);
    assert_eq!(ob.len(), nsubs() + 1);
    assert_eq!(&out[238..243], format!("{:05}", nsubs() + 1).as_bytes());
    assert!(ob[0][3] == 0x00 && ob[1][3] == 0xFF && ob[0][1..3] == ob[1][1..3]);
    assert_eq!(unit_texts(&out)[0], (0, big.to_string()));
    // the multi-block group shrinks back to one block when the text gets short
    let out = build_plain(&data, &[(3, "Short.")]).unwrap();
    assert_eq!(blocks(&out).len(), nsubs() - 1);
    assert_eq!(&out[238..243], format!("{:05}", nsubs() - 1).as_bytes());
    assert_eq!(unit_texts(&out)[3], (3, "Short.".to_string()));
}

#[test]
fn table_switch() {
    let plain = cat(&[
        &gsi_default(2),
        &t(1, &cat(&[DH, b"Hello,", END, b"\x8a\x8a", DH, b"friend", END])),
        &t(2, b"See you $5"),
    ]);
    let out = build_plain(&plain, &[(0, "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442},\n\u{434}\u{440}\u{443}\u{433}")]).unwrap();
    assert_eq!(&out[12..14], b"01");
    assert_eq!(
        unit_texts(&out),
        vec![(0, "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442},\n\u{434}\u{440}\u{443}\u{433}".to_string()), (1, "See you $5".to_string())]
    );
    assert_eq!(blocks(&out)[1], blocks(&plain)[1], "ASCII bytes are the same in 8859-5");
}

#[test]
fn cyrillic_quotes_679() {
    let plain = cat(&[&gsi_default(2), &t(1, &cat(&[DH, b"Hello, caf\xc2e", END])), &t(2, b"Closed today")]);
    let out = build_plain(&plain, &[(0, "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442} \u{2014} \u{ab}\u{43a}\u{430}\u{444}\u{435}\u{bb}"), (1, "\u{421}\u{435}\u{433}\u{43e}\u{434}\u{43d}\u{44f} \u{201e}\u{437}\u{430}\u{43a}\u{440}\u{44b}\u{442}\u{43e}\u{201c}")]).unwrap();
    assert_eq!(&out[12..14], b"01");
    assert_eq!(
        unit_texts(&out),
        vec![
            (0, "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442} - \"\u{43a}\u{430}\u{444}\u{435}\"".to_string()),
            (1, "\u{421}\u{435}\u{433}\u{43e}\u{434}\u{43d}\u{44f} \"\u{437}\u{430}\u{43a}\u{440}\u{44b}\u{442}\u{43e}\"".to_string())
        ]
    );
    // a Latin file keeps its typographic quotes; table 00 has no em dash, so it folds
    let lat = build_plain(&plain, &[(0, "\u{ab}Bonjour\u{bb} \u{2014} \u{201c}ami\u{201d}")]).unwrap();
    assert_eq!(&lat[12..14], b"00");
    assert_eq!(unit_texts(&lat)[0], (0, "\u{ab}Bonjour\u{bb} - \u{201c}ami\u{201d}".to_string()));
    // a real blocker: Cyrillic + CJK in one subtitle names the CJK, not the Cyrillic letters
    let msg = write_msg(build_plain(&plain, &[(0, "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442} \u{4f60}\u{597d}"), (1, "\u{421}\u{435}\u{433}\u{43e}\u{434}\u{43d}\u{44f}")]));
    assert!(msg.contains("subtitle 1") && msg.contains("'\u{4f60}'") && !msg.contains("'\u{41f}'"), "{msg}");
    assert!(!msg.contains("subtitle 2"), "{msg}");
}

#[test]
fn refusals() {
    let data = stl();
    let m = write_msg(build_plain(&data, &[(0, &"x".repeat(150))]));
    assert!(m.contains("subtitle 1") && m.contains("does not fit"), "{m}");
    let m = write_msg(build_plain(&data, &[(0, "\u{4f60}\u{597d}\u{4e16}\u{754c}")]));
    assert!(m.contains("subtitle 1"), "{m}");
    // Greek into a file whose other subtitles hold Latin accents no Greek table has
    let m = write_msg(build_plain(&data, &[(0, "\u{393}\u{3b5}\u{3b9}\u{3ac} \u{3c3}\u{3bf}\u{3c5}")]));
    assert!(m.contains("code table"), "{m}");
}

#[test]
fn language() {
    let data = stl();
    let out = build_localized(&data, &[], "fr-FR").unwrap();
    assert_eq!(&out[14..16], b"0F");
    assert_eq!(out[16..], data[16..]);
    assert_eq!(&build_localized(&data, &[], "nl-BE").unwrap()[14..16], b"2A");
    assert_eq!(&build_localized(&data, &[], "x-klingon").unwrap()[14..16], b"00");
    assert_eq!(language_code("ru"), 0x56);
    assert_eq!(language_code("ar-EG"), 0x7E);
}

#[test]
fn hostile() {
    for bad in [b"solid cube\nfacet normal 0 0 1\n".to_vec(), vec![0u8; 84], stl()[..1100].to_vec(), gsi(b"09", b"09", b'1', b"40", 0)] {
        assert!(matches!(Stl.extract(&bad, &opts(None)), Err(Error::Parse { .. }) | Err(Error::Limit(_))));
    }
    // the segment cap refuses
    let o = ExtractOptions { limits: Some(Limits { max_units: 3, ..Default::default() }), ..Default::default() };
    assert!(matches!(Stl.extract(&stl(), &o), Err(Error::Limit(_))));
    // a dangling diacritic and odd control bytes decode without crashing
    let odd = cat(&[&gsi_default(1), &t(1, b"ab\xc2\x8b\x9f\x7fend\xc8")]);
    assert_eq!(build_plain(&odd, &[]).unwrap(), odd);
    assert_eq!(ex(&odd).units.len(), 1);
    // a huge translation is refused at once, not re-wrapped per width
    let huge = "word ".repeat(20000);
    let t0 = std::time::Instant::now();
    let m = write_msg(build_plain(&stl(), &[(0, huge.trim())]));
    assert!(m.contains("does not fit"), "{m}");
    assert!(t0.elapsed().as_secs() < 2);
    // two short rows fit as they are
    let data = stl();
    let out = build_plain(&data, &[(1, "two short rows")]).unwrap();
    assert_eq!(unit_texts(&out)[1].1, "two short rows");
}

#[test]
fn pipeline() {
    // fill every unit with "Bon " + text, target fr: LC = French; blocks of comments and user data untouched
    let data = stl();
    let mut d = Stl.extract(&data, &opts(Some("fr"))).unwrap();
    let n = d.units.len();
    for k in 0..n {
        let mut t = vec![Inline::text("Bon ")];
        t.extend(d.units[k].segments[0].source.iter().cloned());
        d.units[k].segments[0].target = t;
    }
    let out = Stl.merge_with(&d, &MergeOptions::locos()).unwrap();
    assert!(n >= 5);
    assert_eq!(&out[14..16], b"0F");
    assert!(unit_texts(&out).iter().all(|(_, t)| t.starts_with("Bon")));
    assert_eq!(blocks(&out)[3], blocks(&data)[3]);
    assert_eq!(blocks(&out)[4], blocks(&data)[4]);
    // the same through the default merge: translated, so the language is stamped too
    let out2 = Stl.merge(&d).unwrap();
    assert_eq!(out, out2);
    // CJK: refused with its subtitle
    for k in 0..n {
        d.units[k].segments[0].target = vec![Inline::text("\u{4f60}\u{597d} ")];
    }
    let m = write_msg(Stl.merge(&d));
    assert!(m.contains("subtitle 1"), "{m}");
}

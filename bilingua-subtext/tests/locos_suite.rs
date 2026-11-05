//! The LocOS suite `tests/verify_subtitle_text_filter.py`, one test per LocOS test function (10 of 10), with the
//! same fixtures. `test_registry` and `test_surfaces` check the Filter registration and the library path the
//! API and CLI use (extract, translate, merge, refuse); the web and CLI plumbing itself is LocOS's.
mod common;
use bilingua_model::*;
use bilingua_subtext::{Sbv, Sub};
use common::*;

const SBV: &[u8] = b"0:00:00.599,0:00:04.160\n>> ALICE: Hi, my name is Alice.\n\n0:00:04.160,0:00:06.770\nLine one {0} and %s\nline two  \n\n0:00:06.770,0:00:08.000\n\n0:00:08.000,0:00:09.500\nCaf\xc3\xa9 time.\n";

const SV2: &[u8] = b"[INFORMATION]\r\n[TITLE]Demo Title\r\n[AUTHOR]Someone\r\n[END INFORMATION]\r\n[SUBTITLE]\r\n[COLF]&HFFFFFF,[STYLE]bd,[SIZE]18,[FONT]Arial\r\n00:00:01.00,00:00:04.00\r\nHello there.[br]How are you?\r\n\r\n00:00:05.50,00:00:07.25\r\nFine, thanks.\r\n\r\n";

const MDVD: &[u8] = b"{1}{1}23.976\n{DEFAULT}{C:$FF0000}{F:Arial}\n{25}{50}{y:i}Hello!|How are you?\n{60}{90}Plain {0} text\n{100}{}{Y:b}\n{120}{150}{c:$0000FF}Red|{y:b}Bold line\n";

fn s(v: &[u8]) -> String {
    String::from_utf8_lossy(v).into_owned()
}

#[test]
fn registry() {
    assert_eq!(Sbv.id(), "sbv");
    assert_eq!(Sub.id(), "sub");
    assert_eq!(Sbv.extensions(), &["sbv"]);
    assert_eq!(Sub.extensions(), &["sub"]);
    assert!(Sub.sniff(MDVD) && Sub.sniff(SV2) && !Sub.sniff(SBV));
    assert!(Sbv.sniff(SBV) && !Sbv.sniff(MDVD) && !Sbv.sniff(b"just words\n"));
}

#[test]
fn sbv() {
    assert_eq!(units("sbv", SBV), [">> ALICE: Hi, my name is Alice.", "Line one {0} and %s\nline two", "Café time."]);
    assert_identity("sbv", SBV);
    let out = build("sbv", SBV, &[(1, "Ligne un {0} et %s\n\n  ligne deux"), (2, "Pause café.")]);
    let exp = s(SBV).replace("Line one {0} and %s\nline two", "Ligne un {0} et %s\nligne deux").replace("Café time.", "Pause café.");
    assert_eq!(s(&out), exp);
    // CRLF and a UTF-8 BOM survive; a new line break uses the file's own newline.
    let mut crlf = b"\xef\xbb\xbf".to_vec();
    crlf.extend(s(SBV).replace('\n', "\r\n").into_bytes());
    assert_identity("sbv", &crlf);
    let out = build("sbv", &crlf, &[(0, "Salut\nmoi")]);
    assert!(out.starts_with(b"\xef\xbb\xbf0:00:00.599,0:00:04.160\r\nSalut\r\nmoi\r\n\r\n"), "{}", s(&out));
    assert!(out.ends_with(b"Caf\xc3\xa9 time.\r\n"));
}

#[test]
fn subviewer() {
    assert_eq!(units("sub", SV2), ["Hello there.\nHow are you?", "Fine, thanks."]);
    assert_identity("sub", SV2);
    let out = s(&build("sub", SV2, &[(0, "Bonjour.\nCa va ?"), (1, "Bien, merci.\nEt toi ?")]));
    assert!(out.contains("00:00:01.00,00:00:04.00\r\nBonjour.[br]Ca va ?\r\n"), "{out}");
    assert!(out.contains("Bien, merci.[br]Et toi ?\r\n"), "{out}");
    let head = &s(SV2)[..s(SV2).find("00:00:01.00").unwrap()];
    assert!(out.starts_with(head), "header must be untouched");
}

#[test]
fn microdvd() {
    let doc = extract("sub", MDVD).unwrap();
    assert_eq!(units("sub", MDVD), ["\u{27E6}g1\u{27E7}Hello!\u{27E6}/g1\u{27E7}\nHow are you?", "Plain {0} text", "\u{27E6}g1\u{27E7}Red\u{27E6}/g1\u{27E7}\n\u{27E6}g2\u{27E7}Bold line\u{27E6}/g2\u{27E7}"]);
    let (_, m0) = bilingua_model::profile::tokens::to_locos_text(&doc.units[0].source());
    assert_eq!(m0["1"].open.as_deref(), Some("{y:i}"));
    assert_eq!(doc.units[0].codes["1"].open.as_deref(), Some("{y:i}"));
    assert_eq!(doc.units[2].codes["2"].open.as_deref(), Some("{y:b}"));
    assert_identity("sub", MDVD);
    let out = s(&build(
        "sub",
        MDVD,
        &[
            (0, "\u{27E6}g1\u{27E7}Salut !\u{27E6}/g1\u{27E7}\nCa va ?"),
            (1, "Texte {0} simple"),
            // the token brings back its code; an invented token is dropped
            (2, "\u{27E6}g1\u{27E7}Rouge\u{27E6}/g1\u{27E7}\n\u{27E6}g2\u{27E7}Ligne grasse\u{27E6}/g2\u{27E7} \u{27E6}x9\u{27E7}"),
        ],
    ));
    let lines: Vec<&str> = out.split('\n').collect();
    assert_eq!(lines[0], "{1}{1}23.976");
    assert!(lines[1].starts_with("{DEFAULT}"));
    assert_eq!(lines[2], "{25}{50}{y:i}Salut !|Ca va ?");
    assert_eq!(lines[3], "{60}{90}Texte {0} simple");
    assert_eq!(lines[4], "{100}{}{Y:b}");
    assert_eq!(lines[5], "{120}{150}{c:$0000FF}Rouge|{y:b}Ligne grasse");
    assert!(!out.contains('\u{27E6}'));
    // Gate: spaces around | are layout; the unit is trimmed per line and identity is exact.
    let spaced = b"{10}{20}{0} items | %s\r\n";
    assert_eq!(units("sub", spaced), ["{0} items\n%s"]);
    assert_eq!(build("sub", spaced, &[(0, "{0} items\n%s")]), spaced);
    assert_eq!(build("sub", spaced, &[(0, "{0} Artikel\n%s")]), b"{10}{20}{0} Artikel|%s\r\n");
}

#[test]
fn microdvd_style_pairs_680() {
    let src: &[u8] = b"{1}{1}25\n{10}{20}{y:i}Hello!|How are you?\n{30}{40}Plain|{y:i}{c:$FF}styled words {Y:u}x\n{50}{60}{Y:b}{y:i}Whole cue|second\n{70}{80}{y:i}One line only\n{90}{99}End{y:b}|next\n";
    let us = units("sub", src);
    assert_eq!(
        us,
        [
            "\u{27E6}g1\u{27E7}Hello!\u{27E6}/g1\u{27E7}\nHow are you?",
            "Plain\n\u{27E6}g1\u{27E7}styled words \u{27E6}x2\u{27E7}x\u{27E6}/g1\u{27E7}",
            "\u{27E6}g1\u{27E7}Whole cue\u{27E6}/g1\u{27E7}\nsecond",
            "One line only",
            "End\u{27E6}x1\u{27E7}\nnext",
        ]
    );
    assert_identity("sub", src);
    let line = |tr: &[(usize, &str)], n: usize| s(&build("sub", src, tr)).split('\n').nth(n).unwrap().to_string();
    assert_eq!(line(&[(0, "Ca va ?\n\u{27E6}g1\u{27E7}Salut !\u{27E6}/g1\u{27E7}")], 1), "{10}{20}Ca va ?|{y:i}Salut !");
    assert_eq!(line(&[(0, "Oui \u{27E6}g1\u{27E7}Salut !\u{27E6}/g1\u{27E7}\nCa va ?")], 1), "{10}{20}{y:i}Oui Salut !|Ca va ?");
    assert_eq!(line(&[(1, "\u{27E6}g1\u{27E7}mots\nstyles \u{27E6}x2\u{27E7}x\u{27E6}/g1\u{27E7}\nSimple")], 2), "{30}{40}{y:i}{c:$FF}mots|{y:i}{c:$FF}styles {Y:u}x|Simple");
    assert_eq!(line(&[(2, "Zweite\n\u{27E6}g1\u{27E7}\u{27E6}/g1\u{27E7}ganz")], 3), "{50}{60}{Y:b}Zweite|ganz");
    for (bad, want) in [
        ("Ca va ?\nSalut !", "{10}{20}Ca va ?|Salut !"),
        ("\u{27E6}/g1\u{27E7}Salut\u{27E6}g1\u{27E7} !", "{10}{20}Salut !"),
        ("\u{27E6}g1\u{27E7}a\u{27E6}/g1\u{27E7} \u{27E6}g1\u{27E7}b\u{27E6}/g1\u{27E7}", "{10}{20}{y:i}a b"),
        ("\u{27E6}x1\u{27E7}Salut !\nCa va ?", "{10}{20}Salut !|Ca va ?"),
        ("Salut\u{27E6}g1\u{27E7} !", "{10}{20}Salut !"),
    ] {
        assert_eq!(line(&[(0, bad)], 1), want, "{bad:?}");
    }
    assert_eq!(line(&[(1, "\u{27E6}g1\u{27E7}a \u{27E6}x2\u{27E7}b\nc\u{27E6}/g1\u{27E7}")], 2), "{30}{40}{y:i}{c:$FF}a {Y:u}b|{y:i}{c:$FF}c");
}

#[test]
fn sniff_refusals() {
    let mut ambiguous = MDVD.to_vec();
    ambiguous.extend(b"\n00:00:01.00,00:00:04.00\nHi\n");
    let mut vob = vec![0, 0, 1, 0xBA];
    vob.extend([0x44; 64]);
    for (name, data) in [
        ("ambiguous", ambiguous),
        ("subviewer 1", b"[TITLE]\nx\n[SUBTITLE]\n[00:00:01]\nHello\n[00:00:04]\n".to_vec()),
        ("vobsub", vob),
        ("junk", b"just some words\nnothing timed\n".to_vec()),
    ] {
        match extract("sub", &data) {
            Err(Error::Parse { msg, .. }) => assert!(!msg.is_empty() && !msg.contains('\u{2014}'), "{name}: {msg}"),
            other => panic!("{name} .sub must be refused: {:?}", other.map(|d| d.units.len())),
        }
    }
    let Err(Error::Parse { msg, .. }) = extract("sub", &{
        let mut a = MDVD.to_vec();
        a.extend(b"00:00:01.00,00:00:04.00\nHi\n");
        a
    }) else {
        panic!()
    };
    assert!(msg.contains("ambiguous"));
}

#[test]
fn encodings() {
    let cp1252: Vec<u8> = {
        let mut v = b"0:00:01.000,0:00:02.000\ncaf".to_vec();
        v.extend([0xE9, b' ', b'c', b'r', 0xE8, b'm', b'e', b'\n']);
        v
    };
    let doc = extract("sbv", &cp1252).unwrap();
    assert_eq!(plain_text(&doc.units[0].source()), "café crème");
    assert!(doc.skeleton_encoding().starts_with("windows-1252"));
    assert_eq!(build("sbv", &cp1252, &[(0, "coffee")]), b"0:00:01.000,0:00:02.000\ncoffee\n");
    // UTF-16
    let mut u16: Vec<u8> = vec![0xFF, 0xFE];
    for c in "{1}{2}naïve|ligne\n".encode_utf16() {
        u16.extend(c.to_le_bytes());
    }
    assert_eq!(units("sub", &u16), ["naïve\nligne"]);
    assert_identity("sub", &u16);
    assert_eq!(build("sub", &u16, &[(0, "naïf\nligne")]), "{1}{2}naïf|ligne\n".as_bytes());
    // kept encoding by default
    let kept = try_build_with("sub", &u16, &[(0, "naïf\nligne".into())], &MergeOptions::default()).unwrap();
    assert!(kept.starts_with(&[0xFF, 0xFE]));
    // undecodable
    assert!(matches!(extract("sbv", b"0:00:01.000,0:00:02.000\ncaf\x81\n"), Err(Error::Encoding(_))));
}

trait Enc {
    fn skeleton_encoding(&self) -> String;
}
impl Enc for Document {
    fn skeleton_encoding(&self) -> String {
        match &self.skeleton {
            Skeleton::Text { encoding, .. } => encoding.clone(),
            _ => String::new(),
        }
    }
}

#[test]
fn caps() {
    let lim = |l: Limits| opts().with_limits(l);
    let many: Vec<u8> = (10..30).flat_map(|i| format!("{{{i}}}{{{}}}line {i}\n", i + 1).into_bytes()).collect();
    let sbv: Vec<u8> = (0..20).flat_map(|i| format!("0:00:01.000,0:00:02.000\nx{i}\n\n").into_bytes()).collect();
    let l = Limits::new().with_max_units(5);
    assert!(matches!(Sub.extract(&many, &lim(l)), Err(Error::Limit(_))));
    assert!(matches!(Sbv.extract(&sbv, &lim(l)), Err(Error::Limit(_))));
    let mut long = b"{1}{2}".to_vec();
    long.extend([b'a'; 80]);
    long.push(b'\n');
    assert!(matches!(Sub.extract(&long, &lim(Limits::new().with_max_unit_chars(50))), Err(Error::Limit(_))));
}

#[test]
fn hostile() {
    // Unbalanced braces, huge frame numbers, stray control codes: no crash, no eval.
    let mut data = b"{9999999999}{1}{{{y:i}x}}|\n{5}{6}{zz}\n{7}{8}".to_vec();
    data.extend(b"{".repeat(5000));
    data.push(b'\n');
    let doc = extract("sub", &data).unwrap();
    assert_eq!(Sub.merge(&doc).unwrap(), data);
    assert!(!doc.units.is_empty());
}

#[test]
fn surfaces() {
    // upload -> translate -> download, as the API and CLI do it: the timing and headers stay, nothing leaks.
    for (fmt, data, keep) in [
        ("sbv", SBV, &b"0:00:04.160,0:00:06.770\n"[..]),
        ("sub", MDVD, &b"{1}{1}23.976\n{DEFAULT}"[..]),
        ("sub", SV2, &b"[INFORMATION]\r\n[TITLE]Demo Title\r\n"[..]),
    ] {
        let mut doc = extract(fmt, data).unwrap();
        for u in &mut doc.units {
            let mut t = vec![Inline::text("fr: ")];
            t.extend(u.source());
            u.segments[0].target = t;
        }
        let out = filter(fmt).merge_with(&doc, &bilingua_model::profile::locos().merge_options("xml")).unwrap();
        assert!(out.windows(keep.len()).any(|w| w == keep), "{fmt}");
        assert!(!s(&out).contains('\u{27E6}'));
        assert_ne!(out, data);
    }
    let mut a = MDVD.to_vec();
    a.extend(b"00:00:01.00,00:00:04.00\nHi\n");
    assert!(matches!(extract("sub", &a), Err(Error::Parse { msg, .. }) if msg.contains("ambiguous")));
}

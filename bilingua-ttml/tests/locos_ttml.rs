//! Port of the Localization OS suite `tests/verify_ttml_filter.py`: 12 of its 13 cases. The other one is
//! `test_registered` (the LocOS registry and extension list); the facade contract test covers its Bilingua
//! counterpart. Same inputs, same expected bytes. The fixture is the suite's own `TT` constant
//! (`tests/data/locos_suite_tt.ttml`); the other inputs are written out below as the suite writes them.

mod common;

use bilingua_model::*;
use bilingua_ttml::Ttml;
use common::*;

const TT: &[u8] = include_bytes!("data/locos_suite_tt.ttml");

fn tt() -> String {
    String::from_utf8(TT.to_vec()).unwrap()
}

#[test]
fn identity_round_trip() {
    assert!(TT.starts_with(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- produced by hand -->\n<tt "));
    let bom = [b"\xef\xbb\xbf".as_slice(), TT].concat();
    let crlf = tt().replace('\n', "\r\n").into_bytes();
    for data in [TT.to_vec(), bom, crlf] {
        assert_identity(&data);
    }
}

#[test]
fn units_and_tokens() {
    let got = units(TT);
    assert_eq!(
        got,
        vec![
            (0, "Hello & welcome⟦x1⟧to the ⟦g2⟧show⟦/g2⟧.".to_string()),
            (1, "Second line continues here".to_string()),
            (3, "Press {0} for ⟦g1⟧%1$s⟦/g1⟧⟦x2⟧ now".to_string()),
        ]
    );
}

#[test]
fn translated_round_trip() {
    let out = build_str(
        TT,
        &[
            (0, "Hola & bienvenidos⟦x1⟧al ⟦g2⟧programa⟦/g2⟧ <ya>."),
            (1, "Segunda línea\nsigue aquí"),
            (3, "Pulse {0} para ⟦g1⟧%1$s⟦/g1⟧⟦x2⟧ ahora"),
        ],
        None,
    );
    well_formed(out.as_bytes());
    assert!(out.contains("<p begin=\"00:00:01.000\" end=\"00:00:04.000\" style=\"s1\">Hola &amp; bienvenidos<br/>"));
    assert!(out.contains("<span tts:fontStyle=\"italic\">programa</span> &lt;ya&gt;.</p>"));
    assert!(out.contains("Segunda línea<br/>sigue aquí"), "typed newline becomes <br/>");
    assert!(out.contains("<set begin=\"1s\" tts:color=\"red\"/> ahora</p>"));
    assert!(out.contains("<p begin=\"8s\" end=\"9s\"></p>"));
    // everything before <body> is byte-identical, and nothing is stamped without a locale
    let src = tt();
    assert_eq!(out.split("<body>").next(), src.split("<body>").next());
    assert!(out.contains("xml:lang=\"en\">\n  <head>"));
}

#[test]
fn tolerant_tokens() {
    let mut d = extract(TT);
    let u = &mut d.units[0];
    // an unknown code, a repeated code, a stray close and an unclosed span, built straight into the target
    u.segments[0].target = vec![
        Inline::text("A "),
        Inline::tag("9", TagKind::Standalone, "?"),
        Inline::text("B "),
        Inline::tag("2", TagKind::Open, "<span>"),
        Inline::text("C "),
        Inline::tag("1", TagKind::Standalone, "<br/>"),
        Inline::tag("1", TagKind::Standalone, "<br/>"),
        Inline::text("D "),
        Inline::tag("7", TagKind::Close, "</x>"),
        Inline::text("E"),
    ];
    let out = String::from_utf8(F.merge(&d).unwrap()).unwrap();
    well_formed(out.as_bytes());
    assert!(!out.contains('⟦'));
    assert!(out.contains("A B <span tts:fontStyle=\"italic\">C <br/>D E</span></p>"), "{out}");
}

#[test]
fn localized_lang() {
    let out = build_str(TT, &[(3, "Pulse {0} ⟦g1⟧%1$s⟦/g1⟧⟦x2⟧")], Some("es_ES"));
    assert!(out.contains("\"http://www.w3.org/ns/ttml#metadata\" xml:lang=\"es-ES\">"), "{out}");
    assert!(out.contains("<div xml:lang=\"es-ES\">"));
    assert!(out.contains("<span xml:lang=\"es-ES\" tts:fontWeight=\"bold\">"));
    // an element in another language stays; a <tt> without xml:lang gets one
    let data2 = tt().replace(" xml:lang=\"en\">\n  <head>", ">\n  <head>").replace("<div xml:lang=\"en\">", "<div xml:lang=\"fr\">");
    let out2 = build_str(data2.as_bytes(), &[], Some("de"));
    assert!(out2.contains("<tt xml:lang=\"de\" xmlns=") && out2.contains("<div xml:lang=\"fr\">"), "{out2}");
    assert_eq!(build(TT, &[], None), TT);
}

#[test]
fn prefixed_and_dfxp_namespace() {
    let data = b"<tt:tt xmlns:tt=\"http://www.w3.org/2006/10/ttaf1\" xml:lang=\"en\"><tt:body><tt:div>\
<tt:p begin=\"1s\" end=\"2s\">One<tt:br/>Two</tt:p>\
<x:p xmlns:x=\"urn:other\">not a caption</x:p>\
</tt:div></tt:body></tt:tt>";
    assert_eq!(texts(data), ["One⟦x1⟧Two"]);
    let out = build_str(data, &[(0, "Uno\nDos")], None);
    assert!(out.contains("<tt:p begin=\"1s\" end=\"2s\">Uno<tt:br/>Dos</tt:p>"), "{out}");
    assert!(out.contains("not a caption"));
}

#[test]
fn encodings() {
    let utf16s = tt().replace("encoding=\"UTF-8\"", "encoding=\"UTF-16\"");
    let mut utf16 = vec![0xFF, 0xFE];
    for u in utf16s.encode_utf16() {
        utf16.extend_from_slice(&u.to_le_bytes());
    }
    let d = F.extract(&utf16, &opts()).unwrap();
    match &d.skeleton {
        Skeleton::Text { encoding, .. } => assert_eq!(enc::base(encoding), "utf-16le"),
        _ => panic!(),
    }
    assert_eq!(show(&d.units[1]), "Second line continues here");
    assert_eq!(F.merge(&d).unwrap(), utf16, "untouched merge keeps UTF-16");
    // the LocOS export: UTF-8 with the declaration rewritten
    let out = build(&utf16, &[(1, "x")], Some("de"));
    assert!(out.starts_with(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>"), "{:?}", &out[..60]);
    well_formed(&out);
    // the Bilingua default keeps UTF-16 with its BOM
    let mut d2 = F.extract(&utf16, &opts()).unwrap();
    set(&mut d2, 1, "x");
    let kept = F.merge(&d2).unwrap();
    assert!(kept.starts_with(&[0xFF, 0xFE]));
    well_formed(&kept);
}

fn refused(data: &[u8]) {
    let e = F.extract(data, &opts());
    assert!(e.is_err(), "must be refused: {:?}", String::from_utf8_lossy(&data[..data.len().min(80)]));
}

#[test]
fn hostile() {
    refused(b"<?xml version=\"1.0\"?><!DOCTYPE tt [<!ENTITY x \"boom\">]><tt xmlns=\"http://www.w3.org/ns/ttml\"><body><div><p>&x;</p></div></body></tt>");
    refused(b"<?xml version=\"1.0\"?><!DOCTYPE tt SYSTEM \"file:///etc/passwd\"><tt xmlns=\"http://www.w3.org/ns/ttml\"><body/></tt>");
    refused(b"<tt xmlns=\"http://www.w3.org/ns/ttml\"><body><div><p>&nbsp;</p></div></body></tt>");
    refused(b"<html><body><p>hi</p></body></html>");
    refused(b"<tt xmlns='http://www.w3.org/ns/ttml'><body><p>unclosed</body></tt>");
    refused(b"\x00\x01\x02 not xml at all \x00");
    // MAX_DOCUMENT_SEGMENTS = 5
    let l = Limits { max_units: 5, ..Limits::default() };
    let many: String = (0..8).map(|i| format!("<p>line {i}</p>")).collect();
    let doc = format!("<tt xmlns=\"http://www.w3.org/ns/ttml\"><body><div>{many}</div></body></tt>");
    assert!(matches!(F.extract_with(doc.as_bytes(), &opts(), &l), Err(Error::Limit(_))));
    let doc = format!("<tt xmlns=\"http://www.w3.org/ns/ttml\"><body><div><p>{}</p></div></body></tt>", "<br/>".repeat(300));
    assert!(matches!(F.extract_with(doc.as_bytes(), &opts(), &l), Err(Error::Limit(_))));
}

#[test]
fn comment_and_cdata_in_p() {
    let data = b"<tt xmlns=\"http://www.w3.org/ns/ttml\" xml:lang=\"en\"><body><div><p>Before<!-- note --> <![CDATA[a<b]]> after</p></div></body></tt>";
    assert_eq!(texts(data), ["Before⟦x1⟧ a<b after"]);
    assert_identity(data);
    let out = build_str(data, &[(0, "Antes⟦x1⟧ a<b después")], None);
    assert!(out.contains("<p>Antes<!-- note --> a&lt;b después</p>"), "{out}");
}

#[test]
fn gate_caption_scope_space_and_head_lang() {
    let data = b"<tt xmlns=\"http://www.w3.org/ns/ttml\" xmlns:ttm=\"http://www.w3.org/ns/ttml#metadata\" xml:lang=\"en\"><head><metadata><p>head note</p>\
<ttm:desc xml:lang=\"en\">Demo</ttm:desc></metadata></head>\
<body><div xml:space=\"preserve\">\
<p>Outer<metadata><p>inner</p></metadata>  two  spaces</p>\
</div></body></tt>";
    assert_eq!(units(data), vec![(0, "Outer⟦x1⟧  two  spaces".to_string())]);
    assert_identity(data);
    let out = build_str(data, &[(0, "Fuera⟦x1⟧ dos")], Some("es"));
    well_formed(out.as_bytes());
    assert!(out.contains("<p>Fuera<metadata><p>inner</p></metadata> dos</p>"), "{out}");
    assert!(out.contains("<p>head note</p><ttm:desc xml:lang=\"en\">Demo</ttm:desc>"), "{out}");
    assert!(out.starts_with("<tt xmlns=\"http://www.w3.org/ns/ttml\" xmlns:ttm=\"http://www.w3.org/ns/ttml#metadata\" xml:lang=\"es\">"));
}

#[test]
fn rtl_direction_741() {
    // (a) the soak repro shape: no styling namespace bound at all
    let base = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<tt xmlns=\"http://www.w3.org/ns/ttml\" xml:lang=\"en\">\n  <body>\n    <div>\n\
      <p begin=\"1s\" end=\"3s\">Welcome back {0}!<br/>Line two</p>\n\
      <p begin=\"4s\" end=\"6s\">Kept in English</p>\n\
      <p begin=\"7s\" end=\"9s\">Third</p>\n    </div>\n  </body>\n</tt>\n";
    let b = base.as_bytes();
    let texts: [(usize, &str); 2] = [(0, "أهلاً {0}، مرحباً بك مجدداً!⟦x1⟧الصف الثاني"), (2, "الثالث")];
    let s = build_str(b, &texts, Some("ar-SA"));
    assert!(
        s.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<tt xmlns:tts=\"http://www.w3.org/ns/ttml#styling\" xmlns=\"http://www.w3.org/ns/ttml\" xml:lang=\"ar-SA\">"),
        "{s}"
    );
    assert!(s.contains("<p tts:direction=\"rtl\" begin=\"1s\" end=\"3s\">أهلاً"), "{s}");
    assert!(s.contains("<p begin=\"4s\" end=\"6s\">Kept in English</p>"), "untranslated caption keeps its ltr level");
    assert!(s.contains("<p tts:direction=\"rtl\" begin=\"7s\" end=\"9s\">الثالث</p>"));
    well_formed(s.as_bytes());
    let heb = build_str(b, &[(0, "שלום {0}!⟦x1⟧שורה")], Some("he-IL"));
    assert!(heb.contains("<p tts:direction=\"rtl\" begin=\"1s\" end=\"3s\">שלום {0}!<br/>שורה</p>"), "{heb}");
    let fr = build_str(b, &[(0, "Bon retour {0} !⟦x1⟧Ligne deux")], Some("fr-FR"));
    assert!(!fr.contains("direction") && !fr.contains("#styling"), "{fr}");
    assert!(!build_str(b, &texts, None).contains("direction"));
    assert_eq!(build(b, &[], None), b);
    let noop = build_str(b, &[], Some("ar-SA"));
    assert_eq!(noop, base.replace("xml:lang=\"en\"", "xml:lang=\"ar-SA\""));

    // (b) tts prefix already bound (TTML2): reused; explicit ltr on a translated <p> flipped; ltr on an untouched
    // <p>, span embeddings, region writingMode and textAlign kept
    let doc = b"<tt xmlns=\"http://www.w3.org/ns/ttml\" xmlns:s=\"http://www.w3.org/ns/ttml#styling\" xml:lang=\"en\"><head><layout><region xml:id=\"r1\" s:writingMode=\"lrtb\" s:textAlign=\"start\"/></layout></head><body region=\"r1\"><div>\
<p s:direction = 'ltr' begin=\"1s\" end=\"2s\">Open <span s:unicodeBidi=\"embed\" s:direction=\"ltr\">a.txt</span> now</p>\
<p s:direction=\"ltr\" begin=\"3s\" end=\"4s\">Same</p>\
<p begin=\"5s\" end=\"6s\" s:direction=\"rtl\">Already</p>\
</div></body></tt>";
    let s = build_str(doc, &[(0, "افتح ⟦g1⟧a.txt⟦/g1⟧ الآن"), (2, "سابقاً")], Some("ar"));
    assert!(s.contains("<p s:direction = 'rtl' begin=\"1s\""), "{s}");
    assert!(s.contains("<span s:unicodeBidi=\"embed\" s:direction=\"ltr\">a.txt</span>"));
    assert!(s.contains("<p s:direction=\"ltr\" begin=\"3s\" end=\"4s\">Same</p>"));
    assert!(s.contains("<p begin=\"5s\" end=\"6s\" s:direction=\"rtl\">سابقاً</p>"));
    assert!(s.contains("s:writingMode=\"lrtb\" s:textAlign=\"start\"") && !s.contains("xmlns:tts"));

    // (c) legacy DFXP namespace with a prefix that is NOT tts, bound only on <body>
    let dfxp = b"<tt xmlns=\"http://www.w3.org/2006/10/ttaf1\" xml:lang=\"en\"><body xmlns:st=\"http://www.w3.org/2006/10/ttaf1#styling\"><div><p begin=\"1s\" end=\"2s\">Hi</p></div></body></tt>";
    let s = build_str(dfxp, &[(0, "مرحبا")], Some("ar"));
    assert!(s.contains("<p st:direction=\"rtl\" begin=\"1s\"") && !s.contains("xmlns:tts"), "{s}");

    // (d) prefix "tts" already taken by another namespace: a fresh prefix, no collision
    let clash = b"<tt xmlns=\"http://www.w3.org/2006/10/ttaf1\" xmlns:tts=\"urn:not-styling\" xml:lang=\"en\"><body><div><p tts:x=\"1\" begin=\"1s\" end=\"2s\">Hi</p></div></body></tt>";
    let s = build_str(clash, &[(0, "שלום")], Some("he"));
    assert!(s.contains("xmlns:tts1=\"http://www.w3.org/2006/10/ttaf1#style\""), "{s}");
    assert!(s.contains("<p tts1:direction=\"rtl\" tts:x=\"1\""), "{s}");
}

#[test]
fn gate_rtl_dfxp_style_ns_741() {
    // the DFXP CR defines the TT Style namespace as ttaf1#style: its prefix is reused and an explicit ltr flipped
    let doc = b"<tt xmlns=\"http://www.w3.org/2006/10/ttaf1\" xmlns:tts=\"http://www.w3.org/2006/10/ttaf1#style\" xml:lang=\"en\"><body><div>\
<p begin=\"1s\" end=\"2s\" tts:direction=\"ltr\" tts:color=\"white\">Hi!</p>\
<p begin=\"3s\" end=\"4s\">Bye</p></div></body></tt>";
    let s = build_str(doc, &[(0, "مرحبا!"), (1, "وداعا")], Some("ar"));
    assert_eq!(s.matches("xmlns:").count(), 1, "{s}");
    assert!(!s.contains("#styling"));
    assert!(s.contains("<p begin=\"1s\" end=\"2s\" tts:direction=\"rtl\" tts:color=\"white\">"), "{s}");
    assert!(s.contains("<p tts:direction=\"rtl\" begin=\"3s\" end=\"4s\">"), "{s}");
    let old = String::from_utf8(doc.to_vec()).unwrap().replace("2006/10", "2006/04");
    let out = build_str(old.as_bytes(), &[(1, "וַדָּאִי")], Some("he"));
    assert!(out.contains("<p tts:direction=\"rtl\" begin=\"3s\"") && out.matches("xmlns:").count() == 1, "{out}");

    // prefixed TTML elements (no default namespace), xml:space preserve, CRLF line ends
    let pref = b"<tt:tt xmlns:tt=\"http://www.w3.org/ns/ttml\" xml:lang=\"en\">\r\n<tt:body><tt:div>\r\n\
<tt:p xml:space=\"preserve\" begin=\"1s\" end=\"2s\">  Two  spaces </tt:p>\r\n\
</tt:div></tt:body></tt:tt>\r\n";
    let out = build_str(pref, &[(0, "שתי  רווחים")], Some("he-IL")); // unit text is trimmed
    assert!(out.starts_with("<tt:tt xmlns:tts=\"http://www.w3.org/ns/ttml#styling\" xmlns:tt=\"http://www.w3.org/ns/ttml\""), "{out}");
    assert!(out.contains("<tt:p tts:direction=\"rtl\" xml:space=\"preserve\" begin=\"1s\" end=\"2s\">  שתי  רווחים </tt:p>\r\n"), "{out}");
    assert_eq!(out.matches("\r\n").count(), String::from_utf8_lossy(pref).matches("\r\n").count());

    // UTF-16 with a BOM: exported as UTF-8 (declaration rewritten), direction present
    let src = "<?xml version=\"1.0\" encoding=\"UTF-16\"?>\n<tt xmlns=\"http://www.w3.org/ns/ttml\" xml:lang=\"en\"><body><div><p begin=\"1s\" end=\"2s\">Hi</p></div></body></tt>";
    let mut u16 = vec![0xFF, 0xFE];
    for u in src.encode_utf16() {
        u16.extend_from_slice(&u.to_le_bytes());
    }
    let out = build(&u16, &[(0, "سلام")], Some("fa-IR"));
    assert!(out.starts_with(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    let dec = String::from_utf8(out.clone()).unwrap();
    assert!(dec.contains("<p tts:direction=\"rtl\" begin=\"1s\" end=\"2s\">سلام</p>"), "{dec}");
    let _ = Ttml.extract(&out, &opts()).unwrap();
}

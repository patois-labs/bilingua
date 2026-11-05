//! The LocOS suite `tests/verify_quark_tags_filter.py` ported case by case (the registry, API and upload-list
//! tests are LocOS web surfaces and have no counterpart in a parser crate).
mod common;
use bilingua_model::encoding_rs;
use bilingua_model::*;
use bilingua_xtg::Xtg;
use common::*;

const XTG: &str = concat!(
    "<v19.00><e9>\r\n",
    "@Normal=[S\"\",\"\"]<*L*h\"Standard\"f\"Helvetica\"z12>\r\n",
    "@Heading=[S\"\",\"Body\"]<*C><B>\r\n",
    "@Font set 1=[F]<f\"Arial\">\r\n",
    "~Bookmark=<BookmarkRef\"Intro\">\r\n",
    "@Heading:<*C>Getting started\r\n",
    "@Body:Hello <B>bold<P> world<\\n>second line\r\n",
    "Mail us <\\@> once, 3 <\\<> 4 <\\>> 2, path C:<\\\\>x<\\i>.\r\n",
    "<f\"Arial\"><z10>Leading tags stay outside<f$><z$>\r\n",
    "@$:<XI,\"Entry\",\"\",,,,,\"\">Indexed <A(3,\"HYP\\\"B\",\\#002\\#000)[2]>",
    "link<A(3,\"HYPB\",)[0]> text.\r\n",
    "\r\n",
    "<\\#13><*R>\r\n",
    "@:Rubi <A(\\#68Base\\#36<@><z6>\\#9Rubi\\#132,50,C,A,0,2,B,T)> end.\r\n",
    "<*t(36,0,\"1 \")>Name<\\t>Value <c\"Red\">\u{fc}ber\r\n"
);

const EXPECTED: [(&str, &str); 7] = [
    ("Getting started", "Heading"),
    ("Hello \u{27E6}g1\u{27E7}bold\u{27E6}/g1\u{27E7} world\u{27E6}x2\u{27E7}second line", "Body"),
    ("Mail us @ once, 3 < 4 > 2, path C:\\x\u{27E6}x1\u{27E7}.", "Body"),
    ("Leading tags stay outside", "Body"),
    ("Indexed \u{27E6}x1\u{27E7}link\u{27E6}x2\u{27E7} text.", "Normal"),
    ("Rubi \u{27E6}x1\u{27E7} end.", ""),
    ("Name\u{27E6}x1\u{27E7}Value \u{27E6}x2\u{27E7}\u{fc}ber", ""),
];

fn t(s: &str) -> String {
    s.replace("<<", "\u{27E6}").replace(">>", "\u{27E7}")
}

#[test]
fn registered() {
    assert_eq!(Xtg.id(), "xtg");
    assert!(Xtg.extensions().contains(&"xtg") && Xtg.extensions().contains(&"tag"));
    assert!(!Xtg.extensions().contains(&"txt"), ".txt stays plain text");
}

#[test]
fn units_and_identity() {
    let b = XTG.as_bytes();
    let got = units(b);
    let want: Vec<(String, String)> = EXPECTED.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    assert_eq!(got, want);
    let doc = extract(b).unwrap();
    assert!(matches!(&doc.skeleton, Skeleton::Text { encoding, .. } if encoding == "utf-8"));
    let map = |i: usize| profile::tokens::to_locos_text(&doc.units[i].source()).1;
    let m1 = map(1);
    assert_eq!((m1["1"].open.as_deref(), m1["1"].close.as_deref(), m1["2"].open.as_deref()), (Some("<B>"), Some("<P>"), Some("<\\n>")));
    let m4 = map(4);
    assert_eq!(m4["1"].open.as_deref(), Some("<A(3,\"HYP\\\"B\",\\#002\\#000)[2]>"));
    assert!(map(5)["1"].open.as_deref().unwrap().starts_with("<A(\\#68Base"));
    let m6 = map(6);
    assert_eq!((m6["1"].open.as_deref(), m6["2"].open.as_deref()), (Some("<\\t>"), Some("<c\"Red\">")));
    assert_eq!(Xtg.merge(&doc).unwrap(), b);
    assert_identity(b);
    // LF and CR (classic Mac) line ends, and a UTF-8 BOM, are kept
    for nl in ["\n", "\r"] {
        let b = XTG.replace("\r\n", nl).into_bytes();
        assert_eq!(units(&b), want);
        assert_identity(&b);
    }
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend_from_slice(b);
    assert_eq!(units(&bom).len(), EXPECTED.len());
    let out = build(&bom, &[(0, "Erste Schritte")]);
    assert!(out.starts_with(b"\xEF\xBB\xBF<v19.00><e9>"));
    assert!(String::from_utf8(out).unwrap().contains("@Heading:<*C>Erste Schritte\r\n"));
    // a header on the same line as the first paragraph
    assert_eq!(units(b"<v15.00><e9>@Normal:One line file<\\n>\n"), vec![("One line file".to_string(), "Normal".to_string())]);
    // a paragraph with no letters or digits is not a unit
    let r = units(b"<v1><e9>\n@Normal:Text\n<B>--- ...<P>\n");
    assert_eq!(r, vec![("Text".to_string(), "Normal".to_string())]);
}

#[test]
fn translated_round_trip() {
    let tr: [(usize, String); 5] = [
        (0, "Erste Schritte".into()),
        (1, t("Hallo <<g1>>fett<</g1>> Welt<<x2>>zweite Zeile")),
        (2, t("Mail an uns @ einmal, 3 < 4, Pfad C:\\y<<x1>>.")),
        (4, t("Indiziert <<x2>>ohne Link<<x9>> <<x2>>Text.")),
        (6, "Name\nWert".into()),
    ];
    let m: Vec<(usize, &str)> = tr.iter().map(|(i, s)| (*i, s.as_str())).collect();
    let out = build_s(XTG.as_bytes(), &m);
    assert!(out.contains("@Heading:<*C>Erste Schritte\r\n"));
    assert!(!out.contains("@Body:Hello") && out.contains("@Body:Hallo <B>fett<P> Welt<\\n>zweite Zeile\r\n"));
    assert!(out.contains("Mail an uns <\\@> einmal, 3 <\\<> 4, Pfad C:<\\\\>y<\\i>.\r\n"));
    // untranslated paragraph and every definition stay byte for byte
    assert!(out.contains("<f\"Arial\"><z10>Leading tags stay outside<f$><z$>\r\n"));
    assert!(out.starts_with("<v19.00><e9>\r\n@Normal=[S\"\",\"\"]<*L*h\"Standard\"f\"Helvetica\"z12>\r\n"));
    // unknown (x9) and duplicate (second x2) tokens dropped; missing x1 appended
    assert!(
        out.contains(
            "@$:<XI,\"Entry\",\"\",,,,,\"\">Indiziert <A(3,\"HYPB\",)[0]>ohne Link Text.<A(3,\"HYP\\\"B\",\\#002\\#000)[2]>\r\n"
        ),
        "{out}"
    );
    // a newline in a translation is the New line tag; dropped tokens at the end
    assert!(out.contains("<*t(36,0,\"1 \")>Name<\\n>Wert<\\t><c\"Red\">\r\n"), "{out}");
    let back = texts(out.as_bytes());
    assert_eq!(back[0], "Erste Schritte");
    assert_eq!(back[2], t("Mail an uns @ einmal, 3 < 4, Pfad C:\\y<<x1>>."));
}

#[test]
#[allow(clippy::type_complexity)]
fn encodings() {
    // Mac Roman (<e0>), classic Mac CR line ends
    let mac = enc(encoding_rs::MACINTOSH, "<v15.00><e0>\r@Normal:Caf\u{e9} cr\u{e8}me \u{2013} na\u{ef}ve\r");
    assert!(mac.contains(&0x8E));
    let doc = extract(&mac).unwrap();
    assert_eq!(token_text(&doc.units[0]), "Café crème – naïve");
    assert!(matches!(&doc.skeleton, Skeleton::Text { encoding, .. } if encoding == "mac_roman"));
    assert_eq!(Xtg.merge(&doc).unwrap(), mac);
    assert_eq!(build(&mac, &[(0, "Café crème – naïf")]), enc(encoding_rs::MACINTOSH, "<v15.00><e0>\r@Normal:Café crème – naïf\r"));
    // a translation Mac Roman cannot hold: the whole file becomes UTF-8 with <e9>
    assert_eq!(build(&mac, &[(0, "カフェ")]), "<v15.00><e9>\r@Normal:カフェ\r".as_bytes());
    // Windows Latin (<e1>): the euro sign is 0x80
    let w = enc(encoding_rs::WINDOWS_1252, "<v8.00><e1>\r\n@Normal:Price 5 \u{20ac}\r\n");
    assert_eq!(texts(&w)[0], "Price 5 €");
    assert_eq!(build(&w, &[(0, "Preis 5 €")]), enc(encoding_rs::WINDOWS_1252, "<v8.00><e1>\r\n@Normal:Preis 5 €\r\n"));
    // ISO-8859-1 (<e2>), Japanese Windows (<e3>), Korean (<e19>), Big5 (<e6>), GB2312 (<e7>)
    let latin1 = |s: &str| s.chars().map(|c| c as u32 as u8).collect::<Vec<u8>>();
    let cases: [(u32, Box<dyn Fn(&str) -> Vec<u8>>, &str, &str); 5] = [
        (2, Box::new(latin1), "Grüße", "iso-8859-1"),
        (3, Box::new(|s| enc(encoding_rs::SHIFT_JIS, s)), "日本語のテキスト", "shift_jis"),
        (19, Box::new(|s| enc(encoding_rs::EUC_KR, s)), "한국어 텍스트", "euc-kr"),
        (6, Box::new(|s| enc(encoding_rs::BIG5, s)), "中文文字", "big5"),
        (7, Box::new(|s| enc(encoding_rs::GBK, s)), "中文文本", "gbk"),
    ];
    for (num, f, s, tag) in cases {
        let b = f(&format!("<v19.00><e{num}>\n@Normal:{s}\n"));
        let doc = extract(&b).unwrap();
        assert_eq!(token_text(&doc.units[0]), s, "{num}");
        assert!(matches!(&doc.skeleton, Skeleton::Text { encoding, .. } if encoding == tag), "{num}");
        assert_eq!(Xtg.merge(&doc).unwrap(), b);
        assert_eq!(build(&b, &[(0, &format!("{s}!"))]), f(&format!("<v19.00><e{num}>\n@Normal:{s}!\n")));
    }
    // UTF-16 LE with BOM (<e8>, what QuarkXPress for Windows writes), and BE, and no BOM
    type Enc16 = fn(&str, bool) -> Vec<u8>;
    for (f, bom) in [(utf16le as Enc16, true), (utf16be as Enc16, true), (utf16le as Enc16, false)] {
        let b = f("<v19.00><e8>\r\n@Normal:Hello <B>world<P>\r\n", bom);
        assert_eq!(texts(&b)[0], t("Hello <<g1>>world<</g1>>"));
        assert_identity(&b);
        let out = build(&b, &[(0, &t("Hallo <<g1>>Welt<</g1>>"))]);
        assert_eq!(out, f("<v19.00><e8>\r\n@Normal:Hallo <B>Welt<P>\r\n", bom));
    }
    // <e8> declared on 8-bit bytes: the bytes win (read as UTF-8)
    assert_eq!(texts("<v1><e8>\n@N:Grüße\n".as_bytes())[0], "Grüße");
    // no <e> tag: the shared decode (UTF-8, else Windows-1252), written back the same way
    let nohead = enc(encoding_rs::WINDOWS_1252, "@Normal:Na\u{ef}ve text\n");
    let doc = extract(&nohead).unwrap();
    assert_eq!(token_text(&doc.units[0]), "Naïve text");
    assert!(matches!(&doc.skeleton, Skeleton::Text { encoding, .. } if encoding == "windows-1252"));
    assert_eq!(build(&nohead, &[(0, "Naïf")]), enc(encoding_rs::WINDOWS_1252, "@Normal:Naïf\n"));
    assert_eq!(build(&nohead, &[(0, "ナイーブ")]), "\u{feff}@Normal:ナイーブ\n".as_bytes());
}

fn refused(data: &[u8], needle: &str) -> Error {
    let e = extract(data).err().unwrap_or_else(|| panic!("not refused: {:?}", &data[..data.len().min(60)]));
    assert!(e.to_string().contains(needle), "{needle} not in {e}");
    e
}

#[test]
fn hostile_inputs() {
    assert!(matches!(refused(b"<v1><e21>\n@N:x\n", "Japanese Mac"), Error::Encoding(_)));
    assert!(matches!(refused(b"<v1><e20>\n@N:x\n", "Korean Mac"), Error::Encoding(_)));
    refused(b"<v1><e5>\n@N:x\n", "unknown encoding <e5>");
    refused(b"<v1><e1>\n@N:x\x81y\n", "cp1252");
    refused(b"<v1><e9>\n@N:caf\xe9\n", "utf-8");
    refused(b"\xff\xfe<\x00v\x00>", "odd number of bytes");
    refused(b"<v1><e9>\n@N:a\x00b\n", "NUL");
    assert!(matches!(refused(b"<v1><e9>\n@N:open <B tag\nnext>\n", "line 2"), Error::Parse { .. }));
    let mut deep = b"<v1><e9>\n@N:x<A".to_vec();
    deep.extend(std::iter::repeat_n(b'(', 80));
    deep.extend_from_slice(b">\n");
    assert!(matches!(refused(&deep, "parentheses"), Error::Limit(_)));
    // a source paragraph that already holds the token marker characters is skipped
    let doc = extract("<v1><e9>\n@N:keep \u{27E6}x1\u{27E7} out\n@N:Fine\n".as_bytes()).unwrap();
    assert_eq!(doc.units.len(), 1);
    assert_eq!(token_text(&doc.units[0]), "Fine");
    assert!(doc.warnings.iter().any(|w| w.kind == "skipped_paragraphs" && w.message.starts_with("1 ")));
    // limits: units, tags, characters
    let lim = |l: Limits| ExtractOptions::new().with_limits(l);
    let l = Limits::new().with_max_units(3);
    let mut four = b"<v1><e9>\n".to_vec();
    four.extend(b"@N:a\n".repeat(4));
    let e = Xtg.extract(&four, &lim(l)).err().unwrap();
    assert!(matches!(&e, Error::Limit(x) if x.kind == LimitKind::Units), "{e}");
    let mut tags = b"<v1><e9>\n@N:a".to_vec();
    tags.extend(b"<B>".repeat(700));
    tags.push(b'\n');
    let e = Xtg.extract(&tags, &lim(l)).err().unwrap();
    assert!(e.to_string().contains("tags"), "{e}");
    let l = Limits::new().with_max_unit_chars(50);
    let mut long = b"<v1><e9>\n@N:".to_vec();
    long.extend(b"a".repeat(60));
    long.push(b'\n');
    let e = Xtg.extract(&long, &lim(l)).err().unwrap();
    assert!(matches!(&e, Error::Limit(x) if x.kind == LimitKind::UnitChars), "{e}");
}

#[test]
fn translation_cannot_become_a_definition() {
    let src: &[u8] = b"<v19.00><e9>\n@Body=[S\"\",\"\"]<*L>\n@Body:First line\nSecond line\n";
    assert_eq!(units(src).len(), 2);
    let plain = |s: &str| vec![Inline::text(s)];
    for tr in ["~X=<y", "~Mark=<B>", "@Head:evil", "a<B>b"] {
        let mut doc = extract(src).unwrap();
        doc.units[1].segments[0].target = plain(tr);
        let out = Xtg.merge(&doc).unwrap();
        assert_eq!(units(&out).len(), 2, "{tr}: {}", String::from_utf8_lossy(&out));
    }
    let mut doc = extract(src).unwrap();
    doc.units[1].segments[0].target = plain("~X=<y");
    assert!(Xtg.merge(&doc).unwrap().ends_with(b"<\\#126>X=<\\<>y\n"));
    doc.units[1].segments[0].target = plain("@Head:evil");
    assert!(Xtg.merge(&doc).unwrap().ends_with(b"<\\@>Head:evil\n"));
}

fn ends(out: &[u8], want: &str) {
    assert!(out.ends_with(want.as_bytes()), "{} does not end with {want}", String::from_utf8_lossy(out));
}

#[test]
fn attribute_pairs_travel_with_words_680() {
    // the soak repro: the reset after the last text is part of the unit
    let src: &[u8] = b"<v13.00><e9>\n@Heading:Account <B>settings<B$>\n";
    let doc = extract(src).unwrap();
    assert_eq!(token_text(&doc.units[0]), t("Account <<g1>>settings<</g1>>"));
    let m = profile::tokens::to_locos_text(&doc.units[0].source()).1;
    assert_eq!((m["1"].open.as_deref(), m["1"].close.as_deref()), (Some("<B>"), Some("<B$>")));
    assert_identity(src);
    let out = build(src, &[(0, &t("<<g1>>Paramètres<</g1>> du compte"))]);
    assert_eq!(out, "<v13.00><e9>\n@Heading:<B>Paramètres<B$> du compte\n".as_bytes());
    assert_eq!(texts(&out)[0], t("<<g1>>Paramètres<</g1>> du compte"));
    // MT moved every word out of the pair: no attribute switched on around nothing
    let out = build_s(src, &[(0, &t("Paramètres du compte <<g1>><</g1>>"))]);
    assert!(!out.contains("<B>") && !out.contains("<B$>"), "{out}");
    // a pair opened before the first text and reset inside it pulls its opener in
    let src: &[u8] = b"<v1><e9>\n@B:<*C><B>Account<B$> settings\n";
    assert_eq!(texts(src)[0], t("<<g1>>Account<</g1>> settings"));
    ends(&build(src, &[(0, &t("Paramètres du <<g1>>compte<</g1>>"))]), "@B:<*C>Paramètres du <B>compte<B$>\n");
    // a pair around the whole text stays in the skeleton
    let src: &[u8] = b"<v1><e9>\n@B:<f\"Arial\"><z10>Whole<f$><z$>\n";
    assert_eq!(texts(src)[0], "Whole");
    ends(&build(src, &[(0, "Ganz")]), "<f\"Arial\"><z10>Ganz<f$><z$>\n");
    // nested pairs; closes that stay together keep the source's own reset tags, separated closes reset their own
    let src: &[u8] = b"<v1><e9>\n@B:x <B>a <I>b<I$><B$> c\n";
    assert_eq!(texts(src)[0], t("x <<g1>>a <<g2>>b<</g2>><</g1>> c"));
    ends(&build(src, &[(0, &t("X <<g1>>A <<g2>>B<</g2>><</g1>> C"))]), "@B:X <B>A <I>B<I$><B$> C\n");
    ends(&build(src, &[(0, &t("X <<g2>>B<</g2>> <<g1>>A<</g1>> C"))]), "@B:X <I>B<I$> <B>A<B$> C\n");
    let src: &[u8] = b"<v1><e9>\n@B:<B>a <I>b<$>\n";
    assert_eq!(texts(src)[0], t("<<g1>>a <<g2>>b<</g2>><</g1>>"));
    ends(&build(src, &[(0, &t("<<g2>>B<</g2>> <<g1>>A<</g1>>"))]), "@B:<I>B<I$> <B>A<$>\n");
    // every attribute kind: style sheets, <P>, toggles switched back, fonts, colours
    let src: &[u8] = b"<v1><e9>\n@B:x <@Emph>a<@$> <BI>c<P> d <B>e<B> <f\"Arial\">g<f$> <c\"Red\">h<c$>\n";
    let want = t("x <<g1>>a<</g1>> <<g2>>c<</g2>> d <<g3>>e<</g3>> <<g4>>g<</g4>> <<g5>>h<</g5>>");
    assert_eq!(texts(src)[0], want);
    let out = build(src, &[(0, &t("<<g5>>H<</g5>> <<g4>>G<</g4>> <<g3>>E<</g3>> D <<g2>>C<</g2>> <<g1>>A<</g1>> X"))]);
    ends(&out, "@B:<c\"Red\">H<c$> <f\"Arial\">G<f$> <B>E<B> D <BI>C<P> <@Emph>A<@$> X\n");
    // point tokens inside a pair stay points; an attribute never reset stays a point
    let src: &[u8] = b"<v1><e9>\n@B:<c\"Red\">Red<c$> and <B>bold<\\n>more<B$> <z12>big\n";
    assert_eq!(texts(src)[0], t("<<g1>>Red<</g1>> and <<g2>>bold<<x3>>more<</g2>> <<x4>>big"));
    // what does not nest cleanly keeps the point tokens
    assert_eq!(texts(b"<v1><e9>\n@B:x <z12>a<z14>b<z$> d\n")[0], t("x <<x1>>a<<x2>>b<<x3>> d"));
    // a pair the translation does not write whole loses its formatting and keeps its words
    let src: &[u8] = b"<v1><e9>\n@B:x <B>a <I>b<I$><B$> c\n";
    let cases: [(&str, &str); 7] = [
        ("X A B C", "@B:X A B C\n"),
        ("X <<g1>>A <<g2>>B<</g1>><</g2>> C", "@B:X <B>A <I>B<B$><I$> C\n"),
        ("X <<g1>>A <<g2>>B<</g2>> C", "@B:X A <I>B<I$> C\n"),
        ("X <<g1>>A <<g2>>B<</g2>><</g1>> C <<g2>>", "@B:X <B>A <I>B<I$><B$> C \n"),
        ("X <</g1>>A <<g2>>B<</g2>><<g1>> C", "@B:X A <I>B<I$> C\n"),
        ("X <<x1>>A <<g2>>B<</g2>><</g1>> C", "@B:X A <I>B<I$> C\n"),
        ("X <<g1>>A <<g2>>B<</g2>> C<</g1>> <</g1>>", "@B:X <B>A <I>B<I$> C<B$> \n"),
    ];
    for (got, want) in cases {
        let out = build(src, &[(0, &t(got))]);
        ends(&out, want);
        assert!(extract(&out).is_ok());
    }
    // a source <$> that ends several pairs stays a reset-all when separated
    let src: &[u8] = b"<v1><e9>\n@B:x <B>a <I>b<z12>c<z14>d<$> e\n";
    assert_eq!(texts(src)[0], t("x <<g1>>a <<g2>>b<<x3>>c<<x4>>d<</g2>><</g1>> e"));
    let out = build(src, &[(0, &t("<<g2>>B<<x3>>C<<x4>>D<</g2>> X <<g1>>A<</g1>> E"))]);
    ends(&out, "@B:<I>B<z12>C<z14>D<I$> X <B>A<$> E\n");
}

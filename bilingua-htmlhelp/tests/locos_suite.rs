//! The Localization OS HTML Help suite (`tests/verify_htmlhelp_filter.py`, 11 tests), ported one to one. The two
//! tests that go through the app (registration and the API/CLI round trip) are ported to what they check: the filters'
//! ids and extensions, and a translate-and-merge for ru-RU that writes a Windows-1251 file.
mod common;
use bilingua_htmlhelp::*;
use bilingua_model::encoding_rs::{BIG5, SHIFT_JIS, WINDOWS_1251, WINDOWS_1252, WINDOWS_1258};
use bilingua_model::*;
use common::*;
use std::time::Instant;

fn s(b: &[u8]) -> String {
    dec(WINDOWS_1252, b)
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

fn err_text(r: Result<Vec<u8>, Error>) -> String {
    r.expect_err("expected a refusal").to_string()
}

#[test]
fn registration() {
    for (f, id, ext) in [(&HhcFilter as &dyn Filter, "hhc", "hhc"), (&HhkFilter, "hhk", "hhk"), (&HhpFilter, "hhp", "hhp")] {
        assert_eq!(f.id(), id);
        assert_eq!(f.extensions(), &[ext]);
    }
    assert!(HhcFilter.sniff(HHC.as_bytes()));
    assert!(HhkFilter.sniff(HHK.as_bytes()));
    assert!(!HhcFilter.sniff(HHK.as_bytes()));
    assert!(HhpFilter.sniff(HHP.as_bytes()));
    assert!(!HhpFilter.sniff(b"[Section]\nkey=value\n"));
}

#[test]
fn hhc_parse_and_identity() {
    let data = cp1252(HHC);
    let doc = extract(&HhcFilter, &data).unwrap();
    let got: Vec<String> = doc.units.iter().map(|u| plain_text(&u.source())).collect();
    assert_eq!(got, HHC_UNITS);
    match &doc.skeleton {
        Skeleton::Text { encoding, .. } => assert_eq!(encoding, "windows-1252"),
        _ => panic!("text skeleton"),
    }
    assert_eq!(build(&HhcFilter, &data, &[]), data);
    let echo: Vec<(usize, &str)> = HHC_UNITS.iter().enumerate().map(|(i, t)| (i, *t)).collect();
    assert_eq!(build(&HhcFilter, &data, &echo), data);
}

#[test]
fn hhc_translated_round_trip() {
    let data = cp1252(HHC);
    let out = build(
        &HhcFilter,
        &data,
        &[(0, "Erste Schritte"), (1, "Caf\u{e9} & \"Bar\" <Tipps>"), (2, "%1 und {0} hier"), (3, "X")],
    );
    let exp = cp1252(
        &HHC.replace("value=\"Getting started\"", "value=\"Erste Schritte\"")
            .replace("&lt;tips&gt;", "&lt;Tipps&gt;")
            .replace("VALUE='Use %1 and {0} here'", "VALUE='%1 und {0} hier'")
            .replace("value=\"Unquoted\"", "value=\"X\""),
    );
    assert_eq!(s(&out), s(&exp));
    assert_eq!(texts(&HhcFilter, &out), ["Erste Schritte", "Caf\u{e9} & \"Bar\" <Tipps>", "%1 und {0} hier", "X"]);
    for keep in [
        &b"value=\"html\\start.htm\""[..],
        b"value=\"html/caf\xe9.htm#top\"",
        b"value=http://example.test/x.htm",
        b"value=\"Not translated\"",
        b"value=\"other.chm::/toc.hhc\"",
        b"value=\"Outside any object\"",
        b"value=\"Tahoma,8,0\"",
        b"value=\"1\"",
    ] {
        assert!(contains(&out, keep), "{}", String::from_utf8_lossy(keep));
    }
    // an unquoted value gets quotes when it is rewritten; a single-quoted one escapes '
    let src = b"<OBJECT type=\"text/sitemap\"><param name=Name value=Plain></OBJECT>";
    assert_eq!(build(&HhcFilter, src, &[(0, "Two words")]), b"<OBJECT type=\"text/sitemap\"><param name=Name value=\"Two words\"></OBJECT>");
    let out = build(&HhcFilter, &data, &[(2, "it's \"x\"")]);
    assert!(contains(&out, b"VALUE='it&#39;s &quot;x&quot;'"), "{}", s(&out));
    assert_eq!(texts(&HhcFilter, &out)[2], "it's \"x\"");
}

#[test]
fn hhk_index_and_see_also() {
    let data = cp1252(HHK);
    assert_eq!(texts(&HhkFilter, &data), ["printing", "How to print", "Print settings", "page setup", "output", "paper"]);
    assert_eq!(build(&HhkFilter, &data, &[]), data);
    let out = s(&build_loc(&HhkFilter, &data, &[(0, "impression"), (1, "Comment imprimer"), (3, "mise en page")], "fr-FR").unwrap());
    assert!(out.contains("<param name=\"See Also\" value=\"impression\">"), "{out}");
    assert!(out.contains("<param name=\"See Also\" value=\"no such keyword\">"));
    assert!(out.contains("<param name=\"Keyword\" value=\"mise en page\">"));
    assert!(out.contains("charset=windows-1252")); // already right for French: untouched
    let out = dec(WINDOWS_1251, &build_loc(&HhkFilter, &data, &[(0, "\u{43f}\u{435}\u{447}\u{430}\u{442}\u{44c}")], "ru").unwrap());
    assert!(out.contains("charset=windows-1251"), "{out}");
    assert!(out.contains("<param name=\"See Also\" value=\"\u{43f}\u{435}\u{447}\u{430}\u{442}\u{44c}\">"));
}

#[test]
fn code_pages_and_refusal() {
    use bilingua_model::codepage::target_code_page as cp;
    assert_eq!((cp("ru-RU"), cp("ja")), (Some(1251), Some(932)));
    assert_eq!((cp("zh-Hant"), cp("zh-CN")), (Some(950), Some(936)));
    assert_eq!((cp("sr-Latn-RS"), cp("sr")), (Some(1250), Some(1251)));
    assert_eq!(cp("hi-IN"), None);
    let data = cp1252(HHC);
    let clean = cp1252(&HHC.replace('\u{e9}', "e"));
    let out = build_loc(&HhcFilter, &clean, &[(0, "\u{65e5}\u{672c}\u{8a9e}")], "ja-JP").unwrap();
    assert_eq!(dec(SHIFT_JIS, &out).matches("\u{65e5}\u{672c}\u{8a9e}").count(), 1);
    assert!(contains(&out, b"value=\"Tahoma,8,128\"")); // Font charset = SHIFTJIS_CHARSET
    // untranslated source text counts too: the e-acute in a Local path has no cp932 byte
    let e = err_text(build_loc(&HhcFilter, &data, &[(0, "\u{65e5}\u{672c}\u{8a9e}")], "ja-JP"));
    assert!(e.contains("U+00E9"), "{e}");
    let e = err_text(build_loc(&HhcFilter, &data, &[(0, "\u{416}")], "el-GR"));
    assert!(e.contains("Windows-1253") && e.contains("U+0416") && e.contains("utf8_fallback"), "{e}");
    // sw has the Western code page
    let out = build_loc(&HhcFilter, &data, &[(0, "Anza")], "sw").unwrap();
    assert!(out.starts_with(b"<!DOCTYPE") && contains(&out, b"value=\"Anza\""));
    assert!(build_loc(&HhcFilter, &data, &[(0, "\u{936}\u{941}\u{930}\u{942}")], "hi-IN").is_err());
    // UTF-8 fallback: written as UTF-8, meta charset (when present) relabelled
    let out = build_loc_utf8(&HhcFilter, &data, &[(0, "\u{936}\u{941}\u{930}\u{942}")], "hi-IN");
    assert!(String::from_utf8(out).unwrap().contains("\u{936}\u{941}\u{930}\u{942}"));
    let hhk = cp1252(HHK);
    let out = String::from_utf8(build_loc_utf8(&HhkFilter, &hhk, &[(0, "\u{65e5}\u{672c}")], "ru-RU")).unwrap();
    assert!(out.contains("charset=UTF-8") && out.contains("value=\"\u{65e5}\u{672c}\""), "{out}");
}

#[test]
fn vietnamese_cp1258() {
    let clean = cp1252(&HHC.replace('\u{e9}', "e"));
    let vi = "T\u{1ed5}ng quan v\u{1ec1} \u{111}i\u{1ec7}n tho\u{1ea1}i Vi\u{1ec7}t Nam";
    let out = build_loc(&HhcFilter, &clean, &[(0, vi)], "vi-VN").unwrap();
    assert!(contains(&out, b"T\xf4\xd2ng"), "{}", dec(WINDOWS_1258, &out));
    // Windows-1258's own forms: o-circumflex + hook above (0xF4 0xD2) for the toned letter
    let text = dec(WINDOWS_1258, &out);
    assert!(text.contains("T\u{f4}\u{309}ng quan"), "{text}");
    let e = err_text(build_loc(&HhcFilter, &clean, &[(0, "Vi\u{1ec7}t \u{65e5}\u{672c}")], "vi-VN"));
    assert!(e.contains("U+65E5"), "{e}");
}

#[test]
fn typographic_fallback_715() {
    let clean = cp1252(&HHC.replace('\u{e9}', "e"));
    let fr = "Aide\u{202f}: index\u{2009}\u{2011} a\u{2212}b\u{200b}c \u{ab}x\u{202f}\u{bb} \u{2026}";
    let out = s(&build_loc(&HhcFilter, &clean, &[(0, fr)], "fr-FR").unwrap());
    assert!(out.contains("Aide\u{a0}: index -"), "{out:?}");
    assert!(out.contains("a-bc \u{ab}x\u{a0}\u{bb} \u{2026}"), "{out:?}");
    let ru = build_loc(&HhcFilter, &clean, &[(0, "\u{421}\u{43f}\u{440}\u{430}\u{432}\u{43a}\u{430}\u{202f}\u{2011}")], "ru-RU").unwrap();
    assert!(dec(WINDOWS_1251, &ru).contains("\u{421}\u{43f}\u{440}\u{430}\u{432}\u{43a}\u{430}\u{a0}-"));
    let e = err_text(build_loc(&HhcFilter, &clean, &[(0, "Aide\u{202f}: \u{65e5}")], "fr-FR"));
    assert!(e.contains("U+65E5"), "{e}");
}

#[test]
fn encodings_and_bom() {
    // pure ASCII: ANSI; unchanged bytes on build
    let ascii = HHC.replace('\u{e9}', "e").into_bytes();
    let doc = extract(&HhcFilter, &ascii).unwrap();
    assert!(matches!(&doc.skeleton, Skeleton::Text { encoding, .. } if encoding == "windows-1252"));
    assert_eq!(build(&HhcFilter, &ascii, &[]), ascii);
    // declared charset (Russian source)
    let ru = HHK
        .replace("windows-1252", "windows-1251")
        .replace("value=\"printing\"", "value=\"\u{43f}\u{435}\u{447}\u{430}\u{442}\u{44c}\"");
    let rb = enc(WINDOWS_1251, &ru);
    assert_eq!(texts(&HhkFilter, &rb)[0], "\u{43f}\u{435}\u{447}\u{430}\u{442}\u{44c}");
    assert_eq!(build(&HhkFilter, &rb, &[]), rb);
    let out = s(&build_loc(&HhkFilter, &rb, &[(0, "printing")], "en-US").unwrap());
    assert!(out.contains("charset=windows-1252") && out.contains("<param name=\"See Also\" value=\"printing\">"), "{out}");
    // UTF-8 with BOM and non-ASCII UTF-8 stay UTF-8; UTF-16 LE/BE keep their BOM
    let u16le: Vec<u8> = [0xFF, 0xFE].into_iter().chain(HHC.encode_utf16().flat_map(|u| u.to_le_bytes())).collect();
    let u16be: Vec<u8> = [0xFE, 0xFF].into_iter().chain(HHC.encode_utf16().flat_map(|u| u.to_be_bytes())).collect();
    let u8bom: Vec<u8> = [0xEF, 0xBB, 0xBF].into_iter().chain(HHC.bytes()).collect();
    for raw in [u8bom, HHC.as_bytes().to_vec(), u16le, u16be] {
        assert_eq!(texts(&HhcFilter, &raw), HHC_UNITS);
        assert_eq!(build(&HhcFilter, &raw, &[]), raw);
        let out = build_loc(&HhcFilter, &raw, &[(0, "\u{65e5}\u{672c}")], "ru-RU").unwrap();
        assert_eq!(out[..2], raw[..2]);
        assert_eq!(texts(&HhcFilter, &out)[0], "\u{65e5}\u{672c}");
    }
    // HHP in cp1252
    let hb = cp1252(HHP);
    assert_eq!(texts(&HhpFilter, &hb), ["Product Manual", "Product Help"]);
    assert_eq!(build(&HhpFilter, &hb, &[]), hb);
}

#[test]
fn hhp_title_window_language() {
    let hb = cp1252(HHP);
    let out = s(&build_loc(&HhpFilter, &hb, &[(0, "Manuel du produit"), (1, "Aide \"produit\"")], "fr-CA").unwrap());
    let exp = HHP
        .replace("Title=Product Manual", "Title=Manuel du produit")
        .replace("Language=0x409 English (United States)", "Language=0xc0c French (Canada)")
        .replace("main=\"Product Help\"", "main=\"Aide 'produit'\"");
    assert_eq!(out, exp);
    // nothing translated: no language stamp (lead decision 11-04), original bytes
    assert_eq!(build_loc(&HhpFilter, &hb, &[], "de").unwrap(), hb);
    let out = s(&build_loc(&HhpFilter, &hb, &[(0, "Produkthandbuch")], "de").unwrap());
    assert_eq!(out.matches("Language=0x407 German (Germany)").count(), 1);
    let out = dec(BIG5, &build_loc(&HhpFilter, &hb, &[(0, "\u{88fd}\u{54c1}")], "zh-TW").unwrap());
    assert!(out.contains("Language=0x404 Chinese (Traditional, Taiwan)") && out.contains("Title=\u{88fd}\u{54c1}"), "{out}");
    assert_eq!(target_lcid("ja").unwrap().0, 0x411);
    assert!(target_lcid("xx").is_none());
}

#[test]
fn hostile_inputs() {
    let e = HhcFilter.extract(&b"\x00\x01\x02\x03".repeat(50), &ExtractOptions::default()).unwrap_err();
    assert!(matches!(e, Error::Encoding(_)) && e.to_string().contains("binary"), "{e}");
    let e = HhcFilter.extract(b"<OBJECT type=\"text/sitemap\"><param name=\"Local\" value=\"a.htm\"></OBJECT>", &ExtractOptions::default()).unwrap_err();
    assert!(e.to_string().contains("no entry names"), "{e}");
    let e = HhpFilter.extract(b"[FILES]\r\na.htm\r\n", &ExtractOptions::default()).unwrap_err();
    assert!(e.to_string().contains("Title"), "{e}");
    // unterminated comment, stray '<', unclosed tags: lenient, never crash or hang
    for raw in [
        &b"<!-- never closed <OBJECT type=text/sitemap><param name=Name value=X>"[..],
        b"< <<< <OBJECT type='text/sitemap'><param name='Name' value='a < b'>",
        b"<OBJECT type=text/sitemap><param name=Name value=\"unterminated>",
    ] {
        let _ = HhcFilter.extract(raw, &ExtractOptions::default());
    }
    assert_eq!(texts(&HhcFilter, b"< <<< <OBJECT type='text/sitemap'><param name='Name' value='a < b'>"), ["a < b"]);
    // caps: pre-parse budget, segment cap, one huge value
    let small = ExtractOptions::new().with_limits(Limits::new().with_max_units(5));
    let e = HhcFilter.extract(&b"<".repeat(500), &small).unwrap_err();
    assert!(matches!(e, Error::Limit(_)) && e.to_string().contains("segments"), "{e}");
    let many: Vec<u8> = (0..6).flat_map(|i| format!("<OBJECT type=\"text/sitemap\"><param name=\"Name\" value=\"Entry {i}\"></OBJECT>").into_bytes()).collect();
    let e = HhcFilter.extract(&many, &small).unwrap_err();
    assert!(matches!(e, Error::Limit(_)) && e.to_string().contains("segments"), "{e}");
    let huge = [b"<OBJECT type=\"text/sitemap\"><param name=\"Name\" value=\"".to_vec(), vec![b'a'; 100_005], b"\"></OBJECT>".to_vec()].concat();
    let e = HhcFilter.extract(&huge, &ExtractOptions::default()).unwrap_err();
    assert!(matches!(e, Error::Limit(_)) && e.to_string().contains("longer than"), "{e}");
    // a translation cannot inject markup or break out of the attribute
    let data = cp1252(HHC);
    let out = build(&HhcFilter, &data, &[(0, "\"><script>alert(1)</script>")]);
    assert!(!contains(&out, b"<script>") && contains(&out, b"&quot;&gt;&lt;script&gt;"));
    // tags that never close must not backtrack across the file
    let entry = b"<OBJECT type=\"text/sitemap\"><param name=\"Name\" value=\"Hi\"></OBJECT>\n".to_vec();
    for junk in [&b"<a x "[..], b"<a x\n", b"<a b=c", b"<p a=\""] {
        let raw = [entry.clone(), junk.repeat(40_000)].concat();
        let t0 = Instant::now();
        assert_eq!(texts(&HhcFilter, &raw), ["Hi"]);
        assert!(t0.elapsed().as_secs() < 5, "{junk:?}");
    }
    // unquoted values still end at '<' only where HTML says so
    assert_eq!(texts(&HhcFilter, b"<OBJECT type=text/sitemap><param name=Name value=a/b=c></OBJECT>"), ["a/b=c"]);
}

#[test]
fn translate_and_merge_round_trip() {
    // the app test uploads a .hhc, mock-translates to ru-RU and exports: a Windows-1251 file with only the Name
    // values changed and the Font charset set to 204
    let raw = cp1252(HHC);
    let data = cp1252(&HHC.replace("caf\u{e9}.htm", "cafe.htm").replace("Caf\u{e9}", "Cafe"));
    let doc = extract(&HhcFilter, &data).unwrap();
    let map: Vec<(usize, String)> = doc.units.iter().enumerate().map(|(i, u)| (i, format!("[ru-RU] {}", plain_text(&u.source())))).collect();
    let map: Vec<(usize, &str)> = map.iter().map(|(i, t)| (*i, t.as_str())).collect();
    let out = dec(WINDOWS_1251, &build_loc(&HhcFilter, &data, &map, "ru-RU").unwrap());
    assert!(out.contains("value=\"[ru-RU] Getting started\""), "{out}");
    assert!(out.contains("value=\"Tahoma,8,204\""));
    assert!(out.contains("value=\"html/cafe.htm#top\""));
    assert!(out.contains("\r\n") && out.contains("name=\"Local\" value=\"html\\start.htm\""));
    // the raw file has an e-acute that Windows-1251 cannot hold: a clean refusal
    let e = err_text(build_loc(&HhcFilter, &raw, &[(0, "[ru-RU] Getting started")], "ru-RU"));
    assert!(e.contains("Windows-1251"), "{e}");
}

#[test]
fn empty_but_valid_files_open_with_zero_units() {
    let hhc = b"<!DOCTYPE HTML PUBLIC \"-//IETF//DTD HTML//EN\">
<HTML><HEAD><!-- Sitemap 1.0 --></HEAD>
<BODY>
<UL>
</UL>
</BODY></HTML>
";
    let hhk = b"<HTML><BODY><OBJECT type=\"text/site properties\"><param name=\"FrameName\" value=\"main\"></OBJECT><UL></UL></BODY></HTML>";
    let hhp = b"[OPTIONS]
Language=0x409 English (United States)
Contents file=a.hhc

[FILES]
a.htm
";
    for (f, raw) in [(&HhcFilter as &dyn Filter, &hhc[..]), (&HhkFilter, &hhk[..]), (&HhpFilter, &hhp[..])] {
        let mut d = f.extract(raw, &ExtractOptions::default()).unwrap();
        assert!(d.units.is_empty(), "{}", d.format);
        assert_eq!(d.warnings.len(), 1, "{}", d.format);
        assert_eq!(d.warnings[0].kind, "no_translatable_text");
        assert_eq!(f.merge(&d).unwrap(), raw, "{}", d.format);
        // even with a target language the file is not stamped: nothing was translated
        d.target_lang = Some("de".into());
        assert_eq!(f.merge(&d).unwrap(), raw, "{} with target", d.format);
    }
    // malformed (not just empty) is still refused with a clear message
    let e = HhcFilter.extract(&[0u8, 1, 2, 3].repeat(50), &ExtractOptions::default()).unwrap_err();
    assert!(e.to_string().contains("binary"), "{e}");
}

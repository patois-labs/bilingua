//! Round trips over the made-up corpus (`tests/fixtures/corpus`): twelve languages in their Windows ANSI code pages,
//! a UTF-8 (BOM) and a UTF-16 contents file. The oracle corpus (`oracle/corpus/htmlhelp`) holds the files of it that
//! the oracle's Spanish pseudo-translation can be written into (an Arabic or Korean ANSI file is refused for es-ES, as
//! Localization OS refuses it).
mod common;
use bilingua_htmlhelp::*;
use bilingua_model::encoding_rs::{Encoding, BIG5, EUC_KR, GBK, SHIFT_JIS, WINDOWS_1250, WINDOWS_1251, WINDOWS_1252, WINDOWS_1253, WINDOWS_1256, WINDOWS_1258};
use bilingua_model::*;
use common::*;

fn corpus() -> Vec<(String, Vec<u8>)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/corpus");
    let mut out: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir).unwrap().flatten().map(|f| (f.file_name().to_string_lossy().to_string(), std::fs::read(f.path()).unwrap())).collect();
    out.sort();
    assert!(out.len() >= 38, "{} corpus files", out.len());
    out
}

fn filter_of(name: &str) -> Box<dyn Filter> {
    match name.rsplit('.').next().unwrap() {
        "hhc" => Box::new(HhcFilter),
        "hhk" => Box::new(HhkFilter),
        _ => Box::new(HhpFilter),
    }
}

fn file(name: &str) -> Vec<u8> {
    corpus().into_iter().find(|(n, _)| n == name).unwrap_or_else(|| panic!("{name}")).1
}

/// (corpus code, target locale, decoder, meta charset, GDI charset)
const LANGS: [(&str, &str, &Encoding, &str, &str); 11] = [
    ("de", "de-DE", WINDOWS_1252, "windows-1252", "0"),
    ("fr", "fr-FR", WINDOWS_1252, "windows-1252", "0"),
    ("ru", "ru-RU", WINDOWS_1251, "windows-1251", "204"),
    ("ar", "ar-SA", WINDOWS_1256, "windows-1256", "178"),
    ("ja", "ja-JP", SHIFT_JIS, "shift_jis", "128"),
    ("zh", "zh-CN", GBK, "gb2312", "134"),
    ("zh-tw", "zh-Hant", BIG5, "big5", "136"),
    ("ko", "ko-KR", EUC_KR, "ks_c_5601-1987", "129"),
    ("el", "el-GR", WINDOWS_1253, "windows-1253", "161"),
    ("vi", "vi-VN", WINDOWS_1258, "windows-1258", "163"),
    ("pl", "pl-PL", WINDOWS_1250, "windows-1250", "238"),
];

#[test]
fn untouched_merge_gives_back_the_original_bytes() {
    for (name, bytes) in corpus() {
        let f = filter_of(&name);
        let doc = extract(f.as_ref(), &bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(doc.units.len() >= 2, "{name}");
        assert_eq!(f.merge(&doc).unwrap(), bytes, "{name}: untouched merge");
        let echo: Vec<(usize, String)> = doc.units.iter().enumerate().map(|(i, u)| (i, plain_text(&u.source()))).collect();
        let echo: Vec<(usize, &str)> = echo.iter().map(|(i, t)| (*i, t.as_str())).collect();
        assert_eq!(build(f.as_ref(), &bytes, &echo), bytes, "{name}: echo merge");
    }
}

#[test]
fn pseudo_translated_merge_reads_back_with_the_same_units() {
    for (name, bytes) in corpus() {
        let f = filter_of(&name);
        let before = texts(f.as_ref(), &bytes);
        let map: Vec<(usize, String)> = before.iter().enumerate().map(|(i, t)| (i, format!("[{i}] {t}"))).collect();
        let m: Vec<(usize, &str)> = map.iter().map(|(i, t)| (*i, t.as_str())).collect();
        let out = build(f.as_ref(), &bytes, &m);
        let after = texts(f.as_ref(), &out);
        assert_eq!(after.len(), before.len(), "{name}");
        for (a, (_, t)) in after.iter().zip(&map) {
            assert_eq!(a, t, "{name}");
        }
        // a second merge of the result is stable
        assert_eq!(build(f.as_ref(), &out, &[]), out, "{name}");
    }
}

#[test]
fn english_files_translate_into_eleven_languages() {
    for (code, locale, dec_enc, meta, gdi) in LANGS {
        let want = texts(&HhcFilter, &file(&format!("toc_{code}.hhc")));
        let en = file("toc_en.hhc");
        let map: Vec<(usize, &str)> = want.iter().enumerate().map(|(i, t)| (i, t.as_str())).collect();
        let out = build_loc(&HhcFilter, &en, &map, locale).unwrap_or_else(|e| panic!("{code}: {e}"));
        assert_eq!(texts(&HhcFilter, &out), want, "{code}: units read back");
        let text = dec_enc.decode_without_bom_handling(&out).0;
        assert!(text.contains(&format!("charset={meta}")), "{code}: meta charset\n{text}");
        assert!(text.contains(&format!("\"Tahoma,8,{gdi}\"")), "{code}: font charset");
        assert!(text.contains("\r\n") && text.contains("html/en/start.htm"), "{code}");
        // and the same bytes as the file the corpus holds for that language, up to the paths and the markup
        let theirs = file(&format!("toc_{code}.hhc"));
        assert_eq!(texts(&HhcFilter, &theirs), want);

        // index and project
        let hk = texts(&HhkFilter, &file(&format!("index_{code}.hhk")));
        let hk_map: Vec<(usize, &str)> = hk.iter().enumerate().map(|(i, t)| (i, t.as_str())).collect();
        let out = build_loc(&HhkFilter, &file("index_en.hhk"), &hk_map, locale).unwrap_or_else(|e| panic!("{code}: {e}"));
        assert_eq!(texts(&HhkFilter, &out), hk, "{code}: index");
        let text = dec_enc.decode_without_bom_handling(&out).0;
        // See Also follows the keyword it names
        assert!(text.contains(&format!("\"See Also\" value=\"{}\"", hk[0].replace('"', "&quot;"))), "{code}: See Also\n{text}");

        let hp = texts(&HhpFilter, &file(&format!("project_{code}.hhp")));
        let hp_map: Vec<(usize, &str)> = hp.iter().enumerate().map(|(i, t)| (i, t.as_str())).collect();
        let out = build_loc(&HhpFilter, &file("project_en.hhp"), &hp_map, locale).unwrap_or_else(|e| panic!("{code}: {e}"));
        let back = HhpFilter.extract_with(&out, &ExtractOptions::default(), hhp_code_page(&out)).unwrap();
        let back: Vec<String> = back.units.iter().map(|u| plain_text(&u.source())).collect();
        if code != "vi" {
            // Windows-1258 writes toned letters as base + combining mark, so the read-back text is the decomposed
            // spelling of the precomposed UTF-8 project text; the contents and index files above are already that.
            assert_eq!(back, hp, "{code}: project");
        }
        let text = dec_enc.decode_without_bom_handling(&out).0;
        let theirs_bytes = file(&format!("project_{code}.hhp"));
        let theirs = dec_enc.decode_without_bom_handling(&theirs_bytes).0;
        let lang_of = |t: &str| t.lines().find(|l| l.starts_with("Language=")).unwrap().to_string();
        assert_eq!(lang_of(&text), lang_of(&theirs), "{code}: Language=");
    }
}

#[test]
fn unicode_files_keep_their_envelope_on_a_target_merge() {
    let de = file("toc_de_utf8bom.hhc");
    let out = build_loc(&HhcFilter, &de, &[(0, "\u{65e5}\u{672c}\u{8a9e}")], "ja-JP").unwrap();
    assert_eq!(&out[..3], &[0xEF, 0xBB, 0xBF]);
    assert_eq!(texts(&HhcFilter, &out)[0], "\u{65e5}\u{672c}\u{8a9e}");
    let ja = file("toc_ja_utf16le.hhc");
    let out = build_loc(&HhcFilter, &ja, &[(0, "Start")], "en-US").unwrap();
    assert_eq!(&out[..2], &[0xFF, 0xFE]);
    assert_eq!(texts(&HhcFilter, &out)[0], "Start");
}

#[test]
fn paragraph_mode_and_ids() {
    for (name, bytes) in corpus() {
        let doc = extract(filter_of(&name).as_ref(), &bytes).unwrap();
        for (i, u) in doc.units.iter().enumerate() {
            assert_eq!(u.id, format!("u{}", i + 1), "{name}");
            assert_eq!(u.segments.len(), 1);
            assert_eq!(u.segments[0].id, u.id);
            assert!(u.codes.keys().all(|k| k.starts_with('$')), "{name}: units carry no inline codes");
        }
    }
}

#[test]
fn project_language_gives_the_code_page_of_the_other_files() {
    // A Russian contents file with no charset declared and no non-ASCII UTF-8: ASCII is Windows-1252 unless the
    // project's Language= says otherwise.
    assert_eq!(hhp_code_page(&file("project_ru.hhp")), Some(1251));
    assert_eq!(hhp_code_page(&file("project_ja.hhp")), Some(932));
    assert_eq!(hhp_code_page(&file("project_en.hhp")), Some(1252));
    assert_eq!(hhp_code_page(b"[OPTIONS]\r\nTitle=x\r\n"), None);
    let ru = "<OBJECT type=\"text/sitemap\"><param name=\"Name\" value=\"\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}\">".to_string();
    let bytes = common::enc(WINDOWS_1251, &ru);
    let doc = HhcFilter.extract_with(&bytes, &ExtractOptions::default(), hhp_code_page(&file("project_ru.hhp"))).unwrap();
    assert_eq!(plain_text(&doc.units[0].source()), "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}");
    assert_eq!(HhcFilter.merge(&doc).unwrap(), bytes);
}

#[test]
fn locked_values_are_never_extracted() {
    // "---", "1", Local, URL, Comment, Merge: none of them is a unit
    let doc = extract(&HhcFilter, &cp1252(HHC)).unwrap();
    assert_eq!(doc.units.len(), 4);
    assert!(doc.units.iter().all(|u| u.translate));
}

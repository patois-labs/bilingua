//! Round trips: untouched merges give the original bytes, pseudo-translated merges read back with the same units, in
//! eleven languages (Arabic, Hebrew, Japanese, Chinese, Korean, Thai, Russian, Greek, ...) and the legacy encodings.
mod common;
use bilingua_image::{ImageFilter, SvgFilter};
use bilingua_model::*;
use common::*;

const LANGS: [(&str, &str); 11] = [
    ("de-DE", "Gr\u{fc}\u{df}e aus K\u{f6}ln"),
    ("fr-FR", "\u{c0} bient\u{f4}t, c'est l'\u{e9}t\u{e9}"),
    ("es-ES", "\u{bf}D\u{f3}nde est\u{e1} el ba\u{f1}o?"),
    ("ru-RU", "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}, \u{43c}\u{438}\u{440}"),
    ("el-GR", "\u{393}\u{3b5}\u{3b9}\u{3b1} \u{3c3}\u{3bf}\u{3c5} \u{3ba}\u{3cc}\u{3c3}\u{3bc}\u{3b5}"),
    ("ar-SA", "\u{645}\u{631}\u{62d}\u{628}\u{627} \u{628}\u{627}\u{644}\u{639}\u{627}\u{644}\u{645}"),
    ("he-IL", "\u{5e9}\u{5dc}\u{5d5}\u{5dd} \u{5e2}\u{5d5}\u{5dc}\u{5dd}"),
    ("ja-JP", "\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}\u{4e16}\u{754c}"),
    ("zh-CN", "\u{4f60}\u{597d}\u{ff0c}\u{4e16}\u{754c}"),
    ("ko-KR", "\u{c548}\u{b155}\u{d558}\u{c138}\u{c694} \u{c138}\u{acc4}"),
    ("th-TH", "\u{e2a}\u{e27}\u{e31}\u{e2a}\u{e14}\u{e35}\u{e0a}\u{e32}\u{e27}\u{e42}\u{e25}\u{e01}"),
];

const SRC: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"50\">\n<title>Chart</title>\n<text x=\"1\" y=\"2\">Hello &amp; welcome</text>\n<text x=\"1\" y=\"9\" text-anchor=\"end\">Second <tspan>part</tspan></text>\n<desc>A chart</desc>\n</svg>\n";

fn translate_all(data: &[u8], lang: &str, text: &str, o: &MergeOptions) -> (Document, Vec<u8>) {
    let mut d = SvgFilter.extract(data, &with_target(lang)).unwrap();
    for i in 0..d.units.len() {
        set(&mut d, i, &format!("{text} {i}"));
    }
    let out = SvgFilter.merge_with(&d, o).unwrap();
    (d, out)
}

#[test]
fn eleven_languages_read_back_with_the_same_units_and_valid_xml() {
    for (lang, text) in LANGS {
        let (d, out) = translate_all(SRC.as_bytes(), lang, text, &MergeOptions::default());
        let o = String::from_utf8(out.clone()).unwrap();
        wellformed(&o).unwrap_or_else(|e| panic!("{lang}: {e}\n{o}"));
        let back = SvgFilter.extract(&out, &opts()).unwrap();
        assert_eq!(back.units.len(), d.units.len(), "{lang}");
        let want: Vec<String> = (0..d.units.len()).map(|i| format!("{text} {i}")).collect();
        assert_eq!(texts(&back), want, "{lang}");
        assert_eq!(styles(&back), styles(&d), "{lang}");
        assert!(o.contains(&format!("xml:lang=\"{lang}\"")), "{lang}: {o}");
        let rtl = matches!(lang, "ar-SA" | "he-IL");
        assert_eq!(o.contains("direction=\"rtl\""), rtl, "{lang}");
        // the text nodes only: the title and desc were translated, markup around them is intact
        assert!(o.contains("<title>") && o.contains("</desc>") && o.contains("<tspan>"));
    }
}

#[test]
fn rtl_changes_only_translated_text_elements() {
    let (_, out) = translate_all(SRC.as_bytes(), "ar", "\u{645}\u{631}\u{62d}\u{628}\u{627}", &MergeOptions::default());
    let o = String::from_utf8(out).unwrap();
    assert_eq!(o.matches("direction=\"rtl\"").count(), 2, "{o}");
    let second = o.lines().find(|l| l.contains("y=\"9\"")).unwrap();
    assert!(second.contains("text-anchor=\"start\"") && !second.contains("text-anchor=\"end\""), "{second}");
}

#[test]
fn untouched_merge_keeps_the_bytes_of_a_file_with_no_units_whose_code_page_is_not_one_to_one() {
    // gate 11-05: 0xED40 (a NEC-selected IBM extension) decodes to U+7E8A, which Shift_JIS encodes as 0xFA5C; with no
    // unit to hold the original bytes the merge used to write the other form
    let bytes = b"<?xml version=\"1.0\" encoding=\"shift_jis\"?>\n<svg><!-- \xed\x40 --><rect/></svg>".to_vec();
    let d = SvgFilter.extract(&bytes, &opts()).unwrap();
    assert!(d.units.is_empty());
    assert_eq!(SvgFilter.merge_with(&d, &MergeOptions::default()).unwrap(), bytes);
    assert_eq!(ImageFilter.merge_with(&ImageFilter.extract(&bytes, &opts()).unwrap(), &MergeOptions::default()).unwrap(), bytes);
    // a document that survives JSON keeps them too
    let json = serde_json::to_string(&d).unwrap();
    let back: Document = serde_json::from_str(&json).unwrap();
    assert_eq!(SvgFilter.merge(&back).unwrap(), bytes);
    // UTF-8 out and the always-stamp profile still write the file (as UTF-8, with the language)
    let d = SvgFilter.extract(&bytes, &with_target("ja-JP")).unwrap();
    let out = s(SvgFilter.merge_with(&d, &locos_opts()).unwrap());
    assert!(out.contains("<svg xml:lang=\"ja-JP\">") && out.contains("\u{7e8a}") && !out.to_ascii_lowercase().contains("shift_jis"), "{out}");
    // with a unit, the first unit holds them (unchanged behaviour)
    let bytes = b"<?xml version=\"1.0\" encoding=\"shift_jis\"?>\n<svg><!-- \xed\x40 --><text>Hi</text></svg>".to_vec();
    let d = SvgFilter.extract(&bytes, &opts()).unwrap();
    assert_eq!(SvgFilter.merge(&d).unwrap(), bytes);
}

#[test]
fn untouched_merge_is_the_original_bytes_in_every_encoding() {
    let utf16 = |bom: &[u8], f: fn(u16) -> [u8; 2]| -> Vec<u8> { bom.iter().copied().chain(SRC.encode_utf16().flat_map(f)).collect() };
    let mut cases: Vec<(&str, Vec<u8>)> = vec![
        ("utf-8", SRC.as_bytes().to_vec()),
        ("utf-8 bom", [b"\xef\xbb\xbf".as_slice(), SRC.as_bytes()].concat()),
        ("utf-16le", utf16(&[0xFF, 0xFE], u16::to_le_bytes)),
        ("utf-16be", utf16(&[0xFE, 0xFF], u16::to_be_bytes)),
        ("crlf", SRC.replace('\n', "\r\n").into_bytes()),
        ("cr", SRC.replace('\n', "\r").into_bytes()),
    ];
    for (label, enc_name, text) in [
        ("windows-1252", "windows-1252", "caf\u{e9}"),
        ("shift_jis", "shift_jis", "\u{65e5}\u{672c}\u{8a9e}"),
        ("gbk", "gbk", "\u{4e2d}\u{6587}"),
        ("euc-kr", "euc-kr", "\u{d55c}\u{ad6d}\u{c5b4}"),
        ("windows-1251", "windows-1251", "\u{43f}\u{440}\u{438}\u{432}\u{435}\u{442}"),
        ("windows-1256", "windows-1256", "\u{645}\u{631}\u{62d}\u{628}\u{627}"),
    ] {
        let doc = SRC.replace("UTF-8", label).replace("Hello &amp; welcome", &format!("{text} &amp; x"));
        let e = bilingua_model::encoding_rs::Encoding::for_label(enc_name.as_bytes()).unwrap();
        let (bytes, _, bad) = e.encode(&doc);
        assert!(!bad, "{label}");
        cases.push((label, bytes.into_owned()));
    }
    for (label, bytes) in cases {
        let d = SvgFilter.extract(&bytes, &opts()).unwrap_or_else(|e| panic!("{label}: {e}"));
        assert_eq!(d.units.len(), 5, "{label}");
        assert_eq!(SvgFilter.merge_with(&d, &MergeOptions::default()).unwrap(), bytes, "{label} untouched");
        // "UTF-8 out" is literal: a file in another encoding comes out as UTF-8 (a UTF-8 BOM is kept), same units
        let u = SvgFilter.merge_with(&d, &locos_opts()).unwrap();
        let body = u.strip_prefix(b"\xef\xbb\xbf".as_slice()).unwrap_or(&u);
        assert!(std::str::from_utf8(body).is_ok(), "{label}: UTF-8 out is not UTF-8");
        assert_eq!(texts(&SvgFilter.extract(&u, &opts()).unwrap()), texts(&d), "{label}");
        if std::str::from_utf8(&bytes).is_ok() {
            assert_eq!(u, bytes, "{label}");
        }
        // with a target language and nothing translated the bytes are kept, too
        let with_target = SvgFilter.extract(&bytes, &with_target("de-DE")).unwrap();
        assert_eq!(SvgFilter.merge(&with_target).unwrap(), bytes, "{label}: no translation, no language stamp");
    }
}

#[test]
fn legacy_encoding_is_kept_when_the_translation_fits_and_utf8_is_used_when_it_does_not() {
    let sjis = bilingua_model::encoding_rs::SHIFT_JIS.encode(&SRC.replace("UTF-8", "shift_jis").replace("Hello &amp; welcome", "\u{65e5}\u{672c}")).0.into_owned();
    let mut d = SvgFilter.extract(&sjis, &opts()).unwrap();
    set(&mut d, 0, "\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}");
    let kept = SvgFilter.merge(&d).unwrap();
    assert!(bilingua_model::encoding_rs::SHIFT_JIS.decode(&kept).0.contains("\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}") && String::from_utf8(kept.clone()).is_err());
    assert_eq!(SvgFilter.extract(&kept, &opts()).unwrap().units.len(), 5);
    set(&mut d, 0, "\u{645}\u{631}\u{62d}\u{628}\u{627}");
    let utf8 = SvgFilter.merge(&d).unwrap();
    let t = String::from_utf8(utf8).expect("falls back to UTF-8");
    assert!(t.contains("encoding=\"UTF-8\"") && t.contains("\u{645}\u{631}\u{62d}\u{628}\u{627}"), "{t}");
    // the LocOS option: always UTF-8, declaration rewritten
    let t = String::from_utf8(SvgFilter.merge_with(&d, &locos_opts()).unwrap()).unwrap();
    assert!(t.contains("encoding=\"UTF-8\""));
}

#[test]
fn bom_and_utf16_translated_merges_read_back() {
    let bom = [b"\xef\xbb\xbf".as_slice(), SRC.as_bytes()].concat();
    let (_, out) = translate_all(&bom, "de-DE", "Hallo", &MergeOptions::default());
    assert!(out.starts_with(b"\xef\xbb\xbf"));
    let u16le: Vec<u8> = [0xFFu8, 0xFE].into_iter().chain(SRC.encode_utf16().flat_map(u16::to_le_bytes)).collect();
    let (_, out) = translate_all(&u16le, "de-DE", "Hallo", &MergeOptions::default());
    assert!(out.starts_with(&[0xFF, 0xFE]));
    assert_eq!(texts(&SvgFilter.extract(&out, &opts()).unwrap()).len(), 5);
    let (_, out) = translate_all(&u16le, "de-DE", "Hallo", &locos_opts());
    assert!(String::from_utf8(out).is_ok(), "the LocOS option writes UTF-8");
}

#[test]
fn stamp_target_always_stamps_even_when_nothing_is_translated() {
    let d = SvgFilter.extract(SRC.as_bytes(), &with_target("fr-FR")).unwrap();
    assert_eq!(SvgFilter.merge(&d).unwrap(), SRC.as_bytes());
    let o = SvgFilter.merge_with(&d, &MergeOptions::new().with_stamp_target_always(true)).unwrap();
    assert!(String::from_utf8(o).unwrap().contains("xml:lang=\"fr-FR\""));
}

#[test]
fn paragraph_and_sentence_granularity_extract_the_same_units() {
    let a = SvgFilter.extract(SRC.as_bytes(), &ExtractOptions::new().with_granularity(Granularity::Sentence)).unwrap();
    let b = SvgFilter.extract(SRC.as_bytes(), &ExtractOptions::new().with_granularity(Granularity::Paragraph)).unwrap();
    assert_eq!(texts(&a), texts(&b));
    assert_eq!(a.units.iter().map(|u| u.id.clone()).collect::<Vec<_>>(), ["u1", "u2", "u3", "u4", "u5"]);
    assert_eq!(a.units[0].segments[0].id, "u1");
}

#[test]
fn merging_a_segmented_unit_joins_its_segments() {
    let mut d = extract(SRC.as_bytes());
    let u = &mut d.units[0];
    u.segments = vec![
        Segment::new("u1#1", vec![Inline::text("Hello &")]).with_target(vec![Inline::text("Hallo.")]).with_state(SegState::Draft),
        Segment::new("u1#2", vec![Inline::text("welcome")]).with_target(vec![Inline::text("Willkommen")]).with_state(SegState::Draft),
    ];
    u.separators = vec![" ".into()];
    let out = String::from_utf8(SvgFilter.merge(&d).unwrap()).unwrap();
    assert!(out.contains("Hallo. Willkommen"));
}

// ---- raster -----------------------------------------------------------------------------------------------------------

const REGIONS: &str = "[{\"text\": \"Sale\", \"bbox\": [10, 10, 80, 20], \"confidence\": 0.9, \"extra\": [1, 2]}, {\"text\": \"Shop now\", \"bbox\": [10, 40, 80, 20]}]";

#[test]
fn png_untouched_merge_is_the_original_and_a_translated_merge_rewrites_only_the_sidecar() {
    let img = png(Some(REGIONS), 120, 80, &[(b"tEXt", b"Comment\0keep me".to_vec())]);
    let d = image_extract(&img).unwrap();
    assert_eq!(ImageFilter.merge(&d).unwrap(), img);
    for (lang, text) in LANGS {
        let mut d = image_extract(&img).unwrap();
        for i in 0..d.units.len() {
            set(&mut d, i, &format!("{text} \"{i}\" \\ end"));
        }
        let out = ImageFilter.merge(&d).unwrap();
        let back = image_extract(&out).unwrap();
        let want: Vec<String> = (0..2).map(|i| format!("{text} \"{i}\" \\ end")).collect();
        assert_eq!(texts(&back), want, "{lang}");
        // geometry survives, the other chunks and the pixels are byte for byte
        assert_eq!(back.units[0].context.location, d.units[0].context.location);
        assert!(out.windows(14).any(|w| w == b"Comment\0keep m"));
        let pixels = |b: &[u8]| -> Vec<u8> { b.windows(4).position(|w| w == b"IDAT").map(|p| b[p..].to_vec()).unwrap() };
        assert_eq!(pixels(&out), pixels(&img), "{lang}");
        // the sidecar chunk is plain ASCII (Latin-1 safe) and a second merge works on the output
        let again = ImageFilter.merge(&back).unwrap();
        assert_eq!(again, out);
    }
}

#[test]
fn png_partial_translation_keeps_the_other_regions() {
    let img = png(Some(REGIONS), 120, 80, &[]);
    let mut d = image_extract(&img).unwrap();
    set(&mut d, 1, "Jetzt kaufen");
    let out = ImageFilter.merge(&d).unwrap();
    assert_eq!(texts(&image_extract(&out).unwrap()), ["Sale", "Jetzt kaufen"]);
}

#[test]
fn png_itxt_and_ztxt_sidecars() {
    // iTXt stays iTXt and keeps its language fields
    let mut it = b"loc_tms_regions\0\0\0en\0regions\0".to_vec();
    it.extend_from_slice("[{\"text\": \"Caf\u{e9} \u{65e5}\u{672c}\", \"bbox\": [1, 2, 3, 4]}]".as_bytes());
    let img = png(None, 30, 30, &[(b"iTXt", it)]);
    let mut d = image_extract(&img).unwrap();
    assert_eq!(texts(&d), ["Caf\u{e9} \u{65e5}\u{672c}"]);
    set(&mut d, 0, "\u{645}\u{631}\u{62d}\u{628}\u{627}");
    let out = ImageFilter.merge(&d).unwrap();
    assert!(out.windows(4).any(|w| w == b"iTXt") && out.windows(7).any(|w| w == b"regions"));
    assert_eq!(texts(&image_extract(&out).unwrap()), ["\u{645}\u{631}\u{62d}\u{628}\u{627}"]);
    // zTXt is read and written back as tEXt
    let mut z = b"loc_tms_regions\0\0".to_vec();
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    std::io::Write::write_all(&mut e, b"[{\"text\": \"zipped\", \"bbox\": [1, 2, 3, 4]}]").unwrap();
    z.extend(e.finish().unwrap());
    let img = png(None, 30, 30, &[(b"zTXt", z)]);
    let mut d = image_extract(&img).unwrap();
    assert_eq!(texts(&d), ["zipped"]);
    set(&mut d, 0, "gepackt");
    let out = ImageFilter.merge(&d).unwrap();
    assert!(!out.windows(4).any(|w| w == b"zTXt"));
    assert_eq!(texts(&image_extract(&out).unwrap()), ["gepackt"]);
}

#[test]
fn png_last_sidecar_wins_and_is_the_one_rewritten() {
    let mut second = b"loc_tms_regions\0".to_vec();
    second.extend_from_slice(b"[{\"text\": \"second\", \"bbox\": [1, 2, 3, 4]}]");
    let img = png(Some("[{\"text\": \"first\", \"bbox\": [1, 2, 3, 4]}]"), 30, 30, &[(b"tEXt", second)]);
    let mut d = image_extract(&img).unwrap();
    assert_eq!(texts(&d), ["second"]);
    set(&mut d, 0, "zweite");
    let out = ImageFilter.merge(&d).unwrap();
    assert_eq!(texts(&image_extract(&out).unwrap()), ["zweite"]);
    assert!(out.windows(5).any(|w| w == b"first"), "the shadowed chunk is untouched");
}

#[test]
fn image_filter_reads_svg_like_the_svg_filter() {
    let a = ImageFilter.extract(SRC.as_bytes(), &opts()).unwrap();
    assert_eq!(a.format, "svg");
    assert_eq!(texts(&a), texts(&extract(SRC.as_bytes())));
    assert_eq!(ImageFilter.merge(&a).unwrap(), SRC.as_bytes());
}

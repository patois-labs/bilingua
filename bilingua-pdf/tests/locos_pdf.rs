//! The Localization OS suite `tests/verify_pdf.py`, ported. Its fixtures are built by the LocOS writer (typst) and
//! stored in `tests/data` (`tools/gen_fixtures.py`); the cases that check the writer's rendering (ink, tofu,
//! counterfactual font config) or LocOS's OCR spend ledger have no Bilingua side and are listed in the README.

mod common;
use bilingua_model::*;
use bilingua_pdf::{ocr_render_scale, page_texts, Pdf, OCR_MAX_PIXEL_DIM};
use common::*;
use std::collections::HashMap;

/// verify_pdf `_norm`: NFC is not needed here (the fixtures are NFC), whitespace collapsed.
fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// verify_pdf `_chars`: the multiset of non-space characters.
fn chars(s: &str) -> HashMap<char, usize> {
    let mut m = HashMap::new();
    for c in s.chars().filter(|c| !c.is_whitespace()) {
        *m.entry(c).or_insert(0) += 1;
    }
    m
}

fn missing(want: &str, got: &str) -> Vec<char> {
    let g = chars(got);
    chars(want).into_iter().filter(|(c, n)| g.get(c).copied().unwrap_or(0) < *n).map(|(c, _)| c).collect()
}

/// main(): the digital PDF extracts directly, no OCR.
#[test]
fn digital_pdf_extracts_without_ocr() {
    let d = ex(&data("typst_digital.pdf")).unwrap();
    let all = texts(&d).join(" ");
    assert!(all.contains("digital PDF") && all.contains("Second paragraph"), "{all}");
    assert!(d.warnings.is_empty());
}

/// main(): a scanned PDF takes the OCR path (here: no units, the page listed for OCR).
#[test]
fn scanned_pdf_is_listed_for_ocr() {
    let d = ex(&data("typst_scanned.pdf")).unwrap();
    assert!(d.units.is_empty());
    assert_eq!(d.warnings.len(), 1);
    assert_eq!(d.warnings[0].kind, "pdf_ocr_page");
    assert_eq!(d.warnings[0].part, "page 1");
}

/// test_scanned_hands_png_to_ocr: the scanned page is an A4 page that LocOS renders at 150 dpi (about 1240 px).
#[test]
fn scanned_page_is_a4_at_150_dpi() {
    let p = page_texts(&data("typst_scanned.pdf"), &ExtractOptions::default()).unwrap();
    assert!(p[0].ocr);
    let scale = ocr_render_scale(p[0].width, p[0].height, OCR_MAX_PIXEL_DIM).unwrap();
    let w = (p[0].width * scale).ceil();
    assert!((1180.0..=1300.0).contains(&w), "{w}");
}

/// test_parse_thread_safety: concurrent extractions of a mixed text + scanned document all give the same result.
#[test]
fn concurrent_extraction_is_consistent() {
    let doc = data("typst_mixed_scan.pdf");
    let base = texts(&ex(&doc).unwrap());
    assert!(!base.is_empty());
    std::thread::scope(|s| {
        let hs: Vec<_> = (0..8).map(|_| s.spawn(|| (0..25).all(|_| texts(&ex(&doc).unwrap()) == base))).collect();
        for h in hs {
            assert!(h.join().unwrap());
        }
    });
    let p = page_texts(&doc, &ExtractOptions::default()).unwrap();
    assert_eq!(p.iter().map(|p| p.ocr).collect::<Vec<_>>(), vec![false, true]);
}

/// test_parse_page_at_a_time_memory: a 60-page scan; every page goes to OCR and nothing is held per page here.
#[test]
fn sixty_page_scan_is_sixty_ocr_pages() {
    let img = "q 595 0 0 842 0 0 cm /Im1 Do Q";
    let mut objs = vec![common::O::Plain("<</Type /Catalog /Pages 2 0 R>>".into())];
    let kids: Vec<String> = (0..60).map(|i| format!("{} 0 R", 4 + 2 * i)).collect();
    objs.push(common::O::Plain(format!("<</Type /Pages /Kids [{}] /Count 60>>", kids.join(" "))));
    objs.push(common::O::Stream("/Type /XObject /Subtype /Image /Width 2 /Height 2 /ColorSpace /DeviceGray /BitsPerComponent 8".into(), vec![0, 255, 255, 0]));
    for i in 0..60 {
        objs.push(common::O::Plain(format!("<</Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources <</XObject <</Im1 3 0 R>>>> /Contents {} 0 R>>", 5 + 2 * i)));
        objs.push(common::O::Stream(String::new(), img.as_bytes().to_vec()));
    }
    let d = ex(&pdf(&objs)).unwrap();
    assert!(d.units.is_empty());
    assert_eq!(d.warnings.len(), 60);
}

/// test_escape_is_load_bearing: each escape case, built by LocOS's writer, reads back verbatim (whitespace
/// collapsed, never deleted).
#[test]
fn escape_cases_read_back_verbatim() {
    let cases = [
        "Run the tool with --verbose --help to see all flags.",
        "Values in the range 10--20 are valid; see the docs...",
        "well--known --- ... -? co-?operate --> <-- 1--2",
        "typst meta: #call *bold* _emph_ `code` [group] $math$ @ref ~tilde // comment",
        "path/with/slashes and quotes 'x' \"y\" and a lone backslash \\ end",
        "<b>&</b> and #read(\"/x\") #include \"y\" #import \"z\" stay literal",
        "- leading dash not a bullet",
        "+ leading plus not an enum",
        "= leading equals not a heading",
        "  - spaces then dash",
        "\t+ tab then plus",
        "  = spaced heading",
        "1. leading enum stays",
        "1.5 million (decimal)",
        "WORD1\tWORD2\tWORD3 must not fuse (tab -> space)",
        "col1\tcol2\tcol3 tabbed values stay separated",
    ];
    for (i, s) in cases.iter().enumerate() {
        let p = page_texts(&data(&format!("typst_escape_{i:02}.pdf")), &ExtractOptions::default()).unwrap();
        assert_eq!(norm(s), norm(&p[0].text), "escape case {i}");
    }
}

/// test_build_reparses_to_same_paragraphs.
#[test]
fn rebuilt_pdf_reparses_to_its_paragraphs() {
    let d = ex(&data("typst_reparse.pdf")).unwrap();
    let all = norm(&texts(&d).join(" "));
    for p in [
        "The quick brown fox jumps over the lazy dog.",
        "A second paragraph with distinct wording for the check.",
        "Troisieme paragraphe avec des accents: eleve, francais.",
    ] {
        assert!(all.contains(p), "{p} not in {all}");
    }
}

/// test_pagination: the 120-paragraph document has more than one page, and every paragraph reads back.
#[test]
fn pagination_document_has_pages_and_paragraphs() {
    let p = page_texts(&data("typst_pagination.pdf"), &ExtractOptions::default()).unwrap();
    assert!(p.len() > 1);
    let all = norm(&p.iter().map(|p| p.text.as_str()).collect::<Vec<_>>().join("\n"));
    for i in 0..120 {
        assert!(all.contains(&format!("Paragraph {i}: enough words")), "{i}");
    }
}

const SCRIPTS: [(&str, &str); 14] = [
    ("ru", "Быстрая коричневая лиса перепрыгивает через ленивую собаку."),
    ("el", "Η γρήγορη καφέ αλεπού πηδάει πάνω από το τεμπέλικο σκυλί."),
    ("ja", "素早い茶色の狐が怠け者の犬を飛び越えます。"),
    ("zh-Hans", "敏捷的棕色狐狸跳过了懒狗。"),
    ("ko", "빠른 갈색 여우가 게으른 개를 뛰어넘습니다."),
    ("ar", "الثعلب البني السريع يقفز فوق الكلب الكسول."),
    ("he", "השועל החום המהיר קופץ מעל הכלב העצלן."),
    ("hi", "तेज़ भूरी लोमड़ी आलसी कुत्ते के ऊपर कूद जाती है।"),
    ("bn", "দ্রুত বাদামী শিয়াল অলস কুকুরের উপর দিয়ে লাফ দেয়।"),
    ("th", "สุนัขจิ้งจอกสีน้ำตาลว่องไวกระโดดข้ามสุนัขขี้เกียจ"),
    ("fr", "Le vif renard brun saute par-dessus le chien paresseux, à l'été déjà."),
    ("de", "Der schnelle braune Fuchs springt über den faulen Hund. Größe: ß."),
    ("pl", "Szybki brązowy lis przeskakuje nad leniwym psem. Zażółć gęślą jaźń."),
    ("es", "El veloz zorro marrón salta sobre el perro perezoso. ¿Señor? ¡Sí!"),
];

/// test_per_script_render, extraction half: every script's characters come back (multiset containment, as the
/// suite checks; PDFium reverses RTL runs and reorders Indic and Thai marks).
#[test]
fn every_script_round_trips_its_characters() {
    for (code, text) in SCRIPTS {
        let p = page_texts(&data(&format!("typst_lang_{code}.pdf")), &ExtractOptions::default()).unwrap();
        let m = missing(text, &p[0].text);
        assert!(m.is_empty(), "{code}: missing {m:?}");
    }
}

/// test_multiscript_mixed_document: all the scripts in one document.
#[test]
fn multiscript_document_round_trips() {
    let p = page_texts(&data("typst_multiscript.pdf"), &ExtractOptions::default()).unwrap();
    let all: String = p.iter().map(|p| p.text.as_str()).collect();
    for (code, text) in SCRIPTS {
        assert!(missing(text, &all).is_empty(), "{code}");
    }
}

/// main(): export. LocOS re-typesets a translated PDF; Bilingua leaves that to the caller (see DESIGN-MERGE.md):
/// an untouched merge is the original file, a translated one is refused with a typed error.
#[test]
fn export_untouched_is_original_translated_is_unsupported() {
    let bytes = data("typst_digital.pdf");
    let mut d = ex(&bytes).unwrap();
    assert_eq!(Pdf.merge(&d).unwrap(), bytes);
    let id = d.units[0].id.clone();
    d.set_target(&id, vec![Inline::text("Texte traduit.")]).unwrap();
    assert!(matches!(Pdf.merge(&d), Err(Error::Unsupported(_))));
}

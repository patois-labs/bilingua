//! The LocOS `verify_rst_filter` suite, ported case by case (9 test functions there, 9 here). Fixtures are the
//! suite's own hand-written documents, kept under `data/` and repeated inline where the suite builds them.
mod common;
use bilingua_model::*;
use bilingua_rst::{display_width, Rst};
use common::*;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

const DOC: &str = include_str!("data/doc.rst");

fn tokens(t: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = t;
    while let Some(a) = rest.find('\u{27E6}') {
        let Some(b) = rest[a..].find('\u{27E7}') else { break };
        out.push(rest[a..a + b + 3].to_string());
        rest = &rest[a + b + 3..];
    }
    out
}

#[test]
fn blocks_and_identity() {
    let data = DOC.as_bytes();
    let got = units(data);
    let want = s(&[
        "Project Guide",
        "Introduction",
        "This is a \u{27E6}g1\u{27E7}simple\u{27E6}/g1\u{27E7} paragraph with \u{27E6}x2\u{27E7} and a \u{27E6}x3\u{27E7} role, spanning two lines with \u{27E6}x4\u{27E7} and a \u{27E6}x5\u{27E7} and \u{27E6}g6\u{27E7}Docs\u{27E6}/g6\u{27E7}.",
        "Remember to save your work.",
        "A second note paragraph.",
        "Do not press the red button \u{27E6}x1\u{27E7}.",
        "Example",
        "First item",
        "Second item with \u{27E6}g1\u{27E7}bold\u{27E6}/g1\u{27E7} text",
        "Nested paragraph in item.",
        "Numbered one",
        "Numbered two",
        "Term",
        "Definition of the term.",
        "The footnote text.",
        "Line block one",
        "Line block two",
        "Fruit table",
        "Name",
        "Colour",
        "The figure caption.",
        "The new option.",
    ]);
    assert_eq!(got, want);
    let crlf = DOC.replace('\n', "\r\n").into_bytes();
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend_from_slice(data);
    for body in [data.to_vec(), crlf, bom] {
        assert_identity(&body);
    }
}

#[test]
fn translated_round_trip() {
    let data = DOC.as_bytes();
    let src = units(data);
    let tr: Vec<(usize, String)> = src.iter().enumerate().map(|(i, t)| (i, format!("DE {t}"))).collect();
    let out = String::from_utf8(try_build(data, &tr).unwrap()).unwrap();
    assert!(out.contains("================\nDE Project Guide\n================\n"), "{out}");
    assert!(out.contains("DE Introduction\n===============\n"), "{out}");
    assert!(
        out.contains("DE This is a *simple* paragraph with ``code`` and a :ref:`label <x>` role, spanning two lines with |sub| and a link_ and `Docs <https://example.com>`_.\n"),
        "{out}"
    );
    assert!(out.contains(".. note:: DE Remember to save your work.\n"));
    assert!(out.contains("DE Example::\n\n    literal text\n    not translated\n"));
    assert!(out.contains("- DE Second item with **bold** text\n\n  DE Nested paragraph in item.\n"));
    assert!(out.contains(".. code-block:: python\n\n   print(\"hello\")\n"));
    assert!(out.contains(".. include:: other.rst\n") && out.contains(":Author: Jane Doe\n"));
    assert!(out.contains("   * - DE Name\n     - DE Colour\n"), "{out}");
    assert!(out.contains(".. versionadded:: 2.1\n   DE The new option.\n"), "{out}");
    let back = units(out.as_bytes());
    let want: Vec<String> = tr.iter().map(|(_, t)| t.clone()).collect();
    assert_eq!(back, want);
}

/// #814.1: a token in a translation that names no code of the unit is typed text. It is kept between words, next to
/// escaped characters and in a title, and never deletes anything else.
#[test]
fn unknown_token_is_kept_as_text() {
    let tok = "\u{27E6}x77\u{27E7}";
    let out = build_str(b"Plain words here.\n", &[(0, &format!("Words {tok} *star* {tok}"))]);
    assert_eq!(out, format!("Words {tok} \*star\* {tok}
"));
    // a closing-form token, a pair token with no pair and a number no code has
    for t in ["\u{27E6}/g5\u{27E7}", "\u{27E6}g5\u{27E7}", "\u{27E6}x9999\u{27E7}"] {
        assert_eq!(build_str(b"Text here.\n", &[(0, &format!("Ein {t} Wort"))]), format!("Ein {t} Wort\n"), "{t}");
    }
    // a title keeps it
    let out = build_str(b"Title\n=====\n\nBody.\n", &[(0, &format!("Titel {tok}"))]);
    assert!(out.starts_with(&format!("Titel {tok}\n")), "{out}");
    // a real code still maps, beside the unknown one
    let out = build_str(b"A **bold** word.\n", &[(0, &format!("Ein \u{27E6}g1\u{27E7}fett\u{27E6}/g1\u{27E7} {tok}"))]);
    assert_eq!(out, format!("Ein **fett** {tok}\n"));
}

#[test]
fn inline_escaping_and_paragraph_start() {
    let data = b"Price is low.\n\nSecond para.\n";
    let out = build(data, &[(0, "Use *all* the_ items | now"), (1, "- not a bullet")]);
    assert_eq!(out, b"Use \\*all\\* the\\_ items \\| now\n\n\\- not a bullet\n");
    assert_eq!(units(&out).len(), 2, "still two paragraphs, no list item");
    // a translation that drops a pair keeps the markup balanced; an unknown token stays as text (#814.1)
    let data = b"A **bold** word.\n";
    let out = build(data, &[(0, "Ein \u{27E6}g1\u{27E7}fettes\u{27E6}/g1\u{27E7} Wort \u{27E6}x9\u{27E7}.")]);
    assert_eq!(out, "Ein **fettes** Wort \u{27E6}x9\u{27E7}.\n".as_bytes());
    // gate: a lone close is dropped, an open the translation never closes is closed
    assert_eq!(build(data, &[(0, "Ein \u{27E6}g1\u{27E7}fettes Wort.")]), b"Ein **fettes Wort.**\n");
    assert_eq!(build(data, &[(0, "Ein fettes\u{27E6}/g1\u{27E7} Wort.")]), b"Ein fettes Wort.\n");
}

#[test]
fn cjk_inline_boundaries() {
    // #695: markup glued to CJK letters gets an escaped space ("\ ") between a letter and the markup
    let data = b"Run ``make`` and read *this* and **that** and :ref:`intro` now.\n";
    let src = units(data).remove(0);
    let toks = tokens(&src);
    assert_eq!(toks.len(), 6, "{src}");
    let ja = format!("\u{307E}\u{305A}{}\u{3092}{}\u{3053}\u{308C}{}\u{3068}{}\u{3042}\u{308C}{}\u{3068}{}\u{3002}", toks[0], toks[1], toks[2], toks[3], toks[4], toks[5]);
    let out = build_str(data, &[(0, &ja)]);
    let want = "\u{307E}\u{305A}\\ ``make``\\ \u{3092}\\ *\u{3053}\u{308C}*\\ \u{3068}\\ **\u{3042}\u{308C}**\\ \u{3068}\\ :ref:`intro`\u{3002}\n";
    assert_eq!(out, want);
    // every markup is still markup on re-parse (the escaped spaces are point tokens)
    let again = units(out.as_bytes()).remove(0);
    assert!(again.contains("\u{27E6}g") && !again.contains("``") && !again.contains('*'), "{again}");
    // punctuation is a boundary: no escape. A Latin translation keeps the source bytes.
    let ja2 = format!("\u{FF08}{}\u{3001}{}\u{3053}\u{308C}{}\u{3002}", toks[0], toks[1], toks[2]);
    assert_eq!(build_str(data, &[(0, &ja2)]), "\u{FF08}``make``\u{3001}*\u{3053}\u{308C}*\u{3002}\n");
    let de = src.replace("Run", "Starte").replace(" now", " jetzt");
    let want_de = String::from_utf8_lossy(data).replace("Run", "Starte").replace(" now", " jetzt").into_bytes();
    assert_eq!(build(data, &[(0, &de)]), want_de);
}

#[test]
fn title_widths() {
    let data = b"Title\n=====\n\nBody text.\n";
    assert_eq!(build_str(data, &[(0, "\u{65E5}\u{672C}\u{8A9E}\u{306E}\u{898B}\u{51FA}\u{3057}")]), "\u{65E5}\u{672C}\u{8A9E}\u{306E}\u{898B}\u{51FA}\u{3057}\n==============\n\nBody text.\n");
    assert_eq!(display_width("\u{65E5}\u{672C}\u{8A9E}\u{306E}\u{898B}\u{51FA}\u{3057}"), 14);
    let data = b"########\n Inset \n########\n\nText.\n";
    assert_eq!(build_str(data, &[(0, "Longer inset")]), "###############\n Longer inset \n###############\n\nText.\n");
    let data = b"Long underline\n==============================\n";
    // the extra 16 columns are kept
    assert_eq!(build_str(data, &[(0, "Kurz")]), "Kurz\n====================\n");
    assert_eq!(units(b"Kurz\n====================\n"), s(&["Kurz"]));
}

const GRID: &[u8] = include_bytes!("data/grid.rst");

fn fit(data: &[u8], tr: &[(usize, &str)]) -> Result<String, String> {
    let v: Vec<(usize, String)> = tr.iter().map(|(i, t)| (*i, t.to_string())).collect();
    match try_build(data, &v) {
        Ok(b) => Ok(String::from_utf8(b).unwrap()),
        Err(Error::Write(m)) => Err(m),
        Err(e) => panic!("unexpected error kind: {e}"),
    }
}

#[test]
fn grid_tables() {
    assert_eq!(units(GRID), s(&["Header 1", "Header 2", "Cell one", "Cell two", "After."]));
    assert_identity(GRID);
    let out = fit(GRID, &[(0, "Kopf"), (3, "Zelle zwei ist lang")]).unwrap();
    assert_eq!(
        out,
        "+------------+------------+\n| Kopf       | Header 2   |\n+============+============+\n| Cell one   | Zelle zwei |\n|            | ist lang   |\n+------------+------------+\n\nAfter.\n"
    );
    assert_eq!(units(out.as_bytes())[..4], s(&["Kopf", "Header 2", "Cell one", "Zelle zwei ist lang"])[..]);
    let out = fit(GRID, &[(2, "\u{8868}\u{306E}\u{30BB}\u{30EB}")]).unwrap();
    assert!(out.contains("| \u{8868}\u{306E}\u{30BB}\u{30EB}   |"), "{out}");
    let e = fit(GRID, &[(2, "Donaudampfschifffahrt")]).unwrap_err();
    assert!(e.contains("line 4") && e.contains("Donaudampfschifffahrt"), "{e}");
    let span = include_bytes!("data/span.rst");
    assert_eq!(units(span), s(&["A", "B", "C"]));
    let out = fit(span, &[(0, "eins zwei")]).unwrap();
    assert_eq!(out.lines().skip(1).take(2).collect::<Vec<_>>(), ["| eins  | B     |", "| zwei  |       |"], "{out}");
    assert_eq!(units(out.as_bytes()), s(&["eins zwei", "B", "C"]), "{out}");
    let stairs = include_bytes!("data/stairs.rst");
    assert_eq!(units(stairs), s(&["A", "B", "C", "D"]));
    let e = fit(stairs, &[(0, "eins zwei drei")]).unwrap_err();
    assert!(e.contains("cannot grow"), "{e}");
    let out = fit(span, &[(1, "bb cc dd")]).unwrap();
    assert_eq!(out.lines().skip(1).take(3).collect::<Vec<_>>(), ["| A     | bb cc |", "+-------+ dd    +", "| C     |       |"], "{out}");
    let broken = include_bytes!("data/broken.rst");
    let doc = extract(broken).unwrap();
    assert!(doc.units.is_empty());
    let w: Vec<&Warning> = doc.warnings.iter().filter(|w| w.kind == "rst_skipped_table").collect();
    assert_eq!(w.len(), 1);
    assert_eq!(w[0].part, "line 1");
}

const SIMPLE: &[u8] = include_bytes!("data/simple.rst");

#[test]
fn simple_tables() {
    assert_eq!(units(SIMPLE), s(&["Name", "Value", "Alpha", "First", "Beta", "Second line"]));
    assert_identity(SIMPLE);
    let out = fit(SIMPLE, &[(1, "Wert"), (5, "Zweite"), (3, "Erste Zeile hier")]).unwrap();
    assert_eq!(out, "=====  ========\nName   Wert\n=====  ========\nAlpha  Erste Zeile hier\nBeta   Zweite\n=====  ========\n");
    let e = fit(SIMPLE, &[(2, "Alphabet")]).unwrap_err();
    assert!(e.contains("first column"), "{e}");
    let three = include_bytes!("data/three.rst");
    let out = fit(three, &[(4, "wide words")]).unwrap();
    assert_eq!(out.lines().skip(3).take(2).collect::<Vec<_>>(), ["x      wide   z", "       words"], "{out}");
    let gap = include_bytes!("data/gap.rst");
    let doc = extract(gap).unwrap();
    let w = doc.warnings.iter().find(|w| w.kind == "rst_skipped_table").unwrap();
    assert!(w.message.contains("gap"), "{}", w.message);
}

#[test]
fn hostile() {
    // binary is refused
    let r = extract(b"\x89PNG\r\n\x1a\n\x00\x00\x81\x8d\x8f\x90binary");
    assert!(matches!(r, Err(Error::Encoding(_))), "{r:?}");
    // the segment cap
    let data: Vec<u8> = (0..30).flat_map(|i| format!("Paragraph {i}.\n\n").into_bytes()).collect();
    let limits = Limits::new().with_max_units(10);
    let r = Rst.extract(&data, &ExtractOptions::new().with_limits(limits));
    assert!(matches!(r, Err(Error::Limit(_))), "over-cap file must be refused");
    // deep nesting
    let deep: Vec<u8> = (0..200).flat_map(|k| format!("{}- item\n\n", "  ".repeat(k)).into_bytes()).collect();
    extract(&deep).unwrap();
    // an oversized grid is kept verbatim
    let mut big = String::from("+");
    big.push_str(&"-+".repeat(1500));
    big.push('\n');
    for _ in 0..300 {
        big.push('|');
        big.push_str(&" |".repeat(1500));
        big.push('\n');
    }
    big.push('+');
    big.push_str(&"-+".repeat(1500));
    big.push('\n');
    let doc = extract(big.as_bytes()).unwrap();
    assert!(doc.warnings.iter().any(|w| w.kind == "rst_skipped_table"), "an oversized grid is kept verbatim");
    // pathological inline text
    let mut patho = "*".repeat(20000);
    patho.push_str("\n\n");
    patho.push_str(&"`".repeat(20000));
    patho.push_str("\n\n");
    patho.push_str(&"|a ".repeat(5000));
    patho.push('\n');
    extract(patho.as_bytes()).unwrap();
    // unclosed simple tables: linear end search
    extract(&b"=====  =====\nx\n\n".repeat(30000)).unwrap();
}

const META_DOC: &str = include_str!("data/meta_doc.rst");

#[test]
fn meta_directive_733() {
    // `.. meta::` description / keywords values are units appended after every body unit (body indexes
    // unchanged); other meta fields stay; written back on one line with rST escaping; no-op builds identical.
    let data = META_DOC.as_bytes();
    let doc = extract(data).unwrap();
    let all = units(data);
    let st = styles(data);
    let body: Vec<(usize, String)> = all.iter().cloned().enumerate().filter(|(i, _)| st[*i].is_empty()).collect();
    let plain = META_DOC.replace(":description: Learn", ":x-a: Learn").replace(":keywords: setup", ":x-b: setup").replace(":Keywords:", ":x-c:");
    let plain_units: Vec<(usize, String)> = units(plain.as_bytes()).into_iter().enumerate().collect();
    assert_eq!(body, plain_units);
    let n = body.len();
    let meta: Vec<(usize, String, String)> = all.iter().enumerate().filter(|(i, _)| !st[*i].is_empty()).map(|(i, t)| (i, t.clone(), st[i].clone())).collect();
    assert_eq!(
        meta,
        vec![
            (n, "Learn how to set up \u{27E6}x1\u{27E7} in five minutes.".to_string(), "meta:description".to_string()),
            (n + 1, "setup, onboarding".to_string(), "meta:keywords".to_string()),
            (n + 2, "wrapped, over two lines".to_string(), "meta:keywords".to_string()),
        ]
    );
    assert_identity(data);
    assert_eq!(doc.units.len(), all.len());
    let out = build_str(
        data,
        &[
            (n, "- Apprenez *vite* \u{27E6}x1\u{27E7}\nen cinq minutes"),
            (n + 1, "installation, prise en main"),
            (n + 2, "enveloppe, sur deux lignes"),
        ],
    );
    assert!(out.contains("   :description: \\- Apprenez \\*vite\\* |product| en cinq minutes\n"), "{out}");
    assert!(out.contains("   :keywords: installation, prise en main\n   :robots: noindex\n"), "{out}");
    assert!(out.contains("   :Keywords:\n      enveloppe, sur deux lignes\n   :description: 12345\n"), "{out}");
    for kept in [":description lang=fr: Deja traduit", ":http-equiv=Content-Type: text/html"] {
        assert!(out.contains(kept), "{kept}");
    }
    assert_eq!(units(out.as_bytes()).len(), all.len());
}

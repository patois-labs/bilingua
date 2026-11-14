//! Round trips: untouched merge = the original bytes; a pseudo-translated merge reads back with the same units and
//! codes; the target locale is written; 11 languages; the legacy encodings LocOS reads.
mod common;
use bilingua_model::encoding_rs;
use bilingua_model::*;
use bilingua_serverpages::Dialect;
use common::*;

const LANGS: [(&str, &str, &str); 11] = [
    ("en", "Welcome back", "en"),
    ("ar", "\u{645}\u{631}\u{62d}\u{628}\u{627} \u{628}\u{643} \u{645}\u{631}\u{629} \u{623}\u{62e}\u{631}\u{649}", "ar"),
    ("ja", "\u{304a}\u{5e30}\u{308a}\u{306a}\u{3055}\u{3044}", "ja"),
    ("zh", "\u{6b22}\u{8fce}\u{56de}\u{6765}", "zh-Hans"),
    ("ko", "\u{b2e4}\u{c2dc} \u{c624}\u{c2e0} \u{ac83}\u{c744} \u{d658}\u{c601}\u{d569}\u{b2c8}\u{b2e4}", "ko"),
    ("he", "\u{5d1}\u{5e8}\u{5d5}\u{5da} \u{5e9}\u{5d5}\u{5d1}\u{5da}", "he"),
    ("ru", "\u{421} \u{432}\u{43e}\u{437}\u{432}\u{440}\u{430}\u{449}\u{435}\u{43d}\u{438}\u{435}\u{43c}", "ru"),
    ("el", "\u{39a}\u{3b1}\u{3bb}\u{3ce}\u{3c2} \u{3ae}\u{3c1}\u{3b8}\u{3b1}\u{3c4}\u{3b5}", "el"),
    ("hi", "\u{935}\u{93e}\u{92a}\u{938}\u{940} \u{92a}\u{930} \u{938}\u{94d}\u{935}\u{93e}\u{917}\u{924} \u{939}\u{948}", "hi"),
    ("th", "\u{e22}\u{e34}\u{e19}\u{e14}\u{e35}\u{e15}\u{e49}\u{e2d}\u{e19}\u{e23}\u{e31}\u{e1a}\u{e01}\u{e25}\u{e31}\u{e1a}", "th"),
    ("de", "Willkommen zur\u{fc}ck \u{c4}\u{d6}\u{dc}\u{df}", "de"),
];

/// (output island, statement open, statement close)
fn code(fmt: &str) -> (&'static str, &'static str, &'static str) {
    match fmt {
        "phtml" => ("<?= $name ?>", "<?php if ($u): ?>", "<?php endif; ?>"),
        "jsp" => ("${user.name}", "<% if (u) { %>", "<% } %>"),
        "aspx" => ("<%: Model.Name %>", "<% if (u) { %>", "<% } %>"),
        "erb" => ("<%= name %>", "<% if u -%>", "<% end -%>"),
        _ => ("<!--#echo var=\"NAME\" -->", "<!--#include virtual=\"/a.html\" -->", "<!--#config timefmt=\"%Y\" -->"),
    }
}

fn page(fmt: &str, t: &str, nl: &str) -> String {
    let (out, open, close) = code(fmt);
    format!(
        "<!DOCTYPE html>{nl}<html lang=\"en\">{nl}<head><title>{t}</title></head>{nl}<body>{nl}<h1>{t}</h1>{nl}{open}{nl}<p>{t} {out} <b>{t}</b>, {t}.</p>{nl}{close}{nl}<img src=\"a.png\" alt=\"{t}\">{nl}<a href=\"{out}\" title=\"{t}\">{t}</a>{nl}</body>{nl}</html>{nl}"
    )
}

fn tag_kinds(d: &Document) -> Vec<Vec<(String, TagKind)>> {
    d.units
        .iter()
        .map(|u| {
            u.source()
                .iter()
                .filter_map(|i| match i {
                    Inline::Tag { id, kind, .. } => Some((id.clone(), *kind)),
                    _ => None,
                })
                .collect()
        })
        .collect()
}

fn code_strings(d: &Document) -> Vec<Vec<String>> {
    // the markup behind every code, with the pseudo-translation marks taken out of translated attribute values
    d.units
        .iter()
        .map(|u| {
            u.codes
                .values()
                .flat_map(|c| [c.open.clone(), c.close.clone(), c.standalone.clone()])
                .flatten()
                .map(|s| s.replace(['\u{ab}', '\u{bb}'], ""))
                .collect()
        })
        .collect()
}

/// Every unit translated to "«text»" with its own codes; the merged page reads back with the same units and codes.
fn pseudo(fmt: &str, data: &[u8]) -> Vec<u8> {
    let f = filter(fmt);
    let mut doc = f.extract(data, &opts()).unwrap();
    let ids: Vec<String> = doc.units.iter().map(|u| u.segments[0].id.clone()).collect();
    for (k, id) in ids.iter().enumerate() {
        let mut t = vec![Inline::text("\u{ab}")];
        t.extend(doc.units[k].source());
        t.push(Inline::text("\u{bb}"));
        doc.set_target(id, t).unwrap();
    }
    let out = f.merge(&doc).unwrap();
    let back = f.extract(&out, &opts()).unwrap();
    assert_eq!(back.units.len(), doc.units.len(), "{fmt}: unit count after the merge");
    assert_eq!(tag_kinds(&back), tag_kinds(&doc), "{fmt}: tags after the merge");
    assert_eq!(code_strings(&back), code_strings(&doc), "{fmt}: codes after the merge");
    let d: Dialect = Dialect::from_extension(f.extensions()[0]).unwrap();
    let a = std::str::from_utf8(data).map(|s| d.code_islands(s).unwrap());
    let b = d.code_islands(std::str::from_utf8(&out).unwrap()).unwrap();
    if let Ok(a) = a {
        assert_eq!(a, b, "{fmt}: code islands after the merge");
    }
    out
}

#[test]
fn eleven_languages_in_every_dialect() {
    for fmt in FORMATS {
        for (code_, text, locale) in LANGS {
            let src = page(fmt, text, "\n");
            let data = src.as_bytes();
            let doc = extract(fmt, data).unwrap_or_else(|e| panic!("{fmt} {code_}: {e}"));
            assert!(doc.units.len() >= 6, "{fmt} {code_}: {} units", doc.units.len());
            assert!(doc.units.iter().any(|u| locos_text(u).contains(text)), "{fmt} {code_}");
            assert_identity(fmt, data);
            let out = pseudo(fmt, data);
            let out = String::from_utf8(out).unwrap();
            assert!(out.contains(&format!("\u{ab}{text}\u{bb}")) || out.contains(&format!("\u{ab}{text} ")), "{fmt} {code_}: {out}");
            // the target locale: language (and direction) written
            let mut doc = extract(fmt, data).unwrap();
            doc.target_lang = Some(locale.to_string());
            let id = doc.units[0].segments[0].id.clone();
            doc.set_target(&id, vec![Inline::text("x")]).unwrap();
            let loc = String::from_utf8(filter(fmt).merge(&doc).unwrap()).unwrap();
            assert!(loc.contains(&format!("<html lang=\"{locale}\"")) || loc.contains(&format!("dir=\"rtl\" lang=\"{locale}\"")), "{fmt} {locale}: {loc}");
            assert_eq!(loc.contains("dir=\"rtl\""), matches!(locale, "ar" | "he"), "{fmt} {locale}");
        }
    }
}

#[test]
fn newlines_and_byte_order_marks_are_kept() {
    for fmt in FORMATS {
        for nl in ["\n", "\r\n", "\r"] {
            let src = page(fmt, "Hello there", nl);
            assert_identity(fmt, src.as_bytes());
            let out = String::from_utf8(pseudo(fmt, src.as_bytes())).unwrap();
            if nl != "\n" {
                assert!(!out.contains('\n') || nl == "\r\n", "{fmt}");
                assert_eq!(out.matches(nl).count(), src.matches(nl).count(), "{fmt}");
            }
        }
        let mut bom = vec![0xEF, 0xBB, 0xBF];
        bom.extend(page(fmt, "Hello there", "\n").as_bytes());
        assert_identity(fmt, &bom);
        let out = pseudo(fmt, &bom);
        assert!(out.starts_with(&[0xEF, 0xBB, 0xBF]), "{fmt}");
    }
}

fn encode(label: &str, s: &str) -> Vec<u8> {
    if label == "utf-16le" || label == "utf-16be" {
        let mut v = if label == "utf-16le" { vec![0xFF, 0xFE] } else { vec![0xFE, 0xFF] };
        for u in s.encode_utf16() {
            v.extend(if label == "utf-16le" { u.to_le_bytes() } else { u.to_be_bytes() });
        }
        return v;
    }
    let e = encoding_rs::Encoding::for_label(label.as_bytes()).unwrap();
    let (b, _, bad) = e.encode(s);
    assert!(!bad, "{label} cannot hold {s}");
    b.into_owned()
}

#[test]
fn legacy_encodings_are_read_and_written_back() {
    let cases: [(&str, &str, &str); 11] = [
        ("windows-1252", "Caf\u{e9} cr\u{e8}me", "D\u{e9}j\u{e0} vu"),
        ("windows-1251", "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442} \u{43c}\u{438}\u{440}", "\u{421}\u{43f}\u{430}\u{441}\u{438}\u{431}\u{43e}"),
        ("shift_jis", "\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30c6}\u{30ad}\u{30b9}\u{30c8}", "\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}"),
        ("gbk", "\u{4e2d}\u{6587}\u{6587}\u{672c}", "\u{4f60}\u{597d}\u{5417}"),
        ("euc-kr", "\u{d55c}\u{ad6d}\u{c5b4} \u{d14d}\u{c2a4}\u{d2b8}", "\u{c548}\u{b155}"),
        ("big5", "\u{4e2d}\u{6587}\u{6587}\u{672c}", "\u{4f60}\u{597d}\u{55ce}"),
        ("iso-8859-2", "P\u{159}\u{ed}li\u{161} \u{17e}lu\u{165}ou\u{10d}k\u{fd}", "D\u{11b}kuji"),
        ("koi8-r", "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}", "\u{41c}\u{438}\u{440}"),
        ("windows-1253", "\u{39a}\u{3b1}\u{3bb}\u{3b7}\u{3bc}\u{3ad}\u{3c1}\u{3b1}", "\u{393}\u{3b5}\u{3b9}\u{3b1}"),
        ("utf-16le", "\u{65e5}\u{672c} Caf\u{e9}", "\u{4e2d}\u{6587}"),
        ("utf-16be", "\u{645}\u{631}\u{62d}\u{628}\u{627}", "\u{5e9}\u{5dc}\u{5d5}\u{5dd}"),
    ];
    for fmt in FORMATS {
        for (label, text, new) in cases {
            let (out_island, open, close) = code(fmt);
            let declared = if label.starts_with("utf-16") { String::new() } else { format!("<meta charset=\"{label}\">") };
            let src = format!("<html><head>{declared}<title>{text}</title></head><body>{open}<p>{text} {out_island}</p>{close}</body></html>\n");
            let data = encode(label, &src);
            let doc = extract(fmt, &data).unwrap_or_else(|e| panic!("{fmt} {label}: {e}"));
            assert!(doc.units.iter().any(|u| locos_text(u).starts_with(text)), "{fmt} {label}: {:?}", doc.units.iter().map(locos_text).collect::<Vec<_>>());
            assert_eq!(filter(fmt).merge(&doc).unwrap(), data, "{fmt} {label}: untouched merge");
            // a translation that fits the encoding stays in it
            let mut d2 = doc.clone();
            let id = d2.units[1].segments[0].id.clone();
            let mut t = vec![Inline::text(new.to_string() + " ")];
            t.extend(d2.units[1].source().into_iter().filter(|i| matches!(i, Inline::Tag { .. })));
            d2.set_target(&id, t).unwrap();
            let out = filter(fmt).merge(&d2).unwrap();
            let again = extract(fmt, &out).unwrap();
            assert!(locos_text(&again.units[1]).starts_with(new), "{fmt} {label}: {:?}", locos_text(&again.units[1]));
            assert_ne!(out, data);
            if !label.starts_with("utf-16") {
                let e = encoding_rs::Encoding::for_label(label.as_bytes()).unwrap();
                let (dec, _, bad) = e.decode(&out);
                assert!(!bad && dec.contains(new), "{fmt} {label}: kept in its own encoding");
            }
            // a character outside the code page falls back to UTF-8 (the declaration says so) and still reads back
            if !label.starts_with("utf-16") && label != "gbk" && label != "big5" {
                let mut d3 = doc.clone();
                let id = d3.units[1].segments[0].id.clone();
                d3.set_target(&id, vec![Inline::text("\u{1f600} \u{2603} snow")]).unwrap();
                let out = filter(fmt).merge(&d3).unwrap();
                let s = String::from_utf8(out).unwrap_or_else(|_| panic!("{fmt} {label}: not UTF-8"));
                assert!(s.contains("\u{1f600} \u{2603} snow"), "{fmt} {label}");
            }
        }
    }
}

#[test]
fn undeclared_bytes_that_are_not_text_are_refused() {
    for fmt in FORMATS {
        let mut b = b"<p>Hello</p>".to_vec();
        b.extend([0u8, 1, 2, 3, 0xff, 0xfe, 0, 0, 0]);
        assert!(extract(fmt, &b).is_err(), "{fmt}");
    }
}

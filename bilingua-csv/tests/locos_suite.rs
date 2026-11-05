//! The LocOS suite `tests/verify_csv_filter.py`, case by case (the filter parts: registration, byte fidelity,
//! multilingual columns, formula injection, quoting, mapping and its errors, encodings, TSV and sniffing, the
//! Shopify fixture, hostile input, the ingest/export column plan). What belongs to other LocOS layers (the XLSX
//! column mode, the HTTP API and CLI, the glossary and TM importers) is not this crate's and is listed in the README.
mod common;
use bilingua_csv::{CsvFilter, TsvFilter};
use bilingua_model::*;
use common::*;

const MULTI: &[u8] = b"\xef\xbb\xbfkey;en;de;fr;notes\r\n\
hello;\"Hello; world\";;Bonjour;greeting\r\n\
qty;=\"0012\";;;\r\n\
ml;\"Line one\nline two\";;;x\r\n\
f;=SUM(A1);;;\r\n\
bad;Plus one;;;\r\n";

/// The standard call of the LocOS suite: source `en`, targets `de` and `fr`.
fn call(locale: Option<&str>) -> Call {
    Call::csv().langs("en", &["de", "fr"], locale)
}

fn tcall(locale: Option<&str>) -> Call {
    Call::tsv().langs("en", &["de", "fr"], locale)
}

/// A small RFC 4180 reader for the checks (the LocOS suite uses Python's csv module).
fn rows(bytes: &[u8], delim: char) -> Vec<Vec<String>> {
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut out = vec![];
    let (mut row, mut field, mut quoted, mut any) = (vec![], String::new(), false, false);
    let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        any = true;
        if quoted {
            if c == '"' {
                if it.peek() == Some(&'"') {
                    it.next();
                    field.push('"');
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
        } else if c == '"' {
            quoted = true;
        } else if c == delim {
            row.push(std::mem::take(&mut field));
        } else if c == '\n' || c == '\r' {
            if c == '\r' && it.peek() == Some(&'\n') {
                it.next();
            }
            row.push(std::mem::take(&mut field));
            out.push(std::mem::take(&mut row));
            any = false;
        } else {
            field.push(c);
        }
    }
    if any {
        row.push(field);
        out.push(row);
    }
    out
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn registered() {
    assert_eq!(CsvFilter::default().id(), "csv");
    assert_eq!(CsvFilter::default().extensions(), &["csv"]);
    assert_eq!(TsvFilter::default().id(), "tsv");
    assert_eq!(TsvFilter::default().extensions(), &["tsv", "tab"]);
}

#[test]
fn untouched_bytes_identical() {
    let utf16 = |bom: &[u8], s: &str, be: bool| {
        let mut v = bom.to_vec();
        for u in s.encode_utf16() {
            v.extend_from_slice(&if be { u.to_be_bytes() } else { u.to_le_bytes() });
        }
        v
    };
    let samples: Vec<Vec<u8>> = vec![
        MULTI.to_vec(),
        b"source,target\nOne,\nTwo,\n".to_vec(),
        b"source,target\r\nOne,\r\nTwo,".to_vec(), // no final newline
        b"id|text\n1|Alpha\n\n2|Beta\r\n".to_vec(), // pipe, blank line, mixed endings
        b"text\n\"quoted \"\"inner\"\" text\"\n".to_vec(),
        utf16(&[0xFF, 0xFE], "source\tde\nHallo Welt\t\n", false),
        utf16(&[0xFE, 0xFF], "source\tde\nHallo Welt\t\n", true),
    ];
    for data in &samples {
        for locale in [Some("de"), Some("fr"), None] {
            let c = call(locale);
            let doc = c.extract(data).unwrap();
            assert_eq!(c.merge(&doc).unwrap(), *data, "{data:?}");
            // every unit "translated" with its own text: identical unless a target column takes it
            let echo: Vec<String> = doc.units.iter().map(|u| plain_text(&u.source())).collect();
            let mapping: Vec<(usize, &str)> = echo.iter().enumerate().map(|(i, s)| (i, s.as_str())).collect();
            let out = c.build(data, &mapping);
            let has_targets = c.extract(data).unwrap().skeleton_has_target_column();
            assert!(out == *data || has_targets, "{data:?}");
        }
    }
}

/// True when a unit writes into a column other than the one it was read from.
trait HasTarget {
    fn skeleton_has_target_column(&self) -> bool;
}

impl HasTarget for Document {
    fn skeleton_has_target_column(&self) -> bool {
        self.units.iter().any(|u| {
            let l = u.codes.get("$csv").and_then(|c| c.label.clone()).unwrap_or_default();
            let p: Vec<&str> = l.split(':').collect();
            p.len() == 3 && p[1] != p[2]
        })
    }
}

#[test]
fn multilingual_columns() {
    let c = call(Some("de"));
    let doc = c.extract(MULTI).unwrap();
    let texts: Vec<String> = doc.units.iter().map(|u| plain_text(&u.source())).collect();
    assert_eq!(texts, ["Hello; world", "Line one\nline two", "Plus one"]);
    assert_eq!(doc.units[0].context.notes.join(" | "), "Key: hello | notes: greeting");
    assert_eq!(doc.units[0].context.key.as_deref(), Some("hello"));
    let de = c.build(MULTI, &[(0, "Hallo; Welt"), (1, "Zeile\neins"), (2, "Plus eins")]);
    let r = rows(&de, ';');
    let col2: Vec<&str> = r[1..].iter().map(|x| x[2].as_str()).collect();
    assert_eq!(col2, ["Hallo; Welt", "", "Zeile\neins", "", "Plus eins"]);
    let col3: Vec<&str> = r[1..].iter().map(|x| x[3].as_str()).collect();
    assert_eq!(col3, ["Bonjour", "", "", "", ""], "fr untouched");
    assert!(de.starts_with(&[0xEF, 0xBB, 0xBF]));
    // untouched records keep their exact bytes
    assert!(contains(&de, b"qty;=\"0012\";;;\r\n") && contains(&de, b"f;=SUM(A1);;;\r\n"));
    // fr: an untranslated row (target == source) keeps the existing Bonjour
    let fr = call(Some("fr")).build(MULTI, &[(0, "Hello; world"), (2, "Plus un")]);
    let r = rows(&fr, ';');
    assert_eq!(r[1][3], "Bonjour");
    assert_eq!(r[5][3], "Plus un");
    assert_eq!(r[1][2], "");
}

#[test]
fn formula_injection_guarded() {
    let c = call(Some("de"));
    for bad in ["=cmd|' /C calc'!A0", "+1+1", "-2+3", "@SUM(1)", "\tTab", "\u{ff1d}X"] {
        let out = c.build(MULTI, &[(2, bad)]);
        let r = rows(&out, ';');
        assert_eq!(r[5][2], format!("'{bad}"), "{bad:?}");
    }
    // in-place mode guards too
    let data = b"Name,Count\nWidget,5\n";
    let c = Call::csv().langs("en", &[], None);
    let doc = c.extract(data).unwrap();
    assert!(doc.units.iter().all(|u| !u.codes["$csv"].label.as_deref().unwrap().is_empty()));
    let out = c.build(data, &[(0, "=HYPERLINK(1)")]);
    assert!(contains(&out, b"'=HYPERLINK(1)"), "{out:?}");
}

#[test]
fn source_formula_cells_are_never_units() {
    let t = call(Some("de")).units(MULTI);
    assert!(!t.iter().any(|s| s.contains("SUM") || s.contains("0012")));
    // and are locked units only when asked for
    let o = ExtractOptions { include_locked: true, target_lang: Some("de".into()), source_lang: Some("en".into()), ..Default::default() };
    let doc = call(Some("de")).extract_with(MULTI, &o).unwrap();
    let locked: Vec<String> = doc.units.iter().filter(|u| !u.translate).map(|u| plain_text(&u.source())).collect();
    assert_eq!(locked, ["=\"0012\"", "=SUM(A1)"]);
    assert_eq!(doc.units.iter().filter(|u| u.translate).count(), 3);
}

#[test]
fn quoting_of_written_cells() {
    let data = b"source,target\nPlain,\n\"Was quoted\",\"\"\n";
    let c = Call::csv().langs("en", &["de", "fr"], Some("de"));
    let out = c.build(data, &[(0, "A, \"b\"\nc"), (1, "simple")]);
    let r = rows(&out, ',');
    assert_eq!(r[1], ["Plain", "A, \"b\"\nc"]);
    assert_eq!(r[2], ["Was quoted", "simple"]);
    assert!(contains(&out, b"\"Was quoted\",\"simple\"\n"), "an originally quoted field stays quoted");
}

#[test]
fn explicit_mapping_and_errors() {
    let data = b"ID,English,German\n1,Save,\n2,Open,\n";
    let c = Call::csv().cols("source=English, target.de=German, key=ID").langs("en-US", &["de-DE"], Some("de-DE"));
    let out = c.build(data, &[(0, "Speichern"), (1, "Offnen")]);
    assert_eq!(out, b"ID,English,German\n1,Save,Speichern\n2,Open,Offnen\n");
    let c = Call::csv().cols("header=no,source=2,target=3").langs("en", &[], Some("de"));
    assert_eq!(c.units(b"1,Save\n2,Open\n"), ["Save", "Open"]);
    // short rows are padded to the target column
    assert_eq!(c.build(b"1,Save\n2,Open\n", &[(0, "S"), (1, "O")]), b"1,Save,S\n2,Open,O\n");
    for bad in ["source=Nope", "target=German", "colour=red", "source", "header=maybe", "delimiter=x"] {
        let r = Call::csv().cols(bad).langs("en", &["de", "fr"], None).extract(data);
        assert!(matches!(r, Err(Error::Unsupported(_))), "spec {bad:?} was not refused: {r:?}");
    }
    let r = Call::csv().cols("source=English,target=English").langs("en", &["de", "fr"], None).extract(data);
    assert!(matches!(r, Err(Error::Unsupported(_))), "source == target accepted");
}

#[test]
fn encodings() {
    // Windows-1252
    let cp = b"source,de\nCaf\xe9 cr\xe8me,\n".to_vec();
    let c = call(Some("de"));
    let doc = c.extract(&cp).unwrap();
    assert_eq!(plain_text(&doc.units[0].source()), "Caf\u{e9} cr\u{e8}me");
    let Skeleton::Text { encoding, .. } = &doc.skeleton else { panic!() };
    assert!(encoding.starts_with("windows-1252"), "{encoding}");
    // B-bytes: a translation that fits Windows-1252 goes back in it (no BOM, as uploaded); one that does not
    // becomes UTF-8 with a BOM so Excel still reads it.
    let out = c.build(&cp, &[(0, "Kaffee")]);
    assert_eq!(out, b"source,de\nCaf\xe9 cr\xe8me,Kaffee\n");
    let out = c.build(&cp, &[(0, "\u{30b3}\u{30fc}\u{30d2}\u{30fc}")]);
    assert!(out.starts_with(&[0xEF, 0xBB, 0xBF]), "{out:?}");
    assert!(contains(&out, "Caf\u{e9}".as_bytes()));
    // binary is refused
    assert!(matches!(call(None).extract(b"source\ncaf\x81\x00\x00 x\n"), Err(Error::Encoding(_))));
    // UTF-16 stays UTF-16
    let mut u16 = vec![0xFF, 0xFE];
    for u in "source\tde\nHallo\t\n".encode_utf16() {
        u16.extend_from_slice(&u.to_le_bytes());
    }
    let c = Call::tsv().langs("de", &["en"], Some("en"));
    let out = c.build(&u16, &[(0, "Hello")]);
    assert!(out.starts_with(&[0xFF, 0xFE]));
    let back: Vec<u16> = out[2..].chunks(2).map(|p| u16::from_le_bytes([p[0], p[1]])).collect();
    assert!(String::from_utf16(&back).unwrap().contains("Hello"));
}

#[test]
fn tsv_and_sniffing() {
    let data = b"source\ttarget\nHello, world\t\n";
    let c = tcall(Some("de"));
    assert_eq!(c.units(data), ["Hello, world"]);
    assert_eq!(c.build(data, &[(0, "Hallo, Welt")]), b"source\ttarget\nHello, world\tHallo, Welt\n");
    // CSV sniffs the delimiter
    let doc = call(Some("de")).extract(b"source;target\nA, b;\n").unwrap();
    assert_eq!(doc.units[0].codes["$csv"].open.as_deref(), Some(";"));
    // a build with no mapping stamp must still read a TSV as TAB, not sniff the commas inside its cells
    let data = b"Red, green, blue\tone, two, three\nCats, dogs, birds\tfour, five, six\n";
    let bare = Call::tsv();
    assert_eq!(bare.units(data), ["Red, green, blue", "one, two, three", "Cats, dogs, birds", "four, five, six"]);
    let out = bare.build(data, &[(0, "Rot, Gruen, Blau")]);
    let want = String::from_utf8_lossy(data).replace("Red, green, blue", "Rot, Gruen, Blau");
    assert_eq!(out, want.as_bytes());
}

#[test]
fn shopify_fixture() {
    let data = std::fs::read(data_dir().join("shopify_translate_adapt.csv")).unwrap();
    let c = Call::csv().langs("en", &["de"], Some("de"));
    let texts = c.units(&data);
    assert_eq!(texts[0], "Classic Cotton Tee");
    assert!(texts.iter().any(|t| t == "classic-cotton-tee"), "{texts:?}");
    let out = c.build(&data, &[(0, "Klassisches Baumwoll-T-Shirt"), (3, "Summer Sale")]);
    let lines: Vec<&[u8]> = out.split(|b| *b == b'\n').collect();
    let orig: Vec<&[u8]> = data.split(|b| *b == b'\n').collect();
    let l1 = lines[1].strip_suffix(b"\r").unwrap_or(lines[1]);
    assert!(l1.ends_with(b"Classic Cotton Tee,Klassisches Baumwoll-T-Shirt"), "{l1:?}");
    assert_eq!(lines[4], orig[4], "a filled translation is kept for an untranslated row");
    assert_eq!(lines[2], orig[2]);
    assert_eq!(lines[5], orig[5]);
}

#[test]
fn hostile_inputs() {
    let c = call(None);
    let mut wide = b"source,".to_vec();
    wide.extend((0..1200).map(|i| format!("c{i}")).collect::<Vec<_>>().join(",").into_bytes());
    wide.extend(b"\nx\n");
    assert!(matches!(c.extract(&wide), Err(Error::Limit(_))), "wide row");
    let mut huge = b"source\n\"".to_vec();
    huge.extend(std::iter::repeat_n(b'a', 200_000));
    huge.extend(b"\"\n");
    assert!(matches!(c.extract(&huge), Err(Error::Limit(_))), "huge field");
    // an unterminated quote parses or is refused, never crashes
    let _ = c.extract(b"source\n\"never closed\nmore\n");
    // the segment cap
    let mut rows = b"source\n".to_vec();
    for i in 0..20 {
        rows.extend(format!("row {i}\n").bytes());
    }
    let o = ExtractOptions { limits: Some(Limits { max_units: 5, ..Limits::default() }), ..Default::default() };
    assert!(matches!(c.extract_with(&rows, &o), Err(Error::Limit(_))), "segment cap");
}

const E2E: &[u8] = b"key,en,de,fr,context\r\ngreet,Good morning,,Bonjour,home screen\r\nbye,See you soon,,,\r\n";

#[test]
fn ingest_export_reingest() {
    let c = Call::csv().langs("en-US", &["de-DE", "fr-FR"], Some("de-DE"));
    let doc = c.extract(E2E).unwrap();
    let texts: Vec<String> = doc.units.iter().map(|u| plain_text(&u.source())).collect();
    assert_eq!(texts, ["Good morning", "See you soon"]);
    assert_eq!(doc.units[0].context.notes.join(" | "), "Key: greet | context: home screen");
    let out = c.build(E2E, &[(0, "Guten Morgen")]);
    let want = String::from_utf8_lossy(E2E).replace("Good morning,,", "Good morning,Guten Morgen,");
    assert_eq!(out, want.as_bytes());
    // fr-FR: nothing translated (the export sends no text for untranslated rows): the file is untouched
    let fr = Call::csv().langs("en-US", &["de-DE", "fr-FR"], Some("fr-FR"));
    assert!(fr.build(E2E, &[]).starts_with(b"key,en,de,fr,context\r\ngreet,Good morning,,Bonjour,"));
    // opening the same file again gives the same plan and the same unit ids
    let again = c.extract(E2E).unwrap();
    assert_eq!(doc.units, again.units);
    assert_eq!(doc.skeleton, again.skeleton);
}

#[test]
fn column_mapping_messages_are_plain() {
    let r = Call::csv().cols("source=9").langs("en", &[], None).extract(b"ID,English,German\n1,Save,\n");
    let Err(Error::Unsupported(m)) = r else { panic!("{r:?}") };
    assert!(m.contains("does not exist"), "{m}");
    let r = Call::csv().cols("source=Nope").extract(b"ID,English,German\n1,Save,\n");
    let Err(Error::Unsupported(m)) = r else { panic!("{r:?}") };
    assert!(m.contains("Nope"), "{m}");
    // the column spec given by name and number reaches the same plan
    let data = b"ID,English,German\n1,Save,\n";
    let a = Call::csv().cols("source=English,target.de=German").langs("en-US", &["de-DE"], Some("de-DE")).extract(data).unwrap();
    let b = Call::csv().cols("source=2,target.de=3").langs("en-US", &["de-DE"], Some("de-DE")).extract(data).unwrap();
    assert_eq!(a.units[0].codes, b.units[0].codes);
}

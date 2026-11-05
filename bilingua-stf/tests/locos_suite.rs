//! Port of LocOS `tests/verify_stf_filter.py`. The suite has 12 cases; 10 are ported here. Not ported: the
//! registration check (LocOS app wiring, msgraph, MIME, docs: the facade contract test covers registration) and the
//! API and CLI round trip (LocOS web surfaces; the same upload, translate and export steps are the round-trip
//! tests of this crate).
mod common;
use bilingua_model::*;
use bilingua_stf::{max_length_for_key, over_limit, records, salesforce_language_code, StfFilter};
use common::*;

const BILINGUAL: &str = "# Use the Bilingual file to review translations and add translations.\n\
# - The TRANSLATED section of the file contains text that has been translated.\n\
# - The UNTRANSLATED section of the file contains text that hasn't been translated.\n\
Language code: de\n\
Type: Bilingual\n\
Translation type: Metadata\n\
\n\
------------------TRANSLATED-------------------\n\
\n\
# KEY\tLABEL\tTRANSLATION\tOUT OF DATE\n\
\n\
# CustomField\n\
CustomField.Account.Region.FieldLabel\tRegion\tRegion\t-\n\
CustomField.Account.Region.HelpText\tPick the sales region.\\nOne per account.\tVerkaufsregion.\t*\n\
\n\
------------------UNTRANSLATED-----------------\n\
\n\
# KEY\tLABEL\n\
\n\
# CustomLabel\n\
CustomLabel.Welcome\tWelcome back, {0}! See {!$User.FirstName}.\n\
PicklistValue.Account.Industry.Agriculture\tAgriculture\n\
CustomLabel.Blank\t\n\
CustomLabel.Dots\t\\n\n";

const SOURCE: &str = "# Use the Source file to translate labels for the first time.\r\n\
Language code: en_US\r\n\
Type: Source\r\n\
Translation type: Metadata\r\n\
\r\n\
# KEY\tLABEL\r\n\
\r\n\
ButtonOrLink.Account.Call\tCall now\r\n\
LayoutSection.Account.Account Layout.Address\tAddress Information\r\n";

const OUTDATED: &str = "Language code: fr\n\
Type: Outdated and untranslated\n\
\n\
------------------OUTDATED AND UNTRANSLATED-----------------\n\
\n\
# KEY\tLABEL\n\
CustomLabel.Save\tSave\n";

fn pt(t: &str) -> String {
    format!("[de] {t}")
}

fn s(b: Vec<u8>) -> String {
    String::from_utf8(b).unwrap()
}

#[test]
fn bilingual_units_and_identity() {
    let data = BILINGUAL.as_bytes();
    assert_eq!(
        units(data),
        vec!["Region", "Pick the sales region.\u{27E6}x1\u{27E7}One per account.", "Welcome back, \u{27E6}x1\u{27E7}! See \u{27E6}x2\u{27E7}.", "Agriculture"]
    );
    let styles: Vec<_> = extract(data).unwrap().units.iter().map(|u| u.context.style.clone().unwrap()).collect();
    assert_eq!(styles, ["stf:translated", "stf:translated", "stf:untranslated", "stf:untranslated"]);
    assert_identity(data);
    assert_eq!(build(data, &[], Some("de")), data, "build_localized to the same language");
    let d = extract(data).unwrap();
    assert_eq!(d.units[0].context.max_len, Some(40));
    assert_eq!(d.units[1].context.max_len, Some(510));
    assert_eq!(d.units[2].context.key.as_deref(), Some("CustomLabel.Welcome"));
}

#[test]
fn bilingual_translated_import_shape() {
    let data = BILINGUAL.as_bytes();
    let all = all_with(data, pt);
    let out = s(build_with(data, &all, Some("de-DE"), &MergeOptions::locos()));
    let lines: Vec<&str> = out.split('\n').collect();
    assert!(lines.contains(&"CustomField.Account.Region.FieldLabel\tRegion\t[de] Region\t-"), "{out}");
    assert!(
        lines.contains(&"CustomField.Account.Region.HelpText\tPick the sales region.\\nOne per account.\t[de] Pick the sales region.\\nOne per account.\t*"),
        "{out}"
    );
    assert!(lines.contains(&"CustomLabel.Welcome\t[de] Welcome back, {0}! See {!$User.FirstName}."), "{out}");
    assert!(lines.contains(&"PicklistValue.Account.Industry.Agriculture\t[de] Agriculture"));
    assert!(lines.contains(&"CustomLabel.Blank\t") && lines.contains(&"CustomLabel.Dots\t\\n"));
    assert!(lines.contains(&"Language code: de_DE"));
    assert!(lines.contains(&"Type: Bilingual") && lines.contains(&"# KEY\tLABEL\tTRANSLATION\tOUT OF DATE"));
    let keys = |rows: Vec<&str>| -> Vec<String> {
        rows.into_iter().filter(|l| l.contains('\t') && !l.starts_with('#')).map(|l| l.split('\t').next().unwrap().to_string()).collect()
    };
    assert_eq!(keys(BILINGUAL.split('\n').collect()), keys(lines));
    assert_eq!(units(out.as_bytes()).len(), 4);
}

#[test]
fn partial_translation_keeps_reviewed() {
    let data = BILINGUAL.as_bytes();
    let out = s(build(data, &[(1, "Verkaufsregion ausw\u{e4}hlen.\u{27E6}x1\u{27E7}Eine pro Konto.")], None));
    assert!(out.contains("FieldLabel\tRegion\tRegion\t-"));
    assert!(out.contains("\tVerkaufsregion ausw\u{e4}hlen.\\nEine pro Konto.\t*\n"), "{out}");
}

#[test]
fn source_and_outdated_files() {
    let data = SOURCE.as_bytes();
    assert_eq!(units(data), ["Call now", "Address Information"]);
    assert_identity(data);
    let out = s(build(data, &[(0, "Jetzt anrufen"), (1, "Adressinformationen")], Some("de")));
    assert!(out.contains("Language code: de\r\n"), "{out}");
    assert!(out.contains("ButtonOrLink.Account.Call\tJetzt anrufen\r\n"));
    assert!(out.contains("LayoutSection.Account.Account Layout.Address\tAdressinformationen\r\n"));
    assert!(out.contains("Type: Source\r\n") && out.matches("\r\n").count() == SOURCE.matches("\r\n").count());
    let od = extract(OUTDATED.as_bytes()).unwrap();
    assert_eq!(od.units[0].context.style.as_deref(), Some("stf:outdated and untranslated"));
    let o = s(build(OUTDATED.as_bytes(), &[(0, "Enregistrer")], Some("fr-FR")));
    assert!(o.contains("CustomLabel.Save\tEnregistrer\n") && o.contains("Language code: fr_FR\n"), "{o}");
}

#[test]
fn escaping_new_text_and_tokens() {
    let data = BILINGUAL.as_bytes();
    let out = s(build(data, &[(3, "Land\twirtschaft\r\nneu"), (2, "\u{27E6}x2\u{27E7} hallo \u{27E6}x1\u{27E7}")], None));
    assert!(!out.contains("\tAgriculture\n"));
    assert!(out.contains("PicklistValue.Account.Industry.Agriculture\tLand\\twirtschaft\\nneu\n"), "{out}");
    assert!(out.contains("CustomLabel.Welcome\t{!$User.FirstName} hallo {0}\n"), "{out}");
    assert_eq!(out.split('\n').count(), BILINGUAL.split('\n').count());
}

#[test]
fn missing_translation_cell_and_limits() {
    let text = "Language code: de\nType: Bilingual\n------------------TRANSLATED-------------------\n\
# KEY\tLABEL\tTRANSLATION\tOUT OF DATE\n\
CustomField.Account.Tier.FieldLabel\tTier\n";
    let long = "Sehr langer Feldname der viel zu lang ist fuer Salesforce";
    let out = s(build(text.as_bytes(), &[(0, long)], None));
    assert!(out.contains(&format!("FieldLabel\tTier\t{long}\n")), "{out}");
    let mut doc = extract(text.as_bytes()).unwrap();
    let id = doc.units[0].segments[0].id.clone();
    doc.set_target(&id, vec![Inline::text(long)]).unwrap();
    let rep = over_limit(&doc);
    assert!(rep.len() == 1 && rep[0].max_length == 40 && rep[0].length == long.chars().count(), "{rep:?}");
    let recs = records(BILINGUAL.as_bytes()).unwrap();
    assert_eq!(recs[0].key, "CustomField.Account.Region.FieldLabel");
    assert_eq!(recs[1].key, "CustomField.Account.Region.HelpText");
    assert!(recs[1].translation.as_deref() == Some("Verkaufsregion.") && recs[1].out_of_date);
    assert!(!recs[0].out_of_date && recs[3].max_length == Some(255));
    assert!(recs[2].translation.is_none() && recs.len() == 6);
    assert_eq!(max_length_for_key("CustomLabel.X"), Some(1000));
}

#[test]
fn language_codes() {
    let cases = [
        ("de", "de"),
        ("de-DE", "de_DE"),
        ("pt-BR", "pt_BR"),
        ("es-419", "es_419"),
        ("zh-Hans", "zh_CN"),
        ("zh-Hant-TW", "zh_TW"),
        ("zh-CN", "zh_CN"),
        ("nl", "nl_NL"),
        ("nl-BE", "nl_BE"),
        ("he", "iw"),
        ("id-ID", "in_ID"),
        ("nb", "no"),
        ("en-GB", "en_GB"),
    ];
    for (bcp, sf) in cases {
        assert_eq!(salesforce_language_code(bcp), sf, "{bcp}");
    }
}

#[test]
fn encodings() {
    let data = BILINGUAL.as_bytes();
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend_from_slice(data);
    assert_eq!(units(&bom), units(data));
    assert_identity(&bom);
    let out = build(&bom, &[(0, "Gebiet")], None);
    assert!(out.starts_with(&[0xEF, 0xBB, 0xBF]) && s(out).contains("Gebiet"));
    // Excel "Unicode Text" (UTF-16 LE with BOM): read, and written as UTF-8 when asked (LocOS behaviour)
    let mut u16: Vec<u8> = vec![0xFF, 0xFE];
    u16.extend(BILINGUAL.encode_utf16().flat_map(|u| u.to_le_bytes()));
    assert_eq!(units(&u16), units(data));
    assert_identity(&u16);
    let o16 = s(build(&u16, &[(0, "Gebiet")], None));
    assert!(o16.starts_with("# Use the Bilingual") && o16.contains("Gebiet"));
    // Windows-1252 file
    let cp: Vec<u8> = b"Language code: fr\n# KEY\tLABEL\nCustomLabel.Cafe\tCaf\xe9 cr\xe8me\n".to_vec();
    assert_eq!(units(&cp), ["Caf\u{e9} cr\u{e8}me"]);
    assert!(s(build(&cp, &[(0, "Caf\u{e9}")], None)).ends_with("CustomLabel.Cafe\tCaf\u{e9}\n"));
    // the file's own encoding is kept by default
    let keep = build_with(&cp, &[(0, "Caf\u{e9}".into())], None, &MergeOptions::default());
    assert!(keep.ends_with(b"CustomLabel.Cafe\tCaf\xe9\n"));
}

#[test]
fn hostile_inputs() {
    // binary refused
    assert!(extract(&b"\x00\x01\x02\x03".repeat(100)).is_err());
    // segment cap
    let rows: String = (0..10).map(|i| format!("CustomLabel.K{i}\tText {i}\n")).collect();
    let l = Limits { max_units: 5, ..Limits::default() };
    let text = format!("Language code: de\n{rows}");
    assert!(matches!(StfFilter.extract_with(text.as_bytes(), &opts(), &l), Err(Error::Limit(_))));
    // line budget (20 x cap) before any per-line work
    let l = Limits { max_units: 5, max_lines: 100, ..Limits::default() };
    assert!(matches!(StfFilter.extract_with("\n".repeat(200).as_bytes(), &opts(), &l), Err(Error::Limit(_))));
    // long single cell
    let l = Limits { max_unit_chars: 50, ..Limits::default() };
    let long = format!("CustomLabel.X\t{}\n", "a".repeat(100));
    assert!(matches!(StfFilter.extract_with(long.as_bytes(), &opts(), &l), Err(Error::Limit(_))));
    // header-looking text after data is not a header; keyless rows and comments never units
    let weird = "Language code: de\n\torphan label\nCustomLabel.A\tAlpha\nLanguage code: xx\n#CustomLabel.B\tCommented\n-----not a marker\n";
    assert_eq!(units(weird.as_bytes()), ["Alpha"]);
    let out = s(build(weird.as_bytes(), &[(0, "A")], Some("ja")));
    assert!(out.starts_with("Language code: ja\n") && out.contains("Language code: xx\n"));
    // a header value with a merge-field-like brace never runs or expands anything
    assert_identity(b"Language code: {!x}\n");
}

#[test]
fn gate_row_and_header_edges() {
    let src = "Language code:\ten_US\nType:\tSource\n# KEY\tLABEL\nCustomLabel.A\tFirst second\u{85}third\u{c}fourth\nCustomLabel.B\tPlain\n";
    assert_eq!(units(src.as_bytes()), ["First second\u{85}third\u{c}fourth", "Plain"]);
    assert_identity(src.as_bytes());
    let out = s(build(src.as_bytes(), &[(0, "Erste zweite"), (1, "Einfach")], Some("de-DE")));
    assert_eq!(out, "Language code:\tde_DE\nType:\tSource\n# KEY\tLABEL\nCustomLabel.A\tErste zweite\nCustomLabel.B\tEinfach\n");
}

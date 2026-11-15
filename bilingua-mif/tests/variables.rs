//! BILI-54: `mif.extract_variables` makes the text of user variable definitions (`VariableDef` in
//! `VariableFormats`) and paragraph number strings (`PgfNumString`) units, as Okapi's okf_mif does. It is off by
//! default (LocOS lists neither), so the default units stay LocOS's.
mod common;
use bilingua_mif::Mif;
use bilingua_model::*;
use common::*;

const DOC: &str = "<MIFFile 2019> # test\n\
<VariableFormats\n \
<VariableFormat\n  <VariableName `Product'>\n  <VariableDef `Localization OS'>\n > # end of VariableFormat\n \
<VariableFormat\n  <VariableName `Page Count'>\n  <VariableDef `<$lastpagenum\\>'>\n > # end of VariableFormat\n \
<VariableFormat\n  <VariableName `Chapter'>\n  <VariableDef `Chapter <$chapnum\\> of the \\Qguide\\q'>\n > # end of VariableFormat\n\
> # end of VariableFormats\n\
<TextFlow\n \
<Para\n  <PgfTag `Heading1'>\n  <PgfNumString `Step 1:\\t'>\n  <ParaLine\n   <String `Getting started'>\n  >\n >\n \
<Para\n  <PgfTag `Numbered'>\n  <PgfNumString `2.'>\n  <ParaLine\n   <String `Second'>\n  >\n >\n\
> # end of TextFlow\n";

fn with_vars() -> ExtractOptions {
    ExtractOptions::default().with_format_option("mif.extract_variables", "true")
}

#[test]
fn off_by_default_the_units_are_the_paragraphs() {
    assert_eq!(texts(DOC.as_bytes()), ["Getting started", "Second"]);
    assert!(Mif.format_option_keys().iter().any(|k| k.key == "mif.extract_variables"));
}

#[test]
fn variable_definitions_and_number_strings_are_units() {
    let doc = Mif.extract(DOC.as_bytes(), &with_vars()).unwrap();
    let got: Vec<String> = doc.units.iter().map(locos_text).collect();
    // `<$lastpagenum>` alone and `2.` have no words: not units
    assert_eq!(got, ["Localization OS", "Chapter \u{27e6}x1\u{27e7} of the `guide'", "Step 1:\t", "Getting started", "Second"]);
    assert_eq!(doc.units[0].context.style.as_deref(), Some("Product"));
    assert_eq!(doc.units[0].context.kind.as_deref(), Some("variable"));
    assert_eq!(doc.units[2].context.kind.as_deref(), Some("paragraph number"));
    assert_eq!(doc.units[2].context.style.as_deref(), Some("Heading1"));
    assert_eq!(Mif.merge(&doc).unwrap(), DOC.as_bytes(), "untouched merge is the original");
}

#[test]
fn translated_variables_and_numbers_are_written_back() {
    let mut doc = Mif.extract(DOC.as_bytes(), &with_vars()).unwrap();
    doc.units[0].segments[0].target = vec![Inline::text("Localisation OS")];
    doc.units[1].segments[0].target = target_from_locos("Kapitel \u{27e6}x1\u{27e7} des `Handbuchs' > alles");
    doc.units[2].segments[0].target = vec![Inline::text("Schritt 1:\t")];
    doc.units[3].segments[0].target = vec![Inline::text("Erste Schritte")];
    let out = String::from_utf8(Mif.merge(&doc).unwrap()).unwrap();
    assert!(out.contains("<VariableDef `Localisation OS'>"), "{out}");
    assert!(out.contains("<VariableDef `Kapitel <$chapnum\\> des \\QHandbuchs\\q \\> alles'>"), "{out}");
    assert!(out.contains("<VariableDef `<$lastpagenum\\>'>"), "{out}");
    assert!(out.contains("<PgfNumString `Schritt 1:\\t'>"), "{out}");
    assert!(out.contains("<PgfNumString `2.'>"), "{out}");
    let back = Mif.extract(out.as_bytes(), &with_vars()).unwrap();
    let got: Vec<String> = back.units.iter().map(locos_text).collect();
    assert_eq!(got, ["Localisation OS", "Kapitel \u{27e6}x1\u{27e7} des `Handbuchs' > alles", "Schritt 1:\t", "Erste Schritte", "Second"]);
}

#[test]
fn a_dropped_building_block_is_kept() {
    let mut doc = Mif.extract(DOC.as_bytes(), &with_vars()).unwrap();
    doc.units[1].segments[0].target = vec![Inline::text("Kapitel ")];
    let out = String::from_utf8(Mif.merge(&doc).unwrap()).unwrap();
    assert!(out.contains("<VariableDef `Kapitel <$chapnum\\>'>"), "{out}");
}

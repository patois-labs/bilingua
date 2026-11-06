//! Column mode and comment units against LocOS: the workbooks and what LocOS reads and writes for them are made by
//! `oracle/locos_xlsx_modes.py` into `tests/data/modes/`.

use super::*;
use bilingua_model::Filter;
use serde_json::Value;

fn dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("data").join("modes")
}

fn data(name: &str) -> Vec<u8> {
    std::fs::read(dir().join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn case(name: &str) -> Value {
    serde_json::from_slice(&data(&format!("{name}.json"))).unwrap()
}

fn text(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}

fn extract_case(name: &str, parse: &Value, locale: Option<&str>, locos_profile: bool) -> Result<Document, Error> {
    let mut o = if locos_profile { profile::locos().extract_options().with_granularity(Granularity::Paragraph) } else { ExtractOptions::new() };
    o = o.with_target_lang(locale.map(str::to_string));
    if let Some(s) = text(parse, "source_locale") {
        o = o.with_source_lang(Some(s));
    }
    if let Some(c) = text(parse, "columns") {
        o = o.with_format_option("xlsx.columns", c.as_str());
    }
    if let Some(l) = parse.get("target_locales").and_then(Value::as_array) {
        let l: Vec<&str> = l.iter().filter_map(Value::as_str).collect();
        o = o.with_format_option("xlsx.target_locales", l.join(",").as_str());
    }
    if parse.get("comments").and_then(Value::as_bool) == Some(true) {
        o = o.with_format_option("xlsx.translate_comments", true);
    }
    Filter::extract(&crate::Xlsx::default(), &data(&format!("{name}.xlsx")), &o)
}

fn p_index(u: &Unit) -> String {
    u.context.extra[profile::P_INDEX].clone()
}

fn style(u: &Unit) -> String {
    u.context.extra.get(profile::STYLE).cloned().unwrap_or_default()
}

fn plain(inl: &[Inline]) -> String {
    inl.iter().map(|i| if let Inline::Text(t) = i { t.as_str() } else { "" }).collect()
}

fn entry(zip: &[u8], name: &str) -> Option<String> {
    entries(zip).into_iter().find(|e| e.0 == name).map(|e| String::from_utf8(e.1).unwrap())
}

fn canon(raw: &[u8]) -> Option<String> {
    let t = decode_part(raw)?;
    Some(dom::parse(&t).ok()?.serialize())
}

fn entries(zip: &[u8]) -> Vec<(String, Vec<u8>)> {
    crate::zipio::read_zip(zip, &bilingua_zip::ZipLimits::default()).unwrap().into_iter().map(|e| (e.name, e.data)).collect()
}

fn same_package(got: &[u8], want: &[u8], what: &str) {
    let (g, w) = (entries(got), entries(want));
    assert_eq!(g.iter().map(|e| &e.0).collect::<Vec<_>>(), w.iter().map(|e| &e.0).collect::<Vec<_>>(), "{what}: members");
    for ((name, a), (_, b)) in g.iter().zip(&w) {
        if a != b {
            let (ca, cb) = (canon(a), canon(b));
            assert!(ca.is_some() && ca == cb, "{what}: {name} differs\n got: {}\nwant: {}", String::from_utf8_lossy(a), String::from_utf8_lossy(b));
        }
    }
}

/// Reads the case as LocOS did and writes every build LocOS wrote.
fn check(name: &str, locos_profile: bool) {
    let info = case(name);
    let parse = &info["parse"];
    let want: Vec<&Value> = info["units"].as_array().unwrap().iter().collect();
    if info["error"].as_bool() == Some(true) {
        assert!(extract_case(name, parse, None, locos_profile).is_err(), "{name}: LocOS refuses it");
        return;
    }
    let doc = extract_case(name, parse, None, locos_profile).unwrap();
    assert_eq!(doc.units.len(), want.len(), "{name}: unit count");
    for (u, w) in doc.units.iter().zip(&want) {
        assert_eq!(plain(&u.segments[0].source), w["text"].as_str().unwrap(), "{name}: text");
        assert_eq!(p_index(u), w["p_index"].to_string(), "{name}: p_index");
        assert_eq!(style(u), w["style"].as_str().unwrap(), "{name}: style");
        assert_eq!(u.context.notes.join(" | "), w["note"].as_str().unwrap(), "{name}: note of {}", w["text"]);
    }
    for b in info["builds"].as_array().unwrap() {
        let map = b["map"].as_object().unwrap();
        if !locos_profile && map.is_empty() {
            continue; // LocOS stamps the language of an untranslated file; plain Bilingua does not
        }
        let locale = text(b, "locale");
        let mut d = extract_case(name, parse, locale.as_deref(), locos_profile).unwrap();
        for (k, v) in map {
            let id = d.units.iter().find(|u| &p_index(u) == k).unwrap_or_else(|| panic!("{name}: no unit {k}")).segments[0].id.clone();
            d.set_target(&id, vec![Inline::text(v.as_str().unwrap())]).unwrap();
        }
        let opts = if locos_profile { profile::locos().merge_options("xlsx") } else { MergeOptions::default() };
        let got = Filter::merge_with(&crate::Xlsx::default(), &d, &opts).unwrap();
        same_package(&got, &data(b["file"].as_str().unwrap()), &format!("{name}/{}", b["name"]));
    }
}

macro_rules! cases {
    ($($fn_name:ident => $case:literal),* $(,)?) => {
        $(
            #[test]
            fn $fn_name() {
                check($case, true);
            }
        )*
    };
}

cases! {
    column_mode_header_gaps_and_filled_targets => "table_basic",
    column_mode_notes_and_a_target_column_past_the_end => "table_notes",
    column_mode_overwrites_formula_targets_and_drops_the_calc_chain => "table_formula",
    column_mode_finds_the_roles_from_the_header => "table_auto",
    column_mode_two_targets => "table_two_targets",
    column_mode_refuses_a_mapping_that_fits_no_sheet => "table_nomatch",
    comment_units_and_their_write_back => "comments_basic",
    comments_not_asked_for_are_not_touched => "comments_off",
}

#[test]
fn column_mode_without_the_profile_writes_the_same_cells() {
    for name in ["table_basic", "table_notes", "table_formula", "table_auto", "table_two_targets", "table_nomatch"] {
        check(name, false);
    }
}

#[test]
fn column_mode_documents_carry_the_original_and_merge_through_the_report_function() {
    let info = case("table_basic");
    let mut d = extract_case("table_basic", &info["parse"], Some("de-DE"), false).unwrap();
    assert!(matches!(d.skeleton, Skeleton::Binary { .. }));
    let id = d.units[0].segments[0].id.clone();
    d.set_target(&id, vec![Inline::text("Hallo")]).unwrap();
    let (out, _) = crate::merge_with_report(&d).unwrap();
    let xml = entry(&out, "xl/worksheets/sheet1.xml").unwrap();
    assert!(xml.contains("Hallo"), "{xml}");
    // nothing translated: the upload comes back byte for byte
    let d = extract_case("table_basic", &info["parse"], Some("de-DE"), false).unwrap();
    assert_eq!(crate::merge_with_report(&d).unwrap().0, data("table_basic.xlsx"));
}

#[test]
fn a_bad_mapping_is_refused_with_its_reason() {
    let bytes = data("table_basic.xlsx");
    let run = |cols: &str| Filter::extract(&crate::Xlsx::default(), &bytes, &ExtractOptions::new().with_format_option("xlsx.columns", cols)).err().map(|e| e.to_string());
    assert!(run("bogus").unwrap().contains("role=column"));
    assert!(run("delimiter=tab,source=1").unwrap().contains("CSV/TSV"));
    assert!(run("source=1,source=1,colour=red").unwrap().contains("Unknown column role"));
    // a blank mapping is still column mode (LocOS tests the string's truth, not its content): header detection finds nothing here
    assert!(run(" ").is_some());
}

#[test]
fn no_mapping_and_no_comments_option_change_nothing() {
    let bytes = data("comments_basic.xlsx");
    let plain_doc = Filter::extract(&crate::Xlsx::default(), &bytes, &profile::locos().extract_options()).unwrap();
    assert_eq!(plain_doc.units.len(), 4, "cells only");
    assert!(plain_doc.units.iter().all(|u| style(u).is_empty()));
    let table = Filter::extract(&crate::Xlsx::default(), &bytes, &profile::locos().extract_options().with_format_option("xlsx.columns", "")).unwrap();
    assert_eq!(table.units.len(), 4, "an empty mapping is no column mode");
}

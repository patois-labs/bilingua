//! The `xaml.view_attributes` format option (LocOS `XAML_VIEW_ATTRIBUTES`).

mod common;

use bilingua_model::*;
use bilingua_xaml::Xaml;
use common::*;

const VIEW: &[u8] = include_bytes!("data/locos_suite_view.xaml");

fn with(v: impl ToString) -> ExtractOptions {
    opts().with_format_option("xaml.view_attributes", v)
}

fn texts(f: &Xaml, o: &ExtractOptions) -> Vec<String> {
    let d = f.extract(VIEW, o).unwrap();
    d.units.iter().map(|u| show(u, &u.source())).collect()
}

#[test]
fn the_option_turns_view_attributes_on_and_off() {
    let on = units_with(&ON, VIEW);
    assert_eq!(on.len(), 6);
    for v in ["true", "1", "yes", "on"] {
        assert_eq!(texts(&OFF, &with(v)), on, "{v}");
    }
    assert_eq!(texts(&OFF, &opts()), ["OK"], "no option: unchanged");
    // an explicit false beats a filter built with view attributes on
    assert_eq!(texts(&ON, &with(false)), ["OK"]);
    assert_eq!(texts(&ON, &opts()), on, "no option keeps the filter's own setting");
}

#[test]
fn the_option_is_declared_and_checked() {
    let keys: Vec<&str> = OFF.format_option_keys().iter().map(|k| k.key).collect();
    assert_eq!(keys, ["xaml.view_attributes"]);
    assert!(matches!(OFF.extract(VIEW, &with("maybe")), Err(Error::FormatOption { .. })));
    let typo = opts().with_format_option("xaml.view_attribute", true);
    assert!(matches!(OFF.extract(VIEW, &typo), Err(Error::FormatOption { .. })));
    let d = OFF.extract(VIEW, &with(true)).unwrap();
    assert!(matches!(OFF.merge_with(&d, &MergeOptions::new().with_format_option("xaml.nope", 1)), Err(Error::FormatOption { .. })));
    // another format's key is not this filter's business
    assert!(OFF.extract(VIEW, &opts().with_format_option("docx.walk", "x")).is_ok());
}

#[test]
fn a_merge_after_the_option_round_trips_the_bytes() {
    let d = OFF.extract(VIEW, &with(true)).unwrap();
    assert_eq!(OFF.merge(&d).unwrap(), VIEW);
}

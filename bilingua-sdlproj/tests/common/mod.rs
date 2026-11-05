#![allow(dead_code)]
//! Helpers shared by the integration tests.

use bilingua_model::*;
use bilingua_sdlproj::Sdlproj;

pub fn opts() -> ExtractOptions {
    ExtractOptions { target_lang: Some("de-DE".into()), granularity: Granularity::Paragraph, ..Default::default() }
}

pub fn extract(data: &[u8]) -> Document {
    Sdlproj.extract(data, &opts()).expect("extract")
}

pub fn fixture(list: &[(&'static str, &'static [u8])], name: &str) -> &'static [u8] {
    list.iter().find(|(n, _)| *n == name).unwrap_or_else(|| panic!("no fixture {name}")).1
}

pub fn err(data: &[u8]) -> String {
    match Sdlproj.extract(data, &opts()) {
        Ok(_) => panic!("expected a refusal"),
        Err(e) => e.to_string(),
    }
}

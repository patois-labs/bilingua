//! Large generated files (over 5 MB), extracted and merged. Run
//! `cargo test --release -p bilingua-xcstrings --test speed -- --nocapture` for the timings. Set `XC_SPEED_OUT` to a
//! folder to also save the files (`big.xcstrings`, `big.xcloc`), so the LocOS filters can be timed on the same bytes.
mod common;
#[path = "xcloc_common/mod.rs"]
mod xcloc_common;
use bilingua_model::*;
use bilingua_xcstrings::{Xcloc, Xcstrings};
use std::time::Instant;

pub fn catalog() -> Vec<u8> {
    let words = ["harbor", "lantern", "bread", "window", "morning", "garden", "ticket", "river", "paper", "market"];
    let mut s = String::with_capacity(8_000_000);
    s.push_str("{\n  \"sourceLanguage\" : \"en\",\n  \"strings\" : {\n");
    let n_entries = 12_000usize;
    for n in 0..n_entries {
        let w = |k: usize| words[(n * 7 + k) % words.len()];
        let entry = if n % 3 == 0 {
            format!(
                "    \"items.{n}\" : {{\n      \"comment\" : \"Count of {} in the list\",\n      \"localizations\" : {{\n        \"en\" : {{\n          \"variations\" : {{\n            \"plural\" : {{\n              \"one\" : {{\n                \"stringUnit\" : {{\n                  \"state\" : \"translated\",\n                  \"value\" : \"%lld {} near the {}\"\n                }}\n              }},\n              \"other\" : {{\n                \"stringUnit\" : {{\n                  \"state\" : \"translated\",\n                  \"value\" : \"%lld {}s near the {} caf\\u00e9\"\n                }}\n              }}\n            }}\n          }}\n        }}\n      }}\n    }}",
                w(1), w(2), w(3), w(2), w(3)
            )
        } else {
            format!(
                "    \"Open the {} {} near the {} at %1$@ and bring %2$@ for \\\"{}\\\" number {n}\" : {{\n      \"comment\" : \"Sentence {n}\",\n      \"localizations\" : {{\n        \"en\" : {{\n          \"stringUnit\" : {{\n            \"state\" : \"translated\",\n            \"value\" : \"Open the {} {} near the {} at %1$@ and bring %2$@ for \\\"{}\\\" number {n} of the guide.\"\n          }}\n        }}\n      }}\n    }}",
                w(1), w(2), w(3), w(4), w(1), w(2), w(3), w(4)
            )
        };
        s.push_str(&entry);
        s.push_str(if n + 1 < n_entries { ",\n" } else { "\n" });
    }
    s.push_str("  },\n  \"version\" : \"1.0\"\n}");
    s.into_bytes()
}

pub fn xliff() -> Vec<u8> {
    let words = ["harbor", "lantern", "bread", "window", "morning", "garden", "ticket", "river", "paper", "market"];
    let mut s = String::with_capacity(6_000_000);
    s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<xliff xmlns=\"urn:oasis:names:tc:xliff:document:1.2\" version=\"1.2\">\n  <file original=\"App/Localizable.xcstrings\" source-language=\"en\" target-language=\"fr\" datatype=\"plaintext\">\n    <body>\n");
    for n in 0..20_000usize {
        let w = |k: usize| words[(n * 7 + k) % words.len()];
        s.push_str(&format!(
            "      <trans-unit id=\"key.{n}\" xml:space=\"preserve\">\n        <source>Open the {} {} near the {} at %1$@ and bring %2$@ for number {n} of the guide.</source>\n        <target state=\"new\">Open the {} {} near the {} at %1$@ and bring %2$@ for number {n} of the guide.</target>\n        <note>Sentence {n}</note>\n      </trans-unit>\n",
            w(1), w(2), w(3), w(1), w(2), w(3)
        ));
    }
    s.push_str("    </body>\n  </file>\n</xliff>\n");
    s.into_bytes()
}

#[test]
fn large_catalog_round_trips_and_is_timed() {
    let bytes = catalog();
    assert!(bytes.len() >= 5_000_000, "only {} bytes", bytes.len());
    if let Ok(dir) = std::env::var("XC_SPEED_OUT") {
        std::fs::write(format!("{dir}/big.xcstrings"), &bytes).unwrap();
    }
    let o = ExtractOptions::new().with_target_lang("ru");
    let t = Instant::now();
    let mut d = Xcstrings.extract(&bytes, &o).unwrap();
    let t_extract = t.elapsed();
    // 4000 plural entries x 2 forms + 8000 sentences
    assert_eq!(d.units.len(), 4_000 * 2 + 8_000);
    let t = Instant::now();
    let same = Xcstrings.merge(&d).unwrap();
    let t_same = t.elapsed();
    assert_eq!(same, bytes);
    for u in d.units.iter_mut() {
        let (text, map) = profile::tokens::to_locos_text(&u.source());
        u.segments[0].target = profile::tokens::from_locos_text(&format!("\u{ab}{text}\u{bb}"), &map);
    }
    let t = Instant::now();
    let out = Xcstrings.merge(&d).unwrap();
    let t_ru = t.elapsed();
    let back = Xcstrings.extract(&out, &ExtractOptions::default()).unwrap();
    assert_eq!(back.units.len(), d.units.len());
    println!("xcstrings speed: {} bytes, {} units; extract {t_extract:?}, untouched merge {t_same:?}, ru merge {t_ru:?}; extract+merge {:?}", bytes.len(), d.units.len(), t_extract + t_ru);
}

#[test]
fn large_bundle_round_trips_and_is_timed() {
    let x = xliff();
    assert!(x.len() >= 5_000_000, "only {} bytes", x.len());
    let bundle = xcloc_common::Bundle { xliffs: vec![("fr.xliff", &x)], ..Default::default() }.build();
    if let Ok(dir) = std::env::var("XC_SPEED_OUT") {
        std::fs::write(format!("{dir}/big.xcloc"), &bundle).unwrap();
    }
    let t = Instant::now();
    let mut d = Xcloc.extract(&bundle, &ExtractOptions::new().with_target_lang("de")).unwrap();
    let t_extract = t.elapsed();
    assert_eq!(d.units.len(), 20_000);
    let t = Instant::now();
    let same = Xcloc.merge(&Xcloc.extract(&bundle, &ExtractOptions::default()).unwrap()).unwrap();
    let t_same = t.elapsed();
    assert_eq!(same, bundle);
    for u in d.units.iter_mut() {
        let (text, map) = profile::tokens::to_locos_text(&u.source());
        u.segments[0].target = profile::tokens::from_locos_text(&format!("\u{ab}{text}\u{bb}"), &map);
    }
    let t = Instant::now();
    let out = Xcloc.merge(&d).unwrap();
    let t_de = t.elapsed();
    assert_ne!(out, bundle);
    println!("xcloc speed: {} bytes xliff, {} units; extract {t_extract:?}, untouched extract+merge {t_same:?}, de merge {t_de:?}", x.len(), d.units.len());
}

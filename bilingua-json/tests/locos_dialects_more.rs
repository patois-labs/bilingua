//! More JSON dialect checks: eleven languages, UTF-16, random input, speed.

use bilingua_json::JsonFilter;
use bilingua_model::profile::{self, tokens};
use bilingua_model::*;

fn locos_opts() -> ExtractOptions {
    profile::locos().extract_options()
}

fn extract(src: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
    let mut d = JsonFilter.extract(src, opts)?;
    profile::annotate(&mut d, Some("locos"));
    Ok(d)
}

fn merge(d: &Document) -> Vec<u8> {
    Filter::merge_with(&JsonFilter, d, &profile::locos().merge_options("json")).unwrap()
}

/// Every unit translated with a prefix, as the adapter would set LocOS text.
fn pseudo(src: &str, lang: &str) -> String {
    let opts = locos_opts().with_target_lang(lang.to_string());
    let mut d = extract(src.as_bytes(), &opts).unwrap();
    let todo: Vec<(String, String, tokens::LocosMap)> = d
        .units
        .iter()
        .flat_map(|u| u.segments.iter())
        .map(|s| {
            let (t, m) = tokens::to_locos_text_for("json", &s.source);
            (s.id.clone(), t, m)
        })
        .collect();
    for (id, t, m) in todo {
        profile::locos_set_target(&mut d, &id, tokens::from_locos_text(&format!("\u{dc} {t}"), &m)).unwrap();
    }
    String::from_utf8(merge(&d)).unwrap()
}

const LANGS: [(&str, &str); 11] = [
    ("en", "Welcome to Larchmoor"),
    ("es", "Bienvenido a Larchmoor"),
    ("fr", "Bienvenue \u{e0} Larchmoor"),
    ("it", "Benvenuti a Larchmoor"),
    ("de", "Willkommen in Larchmoor"),
    ("nl", "Welkom in Larchmoor"),
    ("pt", "Bem-vindo a Larchmoor"),
    ("ja", "\u{30e9}\u{30fc}\u{30c1}\u{30e0}\u{30a2}\u{3078}\u{3088}\u{3046}\u{3053}\u{305d}"),
    ("zh", "\u{6b22}\u{8fce}\u{6765}\u{5230}\u{62c9}\u{5947}\u{83ab}\u{5c14}"),
    ("ko", "\u{b77c}\u{ce58}\u{baa8}\u{c5b4}\u{c5d0} \u{c624}\u{c2e0} \u{ac83}\u{c744}"),
    ("ar", "\u{645}\u{631}\u{62d}\u{628}\u{627} \u{628}\u{643}\u{645} \u{641}\u{64a} \u{644}\u{627}\u{631}\u{62a}\u{634}\u{645}\u{648}\u{631}"),
];

fn file(shape: &str) -> (String, usize) {
    match shape {
        "chrome" => {
            let rows: Vec<String> = LANGS.iter().map(|(k, v)| format!("  \"{k}\": {{\"message\": \"{v} $1\", \"description\": \"d\"}}")).collect();
            (format!("{{\n{}\n}}\n", rows.join(",\n")), 11)
        }
        "icu" => {
            let rows: Vec<String> = LANGS.iter().map(|(k, v)| format!("  \"{k}\": {{\"defaultMessage\": \"{v} {{n, plural, one{{# x}} other{{# y}}}}\"}}")).collect();
            (format!("{{\n{}\n}}\n", rows.join(",\n")), 33)
        }
        _ => {
            let rows: Vec<String> = LANGS.iter().map(|(k, v)| format!("  \"{k}\": \"{v} {{who}}\"")).collect();
            (format!("{{\n  \"@@locale\": \"en\",\n{}\n}}\n", rows.join(",\n")), 11)
        }
    }
}

#[test]
fn eleven_languages_round_trip_in_every_dialect() {
    for shape in ["chrome", "icu", "arb"] {
        let (src, want) = file(shape);
        let d = extract(src.as_bytes(), &locos_opts()).unwrap();
        assert_eq!(d.units.len(), want, "{shape}");
        assert_eq!(merge(&d), src.as_bytes(), "{shape}");
        for lang in ["de", "ar", "ja", "ru"] {
            let out = pseudo(&src, lang);
            serde_json::from_str::<serde_json::Value>(&out).unwrap();
            let back = extract(out.as_bytes(), &locos_opts()).unwrap();
            assert!(back.units.len() >= 11, "{shape} {lang}");
            assert!(out.contains("\u{dc} "));
        }
        // UTF-16 is read and written back
        let mut u16 = vec![0xFF, 0xFE];
        u16.extend(src.encode_utf16().flat_map(u16::to_le_bytes));
        let d = extract(&u16, &locos_opts()).unwrap();
        assert_eq!(d.units.len(), want);
        let out = Filter::merge_with(&JsonFilter, &d, &MergeOptions::new()).unwrap();
        assert_eq!(out, u16);
    }
}

fn quote(s: &str) -> String {
    serde_json::to_string(s).unwrap()
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(400))]
    #[test]
    fn random_bytes_never_panic(b in proptest::collection::vec(proptest::prelude::any::<u8>(), 0..300)) {
        let opts = locos_opts().with_target_lang("ru".to_string());
        if let Ok(d) = extract(&b, &opts) {
            let _ = Filter::merge_with(&JsonFilter, &d, &profile::locos().merge_options("json"));
        }
    }

    #[test]
    fn random_text_in_a_dialect_shape_never_panics(s in "[ -~\u{e9}\u{4e2d}]{0,120}") {
        let src = format!("{{\"@@locale\": \"en\", \"k\": {}}}", quote(&s));
        let opts = locos_opts().with_target_lang("ru".to_string());
        let mut d = extract(src.as_bytes(), &opts).unwrap();
        for u in 0..d.units.len() {
            let id = d.units[u].segments[0].id.clone();
            let _ = d.set_target(&id, vec![Inline::text("T")]);
        }
        let out = merge(&d);
        serde_json::from_slice::<serde_json::Value>(&out).unwrap();
    }
}

/// `cargo test -p bilingua-json --release -- --ignored --nocapture`: 6 MB of ARB-shaped JSON with plurals.
#[test]
#[ignore]
fn speed_six_megabytes() {
    let mut s = String::from("{\n  \"@@locale\": \"en\"");
    let mut i = 0;
    while s.len() < 6_000_000 {
        s.push_str(&format!(",\n  \"k{i}\": \"Message {i} for {{who}} with {{count, plural, one{{# file}} other{{# files}}}} and more words {}\"", "lorem ipsum dolor sit amet ".repeat(24)));
        i += 1;
    }
    s.push_str("\n}\n");
    let opts = locos_opts().with_target_lang("ru".to_string());
    let t = std::time::Instant::now();
    let d = extract(s.as_bytes(), &opts).unwrap();
    let t1 = t.elapsed();
    let t = std::time::Instant::now();
    let out = merge(&d);
    println!("SPEED bytes={} units={} extract={:?} merge_with_ru_stamp={:?} out={}", s.len(), d.units.len(), t1, t.elapsed(), out.len());
}

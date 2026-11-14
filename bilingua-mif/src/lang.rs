//! The target language in the file: every `PgfLanguage` and `FLanguage` keyword.

use crate::scan::{Tree, NONE};

/// Locale to FrameMaker language keyword (the MIF Reference's PgfLanguage list). Region-specific entries are
/// checked before the bare language.
const TABLE: &[(&str, &str)] = &[
    ("en-gb", "UKEnglish"),
    ("en", "USEnglish"),
    ("de-ch", "SwissGerman"),
    ("de-at", "AustriaGerman"),
    ("de", "German"),
    ("fr-ca", "CanadianFrench"),
    ("fr", "French"),
    ("es", "Spanish"),
    ("ca", "Catalan"),
    ("it", "Italian"),
    ("pt-br", "Brazilian"),
    ("pt", "Portuguese"),
    ("da", "Danish"),
    ("nl", "Dutch"),
    ("nb", "Norwegian"),
    ("no", "Norwegian"),
    ("nn", "Nynorsk"),
    ("fi", "Finnish"),
    ("sv", "Swedish"),
    ("ja", "Japanese"),
    ("zh-hant", "TraditionalChinese"),
    ("zh-tw", "TraditionalChinese"),
    ("zh-hk", "TraditionalChinese"),
    ("zh-mo", "TraditionalChinese"),
    ("zh", "SimplifiedChinese"),
    ("ko", "Korean"),
    ("ar", "Arabic"),
    ("he", "Hebrew"),
    ("iw", "Hebrew"),
];

/// The FrameMaker language keyword for a locale; `NoLanguage` when FrameMaker has none (better no spelling or
/// hyphenation language than the source one); `None` for an empty locale.
pub fn fm_language(locale: &str) -> Option<&'static str> {
    if locale.is_empty() {
        return None;
    }
    let lower = locale.replace('_', "-").to_lowercase();
    let parts: Vec<&str> = lower.split('-').collect();
    for n in (1..=parts.len()).rev() {
        let key = parts[..n].join("-");
        if let Some((_, v)) = TABLE.iter().find(|(k, _)| *k == key) {
            return Some(v);
        }
    }
    Some("NoLanguage")
}

/// `text` with every `PgfLanguage` / `FLanguage` word set to `lang`.
pub fn stamp(text: &str, tree: &Tree, lang: &str) -> String {
    let mut spans: Vec<(usize, usize)> = Vec::new();
    let mut stack: Vec<u32> = vec![0];
    while let Some(node) = stack.pop() {
        for c in tree.children(node) {
            let name = tree.name(text, c);
            if (name == "PgfLanguage" || name == "FLanguage") && tree.atom_arg(text, c).is_some() {
                let a = tree.nodes[c as usize].args[0];
                spans.push((a.start, a.end));
            } else if tree.nodes[c as usize].first_child != NONE {
                stack.push(c);
            }
        }
    }
    if spans.is_empty() {
        return text.to_string();
    }
    spans.sort_unstable();
    let mut out = String::with_capacity(text.len() + spans.len() * 8);
    let mut pos = 0;
    for (a, b) in spans {
        out.push_str(&text[pos..a]);
        out.push_str(lang);
        pos = b;
    }
    out.push_str(&text[pos..]);
    out
}

//! BCP 47 tag to the InDesign language a package declares in `designmap.xml` (`<Language Name="$ID/...">`).
//! A name is used only when the package declares it, so a wrong guess stamps nothing rather than a bad reference.

use std::collections::HashSet;

const TABLE: &[(&str, &[&str])] = &[
    ("en-us", &["English: USA"]),
    ("en-gb", &["English: UK"]),
    ("en-ca", &["English: Canadian", "English: USA"]),
    ("en", &["English: USA", "English: UK"]),
    ("de-ch", &["de_CH_2006", "German: Swiss"]),
    ("de-at", &["German: Austrian", "de_DE_2006", "German: Reformed"]),
    ("de", &["de_DE_2006", "German: Reformed", "German: Traditional"]),
    ("fr-ca", &["French: Canadian", "French"]),
    ("fr", &["French"]),
    ("es", &["Spanish: Castilian", "es_ES"]),
    ("it", &["Italian"]),
    ("nl", &["nl_NL_2005", "Dutch"]),
    ("pt-br", &["Portuguese: Brazilian", "Portuguese: Orthographic Agreement"]),
    ("pt", &["Portuguese", "Portuguese: Orthographic Agreement"]),
    ("nb", &["Norwegian: Bokmal"]),
    ("no", &["Norwegian: Bokmal"]),
    ("nn", &["Norwegian: Nynorsk"]),
    ("ar", &["Arabic"]),
    ("bg", &["Bulgarian"]),
    ("ca", &["Catalan"]),
    ("hr", &["Croatian"]),
    ("cs", &["Czech"]),
    ("da", &["Danish"]),
    ("et", &["Estonian"]),
    ("fi", &["Finnish"]),
    ("el", &["Greek"]),
    ("he", &["Hebrew"]),
    ("hu", &["Hungarian"]),
    ("lv", &["Latvian"]),
    ("lt", &["Lithuanian"]),
    ("pl", &["Polish"]),
    ("ro", &["Romanian"]),
    ("ru", &["Russian"]),
    ("sk", &["Slovak"]),
    ("sl", &["Slovenian"]),
    ("sv", &["Swedish"]),
    ("th", &["Thai"]),
    ("tr", &["Turkish"]),
    ("uk", &["Ukrainian"]),
    ("ja", &["Japanese"]),
    ("ko", &["Korean"]),
    ("zh-tw", &["Chinese: Traditional"]),
    ("zh-hk", &["Chinese: Traditional"]),
    ("zh-hant", &["Chinese: Traditional"]),
    ("zh", &["Chinese: Simplified"]),
];

fn names_for(key: &str) -> &'static [&'static str] {
    TABLE.iter().find(|(k, _)| *k == key).map(|(_, v)| *v).unwrap_or(&[])
}

/// The `$ID/...` name to stamp for `tag`, or `None` when the package declares no matching language.
pub fn indesign_language(declared: &HashSet<String>, tag: &str) -> Option<String> {
    let norm = tag.replace('_', "-");
    let parts: Vec<&str> = norm.split('-').collect();
    let lang = parts[0].to_ascii_lowercase();
    let region = parts[1..].iter().find(|p| p.chars().count() == 2 && p.chars().all(char::is_alphabetic)).map(|p| p.to_ascii_uppercase());
    let script = parts[1..].iter().find(|p| p.chars().count() == 4).map(|p| p.to_ascii_lowercase());
    let mut keys = vec![];
    if let Some(s) = &script {
        keys.push(format!("{lang}-{s}"));
    }
    if let Some(r) = &region {
        keys.push(format!("{lang}-{}", r.to_ascii_lowercase()));
    }
    keys.push(lang.clone());
    let mut names: Vec<String> = keys.iter().flat_map(|k| names_for(k).iter().map(|s| s.to_string())).collect();
    if let Some(r) = &region {
        names.push(format!("{lang}_{r}"));
    }
    let pfx = format!("$ID/{lang}_");
    let mut extra: Vec<String> = declared.iter().filter(|n| n.starts_with(&pfx)).map(|n| n[4..].to_string()).collect();
    extra.sort();
    names.extend(extra);
    names.into_iter().map(|n| format!("$ID/{n}")).find(|n| declared.contains(n))
}

const RTL_SCRIPTS: &[&str] = &["arab", "hebr", "syrc", "thaa", "nkoo", "adlm", "rohg", "samr", "mand", "mend", "yezi"];
const RTL_LANGS: &[&str] = &[
    "ar", "arc", "azb", "ckb", "dv", "fa", "glk", "he", "iw", "ji", "ks", "lrc", "mzn", "nqo", "pnb", "prs", "ps", "sd", "syr", "ug", "ur", "yi",
];

/// Right to left, as Localization OS decides: a script subtag decides (`ku-Arab` yes, `sd-Deva` no); without one
/// the base language does.
pub fn is_rtl(tag: &str) -> bool {
    let norm = tag.replace('_', "-").to_ascii_lowercase();
    let mut parts = norm.split('-');
    let lang = parts.next().unwrap_or("");
    for sub in parts {
        if sub.len() == 4 && sub.chars().all(|c| c.is_ascii_alphabetic()) {
            return RTL_SCRIPTS.contains(&sub);
        }
    }
    RTL_LANGS.contains(&lang)
}

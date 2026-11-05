//! Target-language helpers: BCP 47 canonical tag, text direction, and the InDesign language name for a tag
//! (LocOS `html_lang`, `text_direction`, `_indesign_language`).

/// The canonical BCP 47 tag for a stored locale code, or None when it is not usable.
pub fn html_lang(code: &str) -> Option<String> {
    let t = code.trim().replace('_', "-");
    if t.is_empty() || t.len() > 64 {
        return None;
    }
    let parts: Vec<&str> = t.split('-').collect();
    let first = parts[0];
    if !(2..=8).contains(&first.len()) || !first.bytes().all(|c| c.is_ascii_alphabetic()) {
        return None;
    }
    for sub in &parts[1..] {
        if sub.is_empty() || sub.len() > 8 || !sub.bytes().all(|c| c.is_ascii_alphanumeric()) {
            return None;
        }
    }
    let mut out = vec![first.to_ascii_lowercase()];
    let mut extension = false;
    for sub in &parts[1..] {
        let alpha = sub.bytes().all(|c| c.is_ascii_alphabetic());
        if sub.len() == 1 {
            extension = true;
            out.push(sub.to_ascii_lowercase());
        } else if extension {
            out.push(sub.to_ascii_lowercase());
        } else if sub.len() == 4 && alpha {
            let l = sub.to_ascii_lowercase();
            out.push(format!("{}{}", l[..1].to_ascii_uppercase(), &l[1..]));
        } else if (sub.len() == 2 && alpha) || (sub.len() == 3 && sub.bytes().all(|c| c.is_ascii_digit())) {
            out.push(sub.to_ascii_uppercase());
        } else {
            out.push(sub.to_ascii_lowercase());
        }
    }
    Some(out.join("-"))
}

const RTL_SCRIPTS: [&str; 11] = ["arab", "hebr", "syrc", "thaa", "nkoo", "adlm", "rohg", "samr", "mand", "mend", "yezi"];
const RTL_LANGS: [&str; 22] = [
    "ar", "arc", "azb", "ckb", "dv", "fa", "glk", "he", "iw", "ji", "ks", "lrc", "mzn", "nqo", "pnb", "prs", "ps", "sd", "syr", "ug", "ur", "yi",
];

/// True when the (canonical) tag is written right to left: a script subtag decides, else the base language.
pub fn is_rtl(tag: &str) -> bool {
    let parts: Vec<&str> = tag.split('-').collect();
    for sub in &parts[1..] {
        if sub.len() == 1 {
            break;
        }
        if sub.len() == 4 && sub.bytes().all(|c| c.is_ascii_alphabetic()) {
            return RTL_SCRIPTS.contains(&sub.to_ascii_lowercase().as_str());
        }
    }
    RTL_LANGS.contains(&parts[0])
}

const ID_LANG: &[(&str, &[&str])] = &[
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

/// The `$ID/...` language name to stamp for a BCP 47 tag, or None when the file declares no matching language.
pub fn indesign_language(declared: &std::collections::BTreeSet<String>, tag: &str) -> Option<String> {
    let parts: Vec<String> = tag.replace('_', "-").split('-').map(str::to_string).collect();
    let lang = parts[0].to_lowercase();
    let region = parts[1..].iter().find(|p| p.len() == 2 && p.bytes().all(|c| c.is_ascii_alphabetic())).map(|p| p.to_uppercase()).unwrap_or_default();
    let script = parts[1..].iter().find(|p| p.len() == 4).map(|p| p.to_lowercase()).unwrap_or_default();
    let mut keys = Vec::new();
    if !script.is_empty() {
        keys.push(format!("{lang}-{script}"));
    }
    if !region.is_empty() {
        keys.push(format!("{lang}-{}", region.to_lowercase()));
    }
    keys.push(lang.clone());
    let mut names: Vec<String> = Vec::new();
    for k in &keys {
        if let Some((_, v)) = ID_LANG.iter().find(|(key, _)| key == k) {
            names.extend(v.iter().map(|s| s.to_string()));
        }
    }
    if !region.is_empty() {
        names.push(format!("{lang}_{region}"));
    }
    let prefix = format!("$ID/{lang}_");
    // BTreeSet iterates sorted, like Python's sorted()
    names.extend(declared.iter().filter(|n| n.starts_with(&prefix)).map(|n| n[4..].to_string()));
    names.into_iter().map(|n| format!("$ID/{n}")).find(|n| declared.contains(n))
}

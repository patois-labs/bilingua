//! Locale spellings: BCP 47 to the POSIX form `.desktop` keys use, and the English language name a Haiku catalog
//! header carries.

use crate::langnames::{ALIASES, NAMES};
use crate::text::py_strip;

fn alpha(s: &str) -> bool {
    !s.is_empty() && s.chars().all(char::is_alphabetic)
}

fn digits(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_digit())
}

fn title(s: &str) -> String {
    let mut it = s.chars();
    match it.next() {
        Some(f) => f.to_uppercase().chain(it.flat_map(char::to_lowercase)).collect(),
        None => String::new(),
    }
}

/// A BCP 47 tag as the POSIX locale a Desktop Entry key uses: `fr-FR` is `fr_FR`, `sr-Latn-RS` is `sr_RS@latin`,
/// `zh-Hans` is `zh_CN`. Empty for an empty tag.
pub fn posix_locale(locale: &str) -> String {
    let parts: Vec<&str> = py_strip(locale).split(['-', '_']).filter(|p| !p.is_empty()).collect();
    let Some(first) = parts.first() else { return String::new() };
    let lang = first.to_lowercase();
    let (mut script, mut region, mut modifier) = (String::new(), String::new(), String::new());
    for p in &parts[1..] {
        let n = p.chars().count();
        if n == 4 && alpha(p) && script.is_empty() {
            script = title(p);
        } else if (n == 2 && alpha(p)) || (n == 3 && digits(p)) {
            if region.is_empty() {
                region = p.to_uppercase();
            }
        } else if let Some(m) = p.strip_prefix('@') {
            modifier = m.to_string();
        } else if n >= 5 && modifier.is_empty() {
            modifier = p.to_lowercase();
        }
    }
    if lang == "zh" && !script.is_empty() && region.is_empty() {
        region = match script.as_str() {
            "Hans" => "CN",
            "Hant" => "TW",
            _ => "",
        }
        .to_string();
        script.clear();
    }
    if lang == "zh" {
        script.clear();
    }
    if !script.is_empty() && modifier.is_empty() {
        modifier = match script.as_str() {
            "Latn" => "latin",
            "Cyrl" => "cyrillic",
            _ => "",
        }
        .to_string();
    }
    let mut out = lang;
    if !region.is_empty() {
        out.push('_');
        out.push_str(&region);
    }
    if !modifier.is_empty() {
        out.push('@');
        out.push_str(&modifier);
    }
    out
}

struct Parts {
    lang: String,
    script: Option<String>,
    territory: Option<String>,
    variant: Option<String>,
}

/// Babel's `parse_locale` (separator `_`); the `@modifier` and `.charset` parts are ignored, as they are in the
/// display name.
fn babel_parts(code: &str) -> Option<Parts> {
    let ident = code.split('@').next().unwrap_or("");
    let ident = ident.split('.').next().unwrap_or("");
    let mut parts: Vec<&str> = ident.split('_').collect();
    let lang = parts.remove(0).to_lowercase();
    if !alpha(&lang) {
        return None;
    }
    let mut script = None;
    if parts.first().is_some_and(|p| p.chars().count() == 4 && alpha(p)) {
        script = Some(title(parts.remove(0)));
    }
    let mut territory = None;
    if let Some(p) = parts.first().copied() {
        let n = p.chars().count();
        if n == 2 && alpha(p) || n == 3 && digits(p) {
            territory = Some(p.to_uppercase());
            parts.remove(0);
        }
    }
    let mut variant = None;
    if let Some(p) = parts.first().copied() {
        let n = p.chars().count();
        let c0 = p.chars().next().unwrap_or(' ');
        if (n == 4 && c0.is_ascii_digit()) || (n >= 5 && c0.is_alphabetic()) {
            variant = parts.pop().map(str::to_uppercase);
        }
    }
    if !parts.is_empty() {
        return None;
    }
    Some(Parts { lang, script, territory, variant })
}

fn join(p: &Parts, script: bool, variant: bool) -> String {
    let mut k = p.lang.clone();
    for part in [script.then_some(&p.script).and_then(Option::as_ref), p.territory.as_ref(), variant.then_some(&p.variant).and_then(Option::as_ref)].into_iter().flatten() {
        k.push('_');
        k.push_str(part);
    }
    k
}

fn name_of(key: &str) -> Option<&'static str> {
    let canon = match ALIASES.binary_search_by(|(k, _)| (*k).cmp(key)) {
        Ok(i) => ALIASES[i].1,
        Err(_) => key,
    };
    NAMES.binary_search_by(|(k, _)| (*k).cmp(canon)).ok().map(|i| NAMES[i].1)
}

/// What Babel's `Locale.parse` does when the exact locale has no data: drop the variant, then the script. A
/// territory it does not know is a refusal, never a fallback to the bare language.
fn lookup(p: &Parts) -> Option<&'static str> {
    let mut tries = vec![join(p, true, true), join(p, true, false), join(p, false, false)];
    if p.territory.is_none() {
        tries.push(p.lang.clone());
    }
    tries.iter().find_map(|k| name_of(k))
}

/// The header language of a Haiku catalog for a BCP 47 target: the lower-case English name (`de` is `german`,
/// `pt-BR` is `portuguese (brazil)`). A code Localization OS's locale library does not know is used as it is.
pub fn language_name(locale: &str) -> String {
    let code = py_strip(locale);
    if code.is_empty() {
        return String::new();
    }
    let name = babel_parts(&code.replace('-', "_")).and_then(|p| lookup(&p));
    let out = name.map_or_else(|| code.to_lowercase(), str::to_string);
    out.replace(['\t', '\r', '\n'], " ")
}

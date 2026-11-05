//! Which strings of a property list are translatable text (Localization OS `apple_filter` policy).

use crate::tree::*;
use bilingua_model::Error;
use crate::xml::is_space;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Info,
    Settings,
    Generic,
}

/// One string value, in document order.
#[derive(Debug, Clone)]
pub(crate) struct Leaf {
    pub node: usize,
    /// Dictionary keys and array positions above the value, joined with `/`.
    pub path: String,
    /// The nearest dictionary key above the value ("" at the top).
    pub near: String,
    /// Some key above the value ends with `UsageDescriptionDictionary`.
    pub usage_dict: bool,
}

const INFO_KEYS: &[&str] = &[
    "CFBundleDisplayName",
    "CFBundleName",
    "CFBundleSpokenName",
    "CFBundleGetInfoString",
    "NSHumanReadableCopyright",
    "CFBundleTypeName",
    "UTTypeDescription",
    "INAlternativeAppName",
    "NSMenuItem",
    "default",
];
const SETTINGS_KEYS: &[&str] = &["Title", "Titles", "ShortTitles", "FooterText"];
const STRINGSDICT_SKIP: &[&str] = &["NSStringFormatSpecTypeKey", "NSStringFormatValueTypeKey"];

pub(crate) fn mode(tree: &Tree) -> Mode {
    let root = &tree.nodes[tree.root];
    if root.kind == Kind::Dict {
        let has = |k: &str| root.kids.iter().any(|kid| kid.key.as_deref() == Some(k));
        if has("CFBundleIdentifier") || root.kids.iter().any(|kid| kid.key.as_deref().is_some_and(|k| k.starts_with("CFBundle"))) {
            return Mode::Info;
        }
        if has("PreferenceSpecifiers") {
            return Mode::Settings;
        }
    }
    Mode::Generic
}

/// Every string value in document order.
pub(crate) fn walk(tree: &Tree, max_units: usize) -> Result<Vec<Leaf>, Error> {
    struct W<'t> {
        tree: &'t Tree,
        out: Vec<Leaf>,
        /// Bytes of path text still allowed to be built (deep nesting under long keys would copy them for ever).
        budget: usize,
        max_units: usize,
    }
    impl W<'_> {
        fn spend(&mut self, n: usize) -> Result<(), Error> {
            self.budget = self.budget.checked_sub(n).ok_or_else(|| crate::xml::too_many(self.max_units))?;
            Ok(())
        }
        fn rec(&mut self, i: usize, path: &str, depth: usize, near: &str, usage: bool) -> Result<(), Error> {
            let tree = self.tree;
            let n = &tree.nodes[i];
            match n.kind {
                Kind::Str => {
                    self.spend(path.len() + near.len())?;
                    self.out.push(Leaf { node: i, path: path.to_string(), near: near.to_string(), usage_dict: usage });
                }
                Kind::Dict => {
                    for kid in &n.kids {
                        let k = kid.key.as_deref().unwrap_or("");
                        let p = if depth == 0 { k.to_string() } else { format!("{path}/{k}") };
                        self.spend(p.len())?;
                        self.rec(kid.node, &p, depth + 1, k, usage || k.ends_with("UsageDescriptionDictionary"))?;
                    }
                }
                Kind::Array => {
                    for (pos, kid) in n.kids.iter().enumerate() {
                        let p = if depth == 0 { pos.to_string() } else { format!("{path}/{pos}") };
                        self.spend(p.len())?;
                        self.rec(kid.node, &p, depth + 1, near, usage)?;
                    }
                }
                Kind::Other => {}
            }
            Ok(())
        }
    }
    let mut w = W { tree, out: Vec::new(), budget: 64 << 20, max_units };
    w.rec(tree.root, "", 0, "", false)?;
    Ok(w.out)
}

/// Whether the value of `leaf` is a unit. `stringsdict` files take every string except the type tags; the others
/// by mode.
pub(crate) fn wanted(stringsdict: bool, mode: Mode, leaf: &Leaf, value: &str) -> bool {
    if !crate::ph::is_translatable(value) {
        return false;
    }
    let near = leaf.near.as_str();
    if stringsdict {
        return !STRINGSDICT_SKIP.contains(&near);
    }
    match mode {
        Mode::Info => INFO_KEYS.contains(&near) || near.ends_with("UsageDescription") || leaf.usage_dict,
        Mode::Settings => SETTINGS_KEYS.contains(&near),
        Mode::Generic => !not_prose(value.trim_matches(is_space)),
    }
}

fn word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Python `\d`: decimal digits of any script.
fn is_digit(c: char) -> bool {
    if c.is_ascii_digit() {
        return true;
    }
    if c.is_ascii() {
        return false;
    }
    const ZEROS: &[u32] = &[
        0x660, 0x6F0, 0x7C0, 0x966, 0x9E6, 0xA66, 0xAE6, 0xB66, 0xBE6, 0xC66, 0xCE6, 0xD66, 0xDE6, 0xE50, 0xED0, 0xF20, 0x1040, 0x1090, 0x17E0, 0x1810, 0x1946,
        0x19D0, 0x1A80, 0x1A90, 0x1B50, 0x1BB0, 0x1C40, 0x1C50, 0xA620, 0xA8D0, 0xA900, 0xA9D0, 0xA9F0, 0xAA50, 0xABF0, 0xFF10, 0x104A0, 0x10D30, 0x11066,
        0x110F0, 0x11136, 0x111D0, 0x112F0, 0x11450, 0x114D0, 0x11650, 0x116C0, 0x11730, 0x118E0, 0x11950, 0x11C50, 0x11D50, 0x11DA0, 0x16A60, 0x16AC0,
        0x16B50, 0x1D7CE, 0x1D7D8, 0x1D7E2, 0x1D7EC, 0x1D7F6, 0x1E140, 0x1E2F0, 0x1E950, 0x1FBF0,
    ];
    let v = c as u32;
    ZEROS.iter().any(|z| (*z..*z + 10).contains(&v))
}

/// The `_NOT_PROSE` pattern: numbers and dates, build settings, reverse-DNS ids and file names, URLs, snake_case,
/// camelCase, CamelCase identifiers and colours. `s` is already trimmed.
pub(crate) fn not_prose(s: &str) -> bool {
    // numbers, versions, dates
    if !s.is_empty() && s.chars().all(|c| is_digit(c) || is_space(c) || matches!(c, '.' | ',' | ':' | '/' | '+' | '-')) {
        return true;
    }
    // $(NAME)rest and ${NAME}rest
    for (open, close) in [("$(", ')'), ("${", '}')] {
        if let Some(body) = s.strip_prefix(open) {
            if let Some(p) = body.find(close) {
                if !body[p + 1..].chars().any(is_space) {
                    return true;
                }
            }
        }
    }
    let first = s.chars().next();
    // reverse-DNS ids, file names
    if first.is_some_and(|c| c.is_ascii_alphabetic()) && s.contains('.') {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() >= 2 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| word(c) || c == '-')) {
            return true;
        }
    }
    // URLs
    if first.is_some_and(|c| c.is_ascii_alphabetic()) {
        let scheme = s.bytes().take_while(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'.' | b'-')).count();
        if let Some(rest) = s[scheme..].strip_prefix("://") {
            if !rest.chars().any(is_space) {
                return true;
            }
        }
    }
    let all_word = !s.is_empty() && s.chars().all(word);
    // snake_case
    if all_word && first.is_some_and(|c| c.is_ascii_alphabetic() || c == '_') {
        let cs: Vec<char> = s.chars().collect();
        if (1..cs.len().saturating_sub(1)).any(|i| cs[i] == '_') {
            return true;
        }
    }
    // camelCase
    if all_word {
        let lower = s.bytes().take_while(u8::is_ascii_lowercase).count();
        if lower >= 1 && s.as_bytes().get(lower).is_some_and(u8::is_ascii_uppercase) {
            return true;
        }
    }
    // UIInterfaceOrientationPortrait
    if s.bytes().all(|c| c.is_ascii_alphanumeric()) && s.as_bytes().first().is_some_and(u8::is_ascii_uppercase) {
        let b = s.as_bytes();
        let mut chunks = Vec::new();
        let mut start = 0;
        for (i, c) in b.iter().enumerate().skip(1) {
            if c.is_ascii_uppercase() {
                chunks.push(i - start);
                start = i;
            }
        }
        chunks.push(b.len() - start);
        // the first chunk needs a lowercase letter or digit after its capital, and two more chunks follow
        if chunks.len() >= 3 && chunks[0] >= 2 {
            return true;
        }
    }
    // colours
    if let Some(h) = s.strip_prefix('#') {
        if (3..=8).contains(&h.len()) && h.bytes().all(|c| c.is_ascii_hexdigit()) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_prose_cases() {
        for s in [
            "2.0.1", "12:30", "$(PRODUCT_NAME)", "${TARGET}-x", "com.example.thing", "https://example.com/help", "home_screen", "isEnabled",
            "#FFAA00", "iOS", "AbcDefGhi", "Info.plist", "a.b-c", "ftp://x",
        ] {
            assert!(not_prose(s), "{s}");
        }
        for s in ["Welcome back", "Save", "Delete note", "Hello.", "UIInterfaceOrientationPortrait", "UIKit", "Home Screen", "Tom & Jerry", "a_", "_a", "$(X) y", "v1.2 beta", "Ok"] {
            assert!(!not_prose(s), "{s}");
        }
    }
}

//! CLDR cardinal plural categories per language (the data Localization OS reads through babel), for the Android
//! `<plurals>` fill on a target-language merge. The table is the shared one in `bilingua_model::cldr`.

use bilingua_model::cldr;

/// The plural categories a language uses, always with `other`, in CLDR order. The base language decides (and the
/// old codes `in`, `iw`, `ji`, `mo`, `tl` count as the language they stand for); an unknown language gets `one`
/// and `other`, as LocOS's `plural_categories` falls back.
pub(crate) fn categories(locale: &str) -> Vec<&'static str> {
    let norm = locale.trim().replace('-', "_");
    let base = norm.split('_').next().unwrap_or("").to_ascii_lowercase();
    match cldr::cardinal_bits_aliased(&base) {
        Some(bits) => cldr::names(bits),
        None => vec!["one", "other"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_languages_match_cldr() {
        assert_eq!(categories("pl"), ["one", "few", "many", "other"]);
        assert_eq!(categories("ja"), ["other"]);
        assert_eq!(categories("ar-EG"), ["zero", "one", "two", "few", "many", "other"]);
        assert_eq!(categories("es-ES"), ["one", "many", "other"]);
        assert_eq!(categories("xx"), ["one", "other"]);
    }
}

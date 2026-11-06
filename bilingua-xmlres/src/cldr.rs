//! CLDR cardinal plural categories per language (the data Localization OS reads through babel), for the Android
//! `<plurals>` fill on a target-language merge. The table is `bilingua-plist`'s (a shared one is on the model list).

use crate::cldr_table::TABLE;

const ORDER: [&str; 6] = ["zero", "one", "two", "few", "many", "other"];

/// The plural categories a language uses, always with `other`, in CLDR order. The base language decides; an
/// unknown language gets `one` and `other`, as LocOS's `plural_categories` falls back.
pub(crate) fn categories(locale: &str) -> Vec<&'static str> {
    let norm = locale.trim().replace('-', "_");
    let base = norm.split('_').next().unwrap_or("").to_ascii_lowercase();
    match TABLE.binary_search_by(|(code, _)| (*code).cmp(base.as_str())) {
        Ok(i) => {
            let bits = TABLE[i].1 | 32;
            ORDER.iter().enumerate().filter(|(n, _)| bits & (1 << n) != 0).map(|(_, c)| *c).collect()
        }
        Err(_) => vec!["one", "other"],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_languages_match_cldr() {
        assert!(TABLE.windows(2).all(|w| w[0].0 < w[1].0));
        assert_eq!(categories("pl"), ["one", "few", "many", "other"]);
        assert_eq!(categories("ja"), ["other"]);
        assert_eq!(categories("ar-EG"), ["zero", "one", "two", "few", "many", "other"]);
        assert_eq!(categories("es-ES"), ["one", "many", "other"]);
        assert_eq!(categories("xx"), ["one", "other"]);
    }
}

//! CLDR cardinal plural categories per language (the data Localization OS reads through babel), used to fill the
//! forms a `.stringsdict` lacks when it is merged for a target language.

use crate::cldr_table::TABLE;

pub(crate) const ORDER: [&str; 6] = ["zero", "one", "two", "few", "many", "other"];

/// The plural categories a language uses, always with `other`, in CLDR order. The full tag is tried first, then
/// its base language; `None` when the language is unknown (the forms then stay as they are).
pub(crate) fn categories(locale: &str) -> Option<Vec<&'static str>> {
    let norm = locale.trim().replace('-', "_");
    let base = norm.split('_').next().unwrap_or("").to_ascii_lowercase();
    let i = TABLE.binary_search_by(|(code, _)| (*code).cmp(base.as_str())).ok()?;
    let bits = TABLE[i].1;
    Some(ORDER.iter().enumerate().filter(|(n, _)| bits & (1 << n) != 0).map(|(_, c)| *c).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_sorted_and_known_languages_match_cldr() {
        assert!(TABLE.windows(2).all(|w| w[0].0 < w[1].0));
        assert_eq!(categories("ru").unwrap(), ["one", "few", "many", "other"]);
        assert_eq!(categories("ja").unwrap(), ["other"]);
        assert_eq!(categories("ar-EG").unwrap(), ["zero", "one", "two", "few", "many", "other"]);
        assert_eq!(categories("pl").unwrap(), ["one", "few", "many", "other"]);
        assert_eq!(categories("en_US").unwrap(), ["one", "other"]);
        assert_eq!(categories("zh-Hant").unwrap(), ["other"]);
        assert_eq!(categories("ko").unwrap(), ["other"]);
        assert_eq!(categories("es").unwrap(), ["one", "many", "other"]);
        // old codes babel still resolves
        assert_eq!(categories("iw").unwrap(), ["one", "two", "other"]);
        assert_eq!(categories("tl-PH").unwrap(), ["one", "other"]);
        assert!(categories("xx").is_none() && categories("").is_none());
    }
}

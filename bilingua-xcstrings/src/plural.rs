//! CLDR plural categories per language (generated from the CLDR data the LocOS filter reads through babel).

pub use bilingua_model::cldr::ORDER;

/// The CLDR plural categories a language uses, `other` always included, in CLDR order. The language is the
/// part of the tag before the first `-` or `_`. `None` when the language is not known (callers then leave the
/// forms as they are).
pub fn categories(locale: &str) -> Option<Vec<&'static str>> {
    bilingua_model::cldr::cardinal(locale)
}

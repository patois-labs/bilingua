//! Bilingua filters for Xcode localization: String Catalogs (`.xcstrings`) and localization bundles (`.xcloc`).
//!
//! **`Xcstrings`** (Localization OS `apple_filter.py`, xcstrings section). A catalog is JSON with a
//! `sourceLanguage` and a `strings` object. Units are the source-language text of each key (or the key itself
//! when the catalog has no source localization), including plural, device and substitution variations;
//! entries with `shouldTranslate: false` or `extractionState: stale` and values with no letter or digit outside
//! their placeholders are skipped (or come as locked units with `include_locked`). Apple format specifiers
//! (`%@`, `%1$@`, `%lld`, `%.2f`, `%#@files@`, `%arg`) are inline codes: a numbered one is a free code (LocOS
//! `p` token), the others keep their order (`x`). A catalog's `comment` is the unit's note.
//!
//! Merge splices into the original text, so everything outside the translated text stays byte for byte (key
//! order, Xcode's ` : ` spacing, `\/`, BOM, line endings). With no `target_lang` (or the source language) the
//! source text is rewritten in place. With a target language the translation is written as that language's
//! localization next to the others: plural variations use the target's CLDR categories (a category the source
//! lacks is filled from the source `other` form), `argNum` and `formatSpecifier` of substitutions are kept,
//! state is `translated`, a unit without a translation keeps the target's existing text, and a new language is
//! inserted in sorted position when the catalog's languages are sorted.
//!
//! **`Xcloc`** (Localization OS `xcloc_filter.py`). A bundle arrives as a zip under the `.xcloc` extension. Every
//! `Localized Contents/*.xliff` goes through `bilingua-xliff`; unit ids are prefixed `x1/`, `x2/`, ... by file.
//! Merge rewrites only the XLIFF files that received a translation (a `state="new"` target that was written
//! becomes `translated`) and, with a `target_lang` and at least one translation, the XLIFF `target-language`
//! and `targetLocale` in `contents.json`. Every other member keeps its bytes, name, order, time stamp and compression; with nothing
//! to change the original archive comes back byte for byte.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod json;
mod ph;
mod plural;
mod xcloc;
mod xcstrings;

pub use xcloc::Xcloc;
pub use xcstrings::{records, Record, Xcstrings};

/// The CLDR plural categories of a language, in CLDR order (`None` when unknown).
pub fn plural_categories(locale: &str) -> Option<Vec<&'static str>> {
    plural::categories(locale)
}

/// Both filters.
pub fn filters() -> Vec<Box<dyn bilingua_model::Filter>> {
    vec![Box::new(Xcstrings), Box::new(Xcloc)]
}

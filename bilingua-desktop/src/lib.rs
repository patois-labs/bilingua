//! Bilingua filters for freedesktop `.desktop` launcher entries and Haiku `.catkeys` catalogs.
//!
//! * [`DesktopFilter`] (`desktop`): the unlocalized `Name`, `GenericName`, `Comment` and `Keywords` keys of the
//!   `[Desktop Entry]` group and every `[Desktop Action ...]` group; a `Keywords` list gives one unit per item.
//!   On a merge with a target language, `Key[ll_CC]=translation` lines are added after the source key (or replace
//!   the file's own line for that locale); without one the values are spliced in place.
//! * [`CatkeysFilter`] (`catkeys`): the source column of a Haiku plain-text catalog is the unit; the translation
//!   goes into the fourth column. The merge with a target language sets the header language to Haiku's English
//!   language name and keeps the header fingerprint (it depends on the source strings only, as in Localization OS).
//!
//! Both read UTF-8 only (a BOM is kept) and refuse anything else, as Localization OS does. Neither makes inline
//! tags: placeholders such as `%s` stay plain text.

/// Skeleton schema number: bump only when this crate's skeleton layout changes.
pub const SKELETON_SCHEMA: u32 = 1;

mod catkeys;
mod desktop;
mod langnames;
mod locale;
mod text;

pub use catkeys::{catkeys_fingerprint, escape as catkeys_escape, records as catkeys_records, unescape as catkeys_unescape, CatkeysFilter};
pub use desktop::{records as desktop_records, DesktopFilter};
pub use locale::{language_name, posix_locale};

/// One entry as a key-based project sees it (translatable or not for catkeys; one per unit for `.desktop`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Record {
    /// `.desktop`: `<group>/<Key>` (`Keywords` items: `<group>/Keywords[<n>]`). `.catkeys`: the source string.
    pub key: String,
    /// `.desktop`: the unescaped value. `.catkeys`: the current translation, unescaped.
    pub value: String,
    pub comment: String,
    /// `.desktop`: the group. `.catkeys`: the Haiku context.
    pub context: String,
    /// Index of the entry's unit, `None` when the entry has none (catkeys entries without a letter).
    pub unit_index: Option<usize>,
    /// The same key (catkeys: source, context and comment) appeared earlier.
    pub duplicate: bool,
}

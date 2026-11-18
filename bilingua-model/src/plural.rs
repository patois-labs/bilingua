//! Plural groups (unit model step 6, BILI-119a): which forms of one message belong together. A unit that is one
//! form of a plural, selectordinal or select message carries a [`PluralSlot`] ([`crate::Unit::plural`]); every
//! form of one message shares its `group`. The target categories are not stored: [`crate::Document::plural_categories`]
//! reads them from the target language.
//!
//! The slot is for consumers (editors, QA). No writer reads it: they keep reading `extra["plural"]`,
//! `extra["icu"]` and their skeletons. Under the LocOS profile no slot is filled.

use serde::{Deserialize, Serialize};

/// The kind of a plural group. Serialises lowercase; a kind this release does not know reads as `other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum PluralKind {
    /// Cardinal plural (`plural`, gettext `msgstr[N]`, xcstrings `variations.plural`).
    Cardinal,
    /// Ordinal plural (ICU `selectordinal`).
    Ordinal,
    /// ICU `select` (a key such as `female`, not a number).
    Select,
    /// A kind written by a newer release.
    #[serde(other)]
    Other,
}

/// One form of a plural group. Build it with [`PluralSlot::new`] and the `with_*` methods. Optional fields are
/// left out of the JSON when unset, `added` when false.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PluralSlot {
    /// Shared by every form of one message (one plural or select argument), unique in the document.
    pub group: Box<str>,
    /// Cardinal, ordinal or select.
    pub kind: PluralKind,
    /// The CLDR category (`one`, `few`), an exact value (`=0`) or a select key (`female`). gettext has none: its
    /// order is not CLDR order (see `index`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<Box<str>>,
    /// The gettext `msgstr[N]` index.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<u8>,
    /// The ICU argument name (`count`), when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arg: Option<Box<str>>,
    /// The enclosing group, for a plural inside a select (or any nesting).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<Box<str>>,
    /// A target category the source does not have, added at extract so it can be translated (and a unit inside
    /// such a form). Untranslated, it writes what the source `other` form gives.
    #[serde(default, skip_serializing_if = "is_false")]
    pub added: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

impl PluralSlot {
    /// A form of `group` of this kind, with no category, index, argument or parent.
    pub fn new(group: impl Into<Box<str>>, kind: PluralKind) -> PluralSlot {
        PluralSlot { group: group.into(), kind, category: None, index: None, arg: None, parent: None, added: false }
    }
    /// Sets the category.
    pub fn with_category(mut self, category: impl Into<Box<str>>) -> PluralSlot {
        self.category = Some(category.into());
        self
    }
    /// Sets the gettext index.
    pub fn with_index(mut self, index: u8) -> PluralSlot {
        self.index = Some(index);
        self
    }
    /// Sets the argument name.
    pub fn with_arg(mut self, arg: impl Into<Box<str>>) -> PluralSlot {
        self.arg = Some(arg.into());
        self
    }
    /// Sets the enclosing group.
    pub fn with_parent(mut self, parent: impl Into<Box<str>>) -> PluralSlot {
        self.parent = Some(parent.into());
        self
    }
    /// Marks the form as added for the target language (or not).
    pub fn with_added(mut self, added: bool) -> PluralSlot {
        self.added = added;
        self
    }
    /// The group.
    pub fn group(&self) -> &str {
        &self.group
    }
    /// The kind.
    pub fn kind(&self) -> PluralKind {
        self.kind
    }
    /// The category.
    pub fn category(&self) -> Option<&str> {
        self.category.as_deref()
    }
    /// The gettext index.
    pub fn index(&self) -> Option<u8> {
        self.index
    }
    /// The argument name.
    pub fn arg(&self) -> Option<&str> {
        self.arg.as_deref()
    }
    /// The enclosing group.
    pub fn parent(&self) -> Option<&str> {
        self.parent.as_deref()
    }
    /// True for a form the target needs and the source does not have.
    pub fn is_added(&self) -> bool {
        self.added
    }
}

/// The categories of `kind` for a target language tag, in CLDR order with `other`; `None` for a select, an
/// unknown kind, no language or one the table does not know. Babel's aliases (`deu`, `iw`, `und`) resolve.
pub(crate) fn categories(target: Option<&str>, kind: PluralKind) -> Option<Vec<&'static str>> {
    let ordinal = match kind {
        PluralKind::Cardinal => false,
        PluralKind::Ordinal => true,
        _ => return None,
    };
    let lang = crate::cldr::language(target?);
    crate::cldr::bits_babel(&lang, ordinal).map(crate::cldr::names)
}

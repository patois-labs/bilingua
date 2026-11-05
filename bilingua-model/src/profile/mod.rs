//! Profiles: named sets of choices one consumer of Bilingua needs, kept out of the general API. There is one,
//! [`locos`]: what the Localization OS adapter needs to give LocOS the files and ids it had before Bilingua.
//!
//! A caller switches a profile on with `ExtractOptions::with_profile("locos")` (and `MergeOptions` from
//! [`Profile::merge_options`]); the `bilingua` facade refuses a name [`find`] does not know. Format crates may switch
//! behaviour on [`ExtractOptions::is_profile`] / [`MergeOptions::is_profile`] (Track D adds the per-format LocOS
//! behaviours that way); [`annotate`] adds what the profile promises to every unit after extraction.
//!
//! [`tokens`] converts inline codes to and from the text tokens Localization OS uses, one token per tag.

use crate::{Document, Error, ExtractOptions, Inline, MergeOptions, OutputEncoding};

pub mod tokens;

/// The Localization OS profile's name.
pub const LOCOS: &str = "locos";
/// Its version: bumped when what the profile produces changes, so the adapter can tell stored documents apart.
pub const LOCOS_VERSION: u32 = 1;
/// `UnitContext::extra` key with the unit's LocOS paragraph index (`p_index`), set under the LocOS profile by the
/// formats that have one. Left unset where Bilingua cannot give LocOS's number yet (the adapter then numbers by
/// position).
pub const P_INDEX: &str = "locos.p_index";
/// `UnitContext::extra` key with the unit's LocOS style (`a11y:*`, `meta:*`, ...), set under the LocOS profile by
/// the formats that have one; unset otherwise.
pub const STYLE: &str = "locos.style";

/// A named profile and its version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Profile {
    /// Its name (`"locos"`).
    pub name: &'static str,
    /// Its version.
    pub version: u32,
}

/// The Localization OS profile.
pub fn locos() -> Profile {
    Profile { name: LOCOS, version: LOCOS_VERSION }
}

/// Every profile Bilingua knows.
pub fn profiles() -> Vec<Profile> {
    vec![locos()]
}

/// The profile with this name.
pub fn find(name: &str) -> Option<Profile> {
    profiles().into_iter().find(|p| p.name == name)
}

/// Refuses a profile name Bilingua does not know (`Error::UnknownProfile`); `None` is fine.
pub fn check(name: Option<&str>) -> Result<(), Error> {
    match name {
        Some(n) if find(n).is_none() => Err(Error::UnknownProfile(n.to_string())),
        _ => Ok(()),
    }
}

/// Formats whose files Localization OS writes back in their own encoding (UTF-16 included) rather than as UTF-8.
const LOCOS_KEEPS_ENCODING: &[&str] = &["ini", "strings", "dxf"];

impl Profile {
    /// Extraction options with this profile switched on.
    pub fn extract_options(&self) -> ExtractOptions {
        ExtractOptions::new().with_profile(self.name)
    }

    /// The merge options this profile uses for a document of `format` (a filter id). Localization OS: UTF-8 out,
    /// except for the formats it writes in their own encoding (ini, strings and dxf: `Keep`), and the target language
    /// stamped and plurals filled even when nothing is translated (lead decision 11-04).
    pub fn merge_options(&self, format: &str) -> MergeOptions {
        let enc = if LOCOS_KEEPS_ENCODING.contains(&format) { OutputEncoding::Keep } else { OutputEncoding::Utf8 };
        MergeOptions::new().with_output_encoding(enc).with_stamp_target_always(true).with_profile(self.name)
    }
}

/// What the profile adds to an extracted document: under the LocOS profile, `context.extra[P_INDEX]` on every unit
/// whose format knows LocOS's paragraph index (odf, Qt ts and wxl from their `p_index`, icml from its paragraph
/// index, idml from its `p<n>` unit ids). Nothing for other profiles or formats. The `bilingua` facade calls this.
pub fn annotate(doc: &mut Document, profile: Option<&str>) {
    if profile != Some(LOCOS) {
        return;
    }
    let format = doc.format.clone();
    for u in &mut doc.units {
        let p = match format.as_str() {
            "icml" => u.hidden("p").map(str::to_string),
            "idml" => u.id.strip_prefix('p').filter(|n| n.parse::<usize>().is_ok()).map(str::to_string),
            // the formats that record LocOS's own index (odf, Qt ts, wxl)
            _ => u.context.extra.get("p_index").cloned(),
        };
        if let Some(p) = p {
            u.context.extra.insert(P_INDEX.to_string(), p);
        }
    }
}

/// Sets a target the way Localization OS means it: an empty LocOS target is a translation to nothing (written as
/// an empty value, [`Document::set_target_empty`]), anything else is [`Document::set_target`].
pub fn locos_set_target(doc: &mut Document, seg_id: &str, target: Vec<Inline>) -> Result<(), Error> {
    if target.iter().all(|i| matches!(i, Inline::Text(t) if t.is_empty())) {
        doc.set_target_empty(seg_id)
    } else {
        doc.set_target(seg_id, target)
    }
}

//! Profiles: named sets of choices one consumer of Bilingua needs, kept out of the general API. There is one,
//! [`locos`]: what the Localization OS adapter needs to give LocOS the files and ids it had before Bilingua.
//!
//! A caller switches a profile on with `ExtractOptions::with_profile("locos")` (and `MergeOptions` from
//! [`Profile::merge_options`]); the `bilingua` facade refuses a name [`find`] does not know. Format crates may switch
//! behaviour on [`ExtractOptions::is_profile`] / [`MergeOptions::is_profile`] (Track D adds the per-format LocOS
//! behaviours that way); [`annotate`] adds what the profile promises to every unit after extraction.
//!
//! [`tokens`] converts inline codes to and from the text tokens Localization OS uses, one token per tag.

use crate::{CodeData, Document, Error, ExtractOptions, Inline, MergeOptions, OutputEncoding, TagKind, Unit};

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

/// Formats whose files Localization OS writes back in their own encoding (UTF-16 included) rather than as UTF-8:
/// ini, strings and dxf; plain text (LocOS `encode_like`: its BOM, UTF-16, or Windows-1252 when the text fits);
/// properties (an ISO-8859-1 file stays ISO-8859-1, `\uXXXX` for the rest). The LocOS PO and subtitle writers keep
/// the file's encoding themselves.
const LOCOS_KEEPS_ENCODING: &[&str] = &["ini", "strings", "dxf", "txt", "properties"];

impl Profile {
    /// Extraction options with this profile switched on.
    pub fn extract_options(&self) -> ExtractOptions {
        ExtractOptions::new().with_profile(self.name)
    }

    /// The merge options this profile uses for a document of `format` (a filter id). Localization OS: UTF-8 out,
    /// except for the formats it writes in their own encoding (ini, strings, dxf, txt and properties: `Keep`), and the target language
    /// stamped and plurals filled even when nothing is translated (lead decision 11-04).
    pub fn merge_options(&self, format: &str) -> MergeOptions {
        let enc = if LOCOS_KEEPS_ENCODING.contains(&format) { OutputEncoding::Keep } else { OutputEncoding::Utf8 };
        MergeOptions::new().with_output_encoding(enc).with_stamp_target_always(true).with_profile(self.name)
    }
}

/// What the profile adds to an extracted document: under the LocOS profile, `context.extra[P_INDEX]` on every unit
/// whose format knows LocOS's paragraph index (odf, Qt ts and wxl from their `p_index`, icml from its paragraph
/// index, idml from its `p<n>` unit ids); the inline codes LocOS keeps as unit text turned back into that text (key
/// and value formats, JSON, PO, ARB placeholders, FTL placeables); `context.style` cleared where LocOS has none and
/// `locos.style` set where it has one (dtd). Nothing for other profiles. The `bilingua` facade calls this.
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
        // Track D, X3: the inline codes LocOS keeps as plain text in the unit become that text again
        if let Some(pick) = locos_text_codes(&format) {
            // FTL keeps the codes: its writer finds the placeables in the translation again, as LocOS's does
            flatten_codes(u, pick, format != "ftl");
        }
        // LocOS's style for the unit: the adapter takes `locos.style`, else `context.style`
        if LOCOS_NO_STYLE.contains(&format.as_str()) {
            u.context.style = None;
        }
        if format == "dtd" {
            if let Some(k) = u.context.key.clone() {
                u.context.extra.insert(STYLE.to_string(), format!("dtd:{k}"));
            }
        }
    }
}

/// Formats whose LocOS units carry no style (`""`) where Bilingua sets `context.style`.
const LOCOS_NO_STYLE: &[&str] = &["po", "wxl", "xaml", "resx", "strings", "properties", "json", "json5", "resjson", "arb"];

/// For the formats whose LocOS filter keeps (some) inline codes as plain unit text, which codes: every code in the
/// key/value formats (placeholders such as `%@`, `{0}`, `[ProductName]`, and JSON's HTML), only `placeholder` codes
/// in ARB (LocOS keeps its ICU plural and select arguments as tokens) and FTL (its placeables).
fn locos_text_codes(format: &str) -> Option<fn(&CodeData) -> bool> {
    match format {
        "po" | "strings" | "properties" | "dtd" | "wxl" | "xaml" | "resx" | "json" | "json5" | "resjson" => Some(|_| true),
        "arb" | "ftl" => Some(|c| c.label.as_deref() == Some("placeholder")),
        _ => None,
    }
}

/// Replaces the tags `pick` selects in every segment's source by the markup they stand for (the code's own
/// open, close or standalone text, else the tag's display), joining the text runs, and drops their codes unless
/// `drop` is false.
fn flatten_codes(u: &mut Unit, pick: fn(&CodeData) -> bool, drop: bool) {
    let mut gone = Vec::new();
    for s in &mut u.segments {
        let mut out: Vec<Inline> = Vec::with_capacity(s.source.len());
        for i in std::mem::take(&mut s.source) {
            let raw = match &i {
                Inline::Tag { id, kind, display, .. } => match u.codes.get(id) {
                    Some(c) if pick(c) => {
                        gone.push(id.clone());
                        let m = match kind {
                            TagKind::Open => c.open.as_deref(),
                            TagKind::Close => c.close.as_deref(),
                            TagKind::Standalone => c.standalone.as_deref(),
                        };
                        Some(m.unwrap_or(display).to_string())
                    }
                    _ => None,
                },
                Inline::Text(_) => None,
            };
            let i = raw.map(Inline::Text).unwrap_or(i);
            match (out.last_mut(), &i) {
                (Some(Inline::Text(a)), Inline::Text(b)) => a.push_str(b),
                _ => out.push(i),
            }
        }
        s.source = out;
    }
    if drop {
        for id in gone {
            u.codes.remove(&id);
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

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
use serde::Serialize;

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
/// `UnitContext::extra` key, LocOS profile: how many images LocOS's accessibility scan anchors at this unit
/// (LocOS `skeleton["a11y_images"]["images"]` is the sum over the units). Unset: none.
pub const A11Y_IMAGES: &str = "locos.a11y_images";
/// `UnitContext::extra` key, LocOS profile: the images anchored at this unit whose alt text is empty, one `where`
/// (the image's file name) per line, in file order (LocOS `missing_alt` entries of kind `empty`). Unset: none.
pub const A11Y_EMPTY_ALT: &str = "locos.a11y_empty_alt";

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

/// The oldest version of the LocOS profile this release can still produce on request. A release that bumps
/// [`LOCOS_VERSION`] keeps the old behaviour reachable (`resolve("locos@1")`), the way LocOS keeps its own walk and
/// reading stamps, so a document stored under the old version still rebuilds; it raises this only when an old version
/// cannot be replayed any more, and the adapter then asks for a re-import.
pub const LOCOS_OLDEST_VERSION: u32 = 1;

/// Every profile Bilingua knows.
pub fn profiles() -> Vec<Profile> {
    vec![locos()]
}

/// The profile with this name.
pub fn find(name: &str) -> Option<Profile> {
    profiles().into_iter().find(|p| p.name == name)
}

/// The profile a caller asks for, at a version: `spec` is a name (`"locos"`) or a name and a version
/// (`"locos@1"`); `version`, when given, must agree with a version in `spec`. No version means the current one. An
/// unknown name is `Error::UnknownProfile`; a version this release cannot produce (newer than the current one, or
/// older than the oldest it replays) is `Error::ParserVersion`, so the caller can ask for a re-import.
pub fn resolve(spec: &str, version: Option<u32>) -> Result<Profile, Error> {
    let (name, in_spec) = match spec.split_once('@') {
        Some((n, v)) => {
            let v: u32 = v.parse().map_err(|_| Error::UnknownProfile(spec.to_string()))?;
            (n, Some(v))
        }
        None => (spec, None),
    };
    let current = find(name).ok_or_else(|| Error::UnknownProfile(spec.to_string()))?;
    let want = match (in_spec, version) {
        (Some(a), Some(b)) if a != b => return Err(Error::UnknownProfile(format!("{spec} (asked for version {b})"))),
        (a, b) => a.or(b).unwrap_or(current.version),
    };
    if !current.reads(want) {
        return Err(Error::ParserVersion {
            found: format!("profile {name} version {want}"),
            expected: format!("profile {name} versions {} to {}", current.oldest(), current.version),
        });
    }
    Ok(Profile { name: current.name, version: want })
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
    /// The oldest version of this profile the release can still produce.
    pub fn oldest(&self) -> u32 {
        match self.name {
            LOCOS => LOCOS_OLDEST_VERSION,
            _ => self.version,
        }
    }

    /// True when this release can produce version `version` of the profile (the current one or an older one it
    /// still replays).
    pub fn reads(&self, version: u32) -> bool {
        let current = find(self.name).map_or(self.version, |p| p.version);
        (self.oldest()..=current).contains(&version)
    }

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

/// The language tag Localization OS declares for a target locale (its `languages_catalog.html_lang`): underscores
/// become hyphens and the case is canonical (`es_ES` gives `es-ES`, `zh_hant_tw` gives `zh-Hant-TW`); `None` for
/// a blank or malformed code (spaces, quotes, an empty subtag, more than 64 characters).
pub fn lang_tag(code: &str) -> Option<String> {
    let tag = code.trim().replace('_', "-");
    if tag.is_empty() || tag.len() > 64 {
        return None;
    }
    let mut parts = tag.split('-');
    let first = parts.next()?;
    if !(2..=8).contains(&first.len()) || !first.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let mut out = vec![first.to_ascii_lowercase()];
    let mut ext = false;
    for sub in parts {
        if sub.is_empty() || sub.len() > 8 || !sub.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return None;
        }
        let alpha = sub.bytes().all(|b| b.is_ascii_alphabetic());
        if sub.len() == 1 {
            ext = true;
            out.push(sub.to_ascii_lowercase());
        } else if ext {
            out.push(sub.to_ascii_lowercase());
        } else if sub.len() == 4 && alpha {
            let mut s = sub.to_ascii_lowercase();
            s[..1].make_ascii_uppercase();
            out.push(s);
        } else if (sub.len() == 2 && alpha) || (sub.len() == 3 && sub.bytes().all(|b| b.is_ascii_digit())) {
            out.push(sub.to_ascii_uppercase());
        } else {
            out.push(sub.to_ascii_lowercase());
        }
    }
    Some(out.join("-"))
}

/// True when Localization OS writes `code` right to left (its `text_direction`): a script subtag decides
/// (`ku-Arab` yes, `ug-Latn` no), else the base language (`ar`, `he`, `fa-IR`, `ur`). False for a code
/// [`lang_tag`] refuses.
pub fn is_rtl(code: &str) -> bool {
    const SCRIPTS: [&str; 11] = ["arab", "hebr", "syrc", "thaa", "nkoo", "adlm", "rohg", "samr", "mand", "mend", "yezi"];
    const LANGS: [&str; 22] =
        ["ar", "arc", "azb", "ckb", "dv", "fa", "glk", "he", "iw", "ji", "ks", "lrc", "mzn", "nqo", "pnb", "prs", "ps", "sd", "syr", "ug", "ur", "yi"];
    let Some(tag) = lang_tag(code) else { return false };
    let parts: Vec<&str> = tag.split('-').collect();
    for sub in &parts[1..] {
        if sub.len() == 1 {
            break;
        }
        if sub.len() == 4 && sub.bytes().all(|b| b.is_ascii_alphabetic()) {
            return SCRIPTS.contains(&sub.to_ascii_lowercase().as_str());
        }
    }
    LANGS.contains(&parts[0])
}

/// One translatable segment as the Localization OS adapter numbers it ([`locos_units`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct LocosUnit {
    /// LocOS's paragraph index (`TextUnit.p_index`).
    pub p_index: i64,
    /// The Bilingua segment id the index stands for (what [`locos_set_target`] takes).
    pub segment_id: String,
    /// The segment's source as LocOS text, tokens included ([`tokens::to_locos_text_for`]).
    pub text: String,
    /// The side map of the tokens in `text`, for [`tokens::from_locos_text`].
    pub map: tokens::LocosMap,
    /// LocOS's style for the unit: `locos.style`, else `context.style`, else empty.
    pub style: String,
}

/// The translatable segments of a document extracted under the LocOS profile at paragraph granularity, numbered the
/// way Localization OS numbers its units: `p_index` is `context.extra[P_INDEX]` when every segment's unit has one
/// and they are all distinct, else the position among the translatable segments; the text is the segment's source
/// with its codes as LocOS tokens, numbered by the format's LocOS rule.
pub fn locos_units(doc: &Document) -> Vec<LocosUnit> {
    let segs: Vec<(&Unit, &crate::Segment)> = doc.units.iter().filter(|u| u.translate).flat_map(|u| u.segments.iter().map(move |s| (u, s))).collect();
    let metas: Vec<Option<i64>> = segs.iter().map(|(u, _)| u.context.extra.get(P_INDEX).and_then(|p| p.parse().ok())).collect();
    let distinct = metas.iter().flatten().collect::<std::collections::BTreeSet<_>>().len() == metas.len();
    let use_meta = !segs.is_empty() && metas.iter().all(Option::is_some) && distinct;
    segs.iter()
        .enumerate()
        .map(|(i, (u, s))| {
            let (text, map) = tokens::to_locos_text_for(&doc.format, &s.source);
            let style = u.context.extra.get(STYLE).filter(|s| !s.is_empty()).cloned().or_else(|| u.context.style.clone()).unwrap_or_default();
            let p_index = match (use_meta, metas[i]) {
                (true, Some(p)) => p,
                _ => i as i64,
            };
            LocosUnit { p_index, segment_id: s.id.clone(), text, map, style }
        })
        .collect()
}

/// Sets a target the way Localization OS means it: an empty LocOS target is a translation to nothing (written as
/// an empty value, [`Document::set_target_empty`]); one of white space only is written as that white space (LocOS
/// writes it, and the adapter's JSON document carries it, where [`Document::set_target`] would read it as no
/// translation); anything else is [`Document::set_target`], except that a translation may repeat a standalone
/// placeholder tag (`#` in ICU, Chrome `$x$`, go `{{..}}`: LocOS accepts it, where `set_target` refuses it as
/// "tag appears more often"); paired tags keep the exactly-once rule and a tag the source lacks is still refused.
pub fn locos_set_target(doc: &mut Document, seg_id: &str, target: Vec<Inline>) -> Result<(), Error> {
    if target.iter().all(|i| matches!(i, Inline::Text(t) if t.is_empty())) {
        return doc.set_target_empty(seg_id);
    }
    let blank = target.iter().all(|i| matches!(i, Inline::Text(t) if t.trim().is_empty()));
    if !blank {
        return doc.set_target_repeating(seg_id, target);
    }
    // validated as any target (unknown segment, locked unit), then kept as it is
    doc.set_target_repeating(seg_id, target.clone())?;
    let (ui, si) = doc.locate(seg_id).ok_or_else(|| Error::UnknownSegment(seg_id.to_string()))?;
    let seg = &mut doc.units[ui].segments[si];
    let text: String = target.iter().map(|i| if let Inline::Text(t) = i { t.as_str() } else { "" }).collect();
    seg.target = vec![Inline::Text(text)];
    seg.empty_target = false;
    seg.state = crate::SegState::Draft;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lang_tags_like_locos() {
        for (code, want) in [("es_ES", Some("es-ES")), ("EN-us", Some("en-US")), ("zh_hant_tw", Some("zh-Hant-TW")), ("es-419", Some("es-419")), ("x", None), ("", None), ("de DE", None), ("de--DE", None), ("\"><b>", None)] {
            assert_eq!(lang_tag(code).as_deref(), want, "{code}");
        }
        assert!(is_rtl("ar-SA") && is_rtl("ku-Arab") && is_rtl("he") && !is_rtl("ug-Latn") && !is_rtl("de") && !is_rtl("<x>"));
    }

    #[test]
    fn resolve_names_and_versions() {
        assert_eq!(resolve("locos", None).unwrap(), locos());
        assert_eq!(resolve("locos@1", None).unwrap(), locos());
        assert_eq!(resolve("locos", Some(LOCOS_VERSION)).unwrap(), locos());
        assert_eq!(resolve("locos@1", Some(1)).unwrap().version, 1);
        assert!(matches!(resolve("locos@1", Some(2)), Err(Error::UnknownProfile(_))));
        assert!(matches!(resolve("locos@x", None), Err(Error::UnknownProfile(_))));
        assert!(matches!(resolve("nope", None), Err(Error::UnknownProfile(_))));
        assert!(matches!(resolve("locos", Some(LOCOS_VERSION + 1)), Err(Error::ParserVersion { .. })));
        assert!(matches!(resolve("locos@0", None), Err(Error::ParserVersion { .. })));
        assert!(locos().reads(1) && !locos().reads(0) && !locos().reads(LOCOS_VERSION + 1));
    }
}

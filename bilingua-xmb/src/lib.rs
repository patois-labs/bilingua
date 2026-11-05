//! Bilingua XMB / XTB filter: Google and Angular message bundles. See the README.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod lang;
mod msgs;
mod render;
mod scan;
mod textenc;

use bilingua_model::*;
use msgs::Kind;

pub use bilingua_model::Limits;

// Hidden codes (ids start with `~`, never a placeholder id).
pub(crate) const RAW: &str = "~raw"; // the message content as written
pub(crate) const LEAD: &str = "~lead"; // white space before the text
pub(crate) const TRAIL: &str = "~trail"; // white space after the text

/// The XMB filter (Angular `ng extract-i18n --format=xmb`, Google message bundles). An XMB merge with no
/// translation gives the original bytes back; with translations it gives the XTB Angular reads (what Localization
/// OS exports). It also reads XTB files, like Localization OS.
#[derive(Debug, Clone, Copy, Default)]
pub struct Xmb;

/// The XTB filter (translation bundles). Same engine as [`Xmb`], with its own id and extension.
#[derive(Debug, Clone, Copy, Default)]
pub struct Xtb;

fn head_text(bytes: &[u8]) -> String {
    let head = &bytes[..bytes.len().min(8192)];
    if head.starts_with(&[0xFF, 0xFE]) {
        return encoding_rs::UTF_16LE.decode_without_bom_handling(&head[2..]).0.into_owned();
    }
    if head.starts_with(&[0xFE, 0xFF]) {
        return encoding_rs::UTF_16BE.decode_without_bom_handling(&head[2..]).0.into_owned();
    }
    String::from_utf8_lossy(head.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(head)).into_owned()
}

/// True when the first start tag in the head is `<name`. The DOCTYPE of a real XMB names `messagebundle` but
/// never opens it as a tag.
fn root_is(bytes: &[u8], name: &str) -> bool {
    let h = head_text(bytes);
    let pat = format!("<{name}");
    h.trim_start().starts_with('<')
        && h.match_indices(&pat).any(|(i, _)| h[i + pat.len()..].starts_with(|c: char| c.is_whitespace() || c == '>' || c == '/'))
}

macro_rules! bundle_filter {
    ($ty:ident, $id:literal, $root:literal) => {
        impl $ty {
            /// Like [`Filter::extract`] with explicit limits.
            pub fn extract_with(&self, bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<Document, Error> {
                msgs::extract(bytes, opts, limits)
            }
        }

        impl Filter for $ty {
            fn id(&self) -> &'static str {
                $id
            }
            fn extensions(&self) -> &'static [&'static str] {
                &[$id]
            }
            /// Tight: the first start tag in the file is the bundle's root element.
            fn sniff(&self, bytes: &[u8]) -> bool {
                root_is(bytes, $root)
            }
            fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
                self.extract_with(bytes, opts, &opts.limits())
            }
            fn merge(&self, doc: &Document) -> Result<Vec<u8>, Error> {
                merge(doc, false)
            }
        }
    };
}

bundle_filter!(Xmb, "xmb", "messagebundle");
bundle_filter!(Xtb, "xtb", "translationbundle");

impl Xmb {
    /// The XTB for a document, whether or not anything is translated (untranslated messages keep their source
    /// text, as Angular falls back). `lang` comes from the document's target language. For an XTB document this is
    /// the same as `merge`, with the language written even when nothing changed.
    pub fn to_xtb(&self, doc: &Document) -> Result<Vec<u8>, Error> {
        merge(doc, true)
    }
}

impl Xtb {
    /// Like `merge`, and always writes the document's target language into `lang`, even when no translation changed.
    pub fn merge_localized(&self, doc: &Document) -> Result<Vec<u8>, Error> {
        merge(doc, true)
    }
}

/// Some unit has a target of any kind.
fn touched(doc: &Document) -> bool {
    doc.units.iter().any(|u| u.translate && u.segments.iter().any(|s| !s.target.is_empty()))
}

/// Some unit's text really differs from its source.
fn changed_text(doc: &Document) -> bool {
    doc.units.iter().any(|u| u.translate && u.segments.iter().any(|s| !s.target.is_empty()) && u.output() != u.source())
}

fn code(u: &Unit, key: &str) -> String {
    u.codes.get(key).and_then(|c| c.standalone.clone()).unwrap_or_default()
}

fn merge(doc: &Document, force: bool) -> Result<Vec<u8>, Error> {
    bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
    let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
        return Err(Error::Write("xmb needs a text skeleton".into()));
    };
    let kind = match doc.format.as_str() {
        "xmb" => Kind::Xmb,
        "xtb" => Kind::Xtb,
        other => return Err(Error::Write(format!("not an XMB or XTB document: {other}"))),
    };
    let touched = touched(doc);
    let changed = changed_text(doc);
    let unit_of = |i: usize| doc.units.get(i).ok_or_else(|| Error::Write(format!("skeleton points at missing unit {i}")));
    match kind {
        Kind::Xmb if touched || force => {
            let mut original = String::new();
            for p in parts {
                match p {
                    SkelPart::Raw(s) => original.push_str(s),
                    SkelPart::Unit(i) => original.push_str(&code(unit_of(*i)?, RAW)),
                }
            }
            let xtb = msgs::xtb_from_xmb(&original, doc)?;
            Ok(xtb.into_bytes())
        }
        Kind::Xmb => {
            let mut out = String::new();
            for p in parts {
                match p {
                    SkelPart::Raw(s) => out.push_str(s),
                    SkelPart::Unit(i) => out.push_str(&code(unit_of(*i)?, RAW)),
                }
            }
            textenc::encode(&out, encoding)
        }
        Kind::Xtb => {
            let mut out = String::new();
            let mut first = true;
            for p in parts {
                match p {
                    SkelPart::Raw(s) => {
                        if first && (changed || force) {
                            match doc.target_lang.as_deref().and_then(lang::html_lang) {
                                Some(l) => out.push_str(&msgs::set_lang(s, &l)?),
                                None => out.push_str(s),
                            }
                        } else {
                            out.push_str(s);
                        }
                    }
                    SkelPart::Unit(i) => msgs::write_xtb_unit(unit_of(*i)?, &mut out),
                }
                first = false;
            }
            textenc::encode(&out, encoding)
        }
    }
}

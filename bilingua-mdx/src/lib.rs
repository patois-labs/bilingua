//! Bilingua filter for MDX (`.mdx`): Markdown with JSX components, `{expressions}` and ES modules, read the way the
//! Localization OS MDX filter reads it (its Markdown line rules, walk 3). See the README.
//!
//! * Markdown prose is translated line by line; heading marks, bullets, quote marks and numbers stay in the
//!   skeleton, fenced code, rules, table separators, link reference definitions (only a one-line title is a unit),
//!   HTML comment blocks, `import`/`export` blocks, flow JSX and flow `{expressions}` never are.
//! * Inline JSX and `{expressions}` inside a line are inline tags: a paired component is an open and a close tag, a
//!   self-closing component, an unpaired tag or an expression a standalone tag. Code spans and backslash escapes are
//!   text.
//! * String attributes of JSX tags are units after all line units when the attribute name is in the allowlist
//!   (`alt aria-label caption label placeholder title`); expression attributes never are.
//! * Front matter values of allowlisted keys come first.
//!
//! The file is never re-rendered: only the byte ranges of unit text are rewritten, and an untouched file comes back
//! byte for byte. Nothing is executed.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod codespan;
mod emph;
mod front;
mod mdline;
mod pyx;
mod render;
mod scan;
mod unitab;
mod walk;

use bilingua_model::*;
use std::collections::HashSet;

const ID: &str = "mdx";
/// The one hidden code of a unit: `open` = unit kind (`fm`, `line`, `title`, `attr`), `standalone` = the original
/// text of the replaced range, `close` = kind specific extra.
pub(crate) const HIDDEN: &str = "$mdx";

/// The JSX attributes whose string values are translated, by default.
pub const DEFAULT_ATTRS: &[&str] = &["alt", "aria-label", "caption", "label", "placeholder", "title"];

/// What an MDX extraction can be told (the LocOS settings `LOC_TMS_MDX_ATTRS` and `LOC_TMS_FRONT_MATTER_KEYS`).
#[derive(Debug, Clone)]
pub struct MdxOptions {
    /// JSX attribute names whose string values are units.
    pub attrs: Vec<String>,
    /// Front matter keys (case insensitive) whose one-line string values are units.
    pub front_matter_keys: Vec<String>,
}

impl Default for MdxOptions {
    fn default() -> Self {
        MdxOptions {
            attrs: DEFAULT_ATTRS.iter().map(|s| s.to_string()).collect(),
            front_matter_keys: front::DEFAULT_KEYS.iter().map(|s| s.to_string()).collect(),
        }
    }
}

/// The MDX filter.
pub struct Mdx;

struct Loaded {
    text: String,
    encoding: String,
    warnings: Vec<Warning>,
}

fn load(bytes: &[u8], opts: &ExtractOptions, limits: &Limits) -> Result<Loaded, Error> {
    guard::check_input(bytes, limits)?;
    let dec = enc::decode_text(bytes, &enc::DecodeOptions { declared: false, source_lang: opts.source_lang.as_deref(), limits: *limits })?;
    if !matches!(enc::base(&dec.encoding), "utf-8" | "utf-8-bom" | "utf-16le" | "utf-16be") && enc::encode_text(&dec.text, &dec.encoding)? != bytes {
        // double-mapped bytes of the CJK code pages read to the same character and write back as the other
        // spelling; an untouched merge must be the original bytes, so the file is refused, never altered
        return Err(Error::Encoding("This MDX file uses byte sequences of its legacy encoding that cannot be written back unchanged. Re-save the file as UTF-8.".into()));
    }
    Ok(Loaded { text: dec.text, encoding: dec.encoding, warnings: dec.warnings })
}

fn lc_set(v: &[String]) -> HashSet<String> {
    v.iter().map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()).collect()
}

impl Mdx {
    /// `extract` with explicit options.
    pub fn extract_with(&self, bytes: &[u8], opts: &ExtractOptions, mopts: &MdxOptions) -> Result<Document, Error> {
        let limits = opts.limits();
        let l = load(bytes, opts, &limits)?;
        let allow: HashSet<String> = mopts.attrs.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        let keys = lc_set(&mopts.front_matter_keys);
        let bom = enc::base(&l.encoding) == "utf-8-bom";
        let w = walk::Walk::new(&l.text, &allow, &keys, limits.max_units, bom)?;
        let (doc, mut warnings) = render::build_document(&w, &l, opts)?;
        let mut all = l.warnings;
        all.append(&mut warnings);
        guard::finish(doc, all, &limits)
    }
}

impl Filter for Mdx {
    fn id(&self) -> &'static str {
        ID
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["mdx"]
    }

    /// MDX is plain text; only the extension says so.
    fn sniff(&self, _bytes: &[u8]) -> bool {
        false
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        self.extract_with(bytes, opts, &MdxOptions::default())
    }

    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        render::merge(doc, opts)
    }
}

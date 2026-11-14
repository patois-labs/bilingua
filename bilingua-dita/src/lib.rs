//! Bilingua filters for DITA 1.x (`.dita`, `.ditamap`, `.bookmap`) and DocBook 4.5 / 5.x (`.dbk`, `.docbook`).
//!
//! Both are the `bilingua-xml` engine with a fixed dialect, as the Localization OS `DitaFilter` and `DocBookFilter`
//! are its `ItsXmlFilter` with a fixed rule set. The rule sets themselves ([`Dialect::dita`], [`Dialect::docbook`])
//! live in `bilingua-xml`, because the generic `xml` filter recognises both vocabularies on its own (LocOS sniffs
//! them for a `.xml` upload too); this crate holds the format identity: ids, extensions, strict sniffers and the
//! DITA / DocBook test suite. See the README for what is translated.
//!
//! ```
//! use bilingua_model::*;
//! let f = bilingua_dita::Dita::new();
//! let src = br#"<topic id="t"><title>Print a <ph>report</ph></title><body><p conref="a.dita#a/b"/></body></topic>"#;
//! let mut doc = f.extract(src, &ExtractOptions::new().with_target_lang("fr-FR")).unwrap();
//! assert_eq!(doc.units.len(), 1);
//! let id = doc.units[0].segments[0].id.clone();
//! let mut t = vec![Inline::text("Imprimer un ")];
//! t.extend(doc.units[0].source().into_iter().skip(1));
//! doc.set_target(&id, t).unwrap();
//! let out = String::from_utf8(f.merge(&doc).unwrap()).unwrap();
//! assert!(out.contains(r#"<topic id="t" xml:lang="fr-FR"><title>Imprimer un <ph>report</ph></title>"#));
//! assert!(out.contains(r#"<p conref="a.dita#a/b"/>"#));
//! ```

use bilingua_model::*;
pub use bilingua_xml::{Dialect, ItsRules};
use bilingua_xml::{Detect, XmlFilter, DITA_ARCH_NS, DOCBOOK_NS};

/// How far into the file the sniffers look for their marker before parsing.
const SNIFF_WINDOW: usize = 8192;

fn utf16(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF])
}

fn head_has(bytes: &[u8], needles: &[&[u8]]) -> bool {
    let head = &bytes[..bytes.len().min(SNIFF_WINDOW)];
    needles.iter().any(|n| head.windows(n.len()).any(|w| w == *n))
}

/// True when the generic engine, given only `strong` (a dialect without root-name guessing), recognises the file.
fn recognised(bytes: &[u8], strong: Dialect, name: &str) -> bool {
    XmlFilter::with_id("sniff", &[]).auto_dialects(vec![strong]).inspect(bytes).is_ok_and(|i| i.dialect.as_deref() == Some(name))
}

/// DITA topics and maps: the Localization OS `dita` filter.
#[derive(Debug, Clone, Default)]
pub struct Dita {
    rules: ItsRules,
}

impl Dita {
    pub fn new() -> Dita {
        Dita::default()
    }

    /// ITS rules applied after the DITA rule set and before the `<its:rules>` inside the document (LocOS
    /// `DitaFilter(rules=...)`, a user's rules file read with [`ItsRules::parse`]).
    pub fn with_its_rules(mut self, rules: ItsRules) -> Dita {
        self.rules = rules;
        self
    }

    /// The configured engine (for `inspect` and the like).
    pub fn engine(&self) -> XmlFilter {
        XmlFilter::with_id("dita", &["dita", "ditamap", "bookmap"]).dialect(Dialect::dita()).its_rules(self.rules.clone())
    }
}

impl Filter for Dita {
    fn id(&self) -> &'static str {
        "dita"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["dita", "ditamap", "bookmap"]
    }

    /// Strict: a well-formed XML file whose DOCTYPE names DITA (`-//OASIS//DTD DITA ...`) or whose root carries
    /// `ditaarch:DITAArchVersion`. A bare `<topic>` / `<map>` root without either is left to the generic XML
    /// filter, which reads it with the same DITA rules.
    fn sniff(&self, bytes: &[u8]) -> bool {
        if !utf16(bytes) && !head_has(bytes, &[b"DITA"]) {
            return false;
        }
        let strong = Dialect::dita().detect(Detect::new().with_doctype_contains(["DITA"]).with_root_attr(DITA_ARCH_NS, "DITAArchVersion"));
        recognised(bytes, strong, "dita")
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        check_own_options(self, &opts.format_options)?;
        self.engine().extract(bytes, opts)
    }

    fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
        check_own_options(self, &opts.format_options)?;
        self.engine().merge_with(doc, opts)
    }
}

/// DocBook 4.5 (no namespace, `lang`) and 5.x (`http://docbook.org/ns/docbook`, `xml:lang`): the Localization OS
/// `docbook` filter.
#[derive(Debug, Clone, Default)]
pub struct DocBook {
    rules: ItsRules,
}

impl DocBook {
    pub fn new() -> DocBook {
        DocBook::default()
    }

    /// ITS rules applied after the DocBook rule set and before the `<its:rules>` inside the document.
    pub fn with_its_rules(mut self, rules: ItsRules) -> DocBook {
        self.rules = rules;
        self
    }

    /// The configured engine (for `inspect` and the like).
    pub fn engine(&self) -> XmlFilter {
        XmlFilter::with_id("docbook", &["dbk", "docbook"]).dialect(Dialect::docbook()).its_rules(self.rules.clone())
    }
}

impl Filter for DocBook {
    fn id(&self) -> &'static str {
        "docbook"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["dbk", "docbook"]
    }

    /// Strict: a well-formed XML file whose DOCTYPE names DocBook or whose root element is in the DocBook 5
    /// namespace.
    fn sniff(&self, bytes: &[u8]) -> bool {
        if !utf16(bytes) && !head_has(bytes, &[b"DocBook", DOCBOOK_NS.as_bytes()]) {
            return false;
        }
        let strong = Dialect::docbook().detect(Detect::new().with_doctype_contains(["DocBook"]).with_root_ns([DOCBOOK_NS]));
        recognised(bytes, strong, "docbook")
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        check_own_options(self, &opts.format_options)?;
        self.engine().extract(bytes, opts)
    }

    fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
        check_own_options(self, &opts.format_options)?;
        self.engine().merge_with(doc, opts)
    }
}

/// Both filters, DITA first.
pub fn filters() -> Vec<Box<dyn Filter>> {
    vec![Box::new(Dita::new()), Box::new(DocBook::new())]
}

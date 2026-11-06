//! Bilingua generic XML filter: W3C ITS 2.0 rules, configurable dialects (DITA and DocBook ship with it) and
//! multilingual XML. See the README for the rule-driven API later XML formats configure.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod dialect;
mod its;
mod langs;
mod ml;
mod plan;
mod skel;
mod tree;
mod xpath;

pub use dialect::{AttrRule, Detect, Dialect, LangStamp, DITA_ARCH_NS, DOCBOOK_NS, ITS_NS};
pub use its::ItsRules;
pub use ml::{MlConfig, MlMode};

use bilingua_model::enc::{self, DecodeOptions};
use bilingua_model::*;
use std::collections::HashSet;
use tree::{Kind, Tree};

#[derive(Debug, Clone)]
enum Dialects {
    /// No bundled rules: only ITS rules in the document and those given to the filter.
    None,
    Fixed(Box<Dialect>),
    /// Recognised from the DOCTYPE, the root element or its namespace.
    Auto(Vec<Dialect>),
}

#[derive(Debug, Clone)]
enum MlPolicy {
    Auto,
    Off,
    Config(MlConfig),
}

/// The generic XML filter. `XmlFilter::new()` reads any XML: ITS 2.0 rules in the document, DITA and DocBook
/// recognised on their own, and multilingual layouts detected. Format crates configure their own instance with
/// [`XmlFilter::with_id`], [`XmlFilter::dialect`], [`XmlFilter::its_rules`] and [`XmlFilter::multilingual`].
#[derive(Debug, Clone)]
pub struct XmlFilter {
    id: &'static str,
    exts: &'static [&'static str],
    dialects: Dialects,
    rules: ItsRules,
    ml: MlPolicy,
}

impl Default for XmlFilter {
    fn default() -> Self {
        XmlFilter::new()
    }
}

/// What the filter found out about a file (see [`XmlFilter::inspect`]).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Inspection {
    /// The dialect that applies (`dita`, `docbook`, ...), if any.
    pub dialect: Option<String>,
    /// The multilingual layout, if the file is one.
    pub multilingual: Option<MlInfo>,
}

/// The multilingual layout of a file (see [`Inspection::multilingual`]).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct MlInfo {
    pub mode: MlMode,
    pub source: String,
    pub lang_attr: String,
    pub languages: Vec<String>,
}

impl XmlFilter {
    /// The generic filter: id `xml`, extension `xml`, DITA and DocBook recognised, multilingual detected.
    pub fn new() -> XmlFilter {
        XmlFilter { id: "xml", exts: &["xml"], dialects: Dialects::Auto(vec![Dialect::dita(), Dialect::docbook()]), rules: ItsRules::new(), ml: MlPolicy::Auto }
    }

    /// A filter with another id and extensions and no bundled dialect (plain ITS), multilingual detection off.
    pub fn with_id(id: &'static str, exts: &'static [&'static str]) -> XmlFilter {
        XmlFilter { id, exts, dialects: Dialects::None, rules: ItsRules::new(), ml: MlPolicy::Off }
    }

    /// Always use this dialect.
    pub fn dialect(mut self, d: Dialect) -> Self {
        self.dialects = Dialects::Fixed(Box::new(d));
        self
    }

    /// Recognise one of these dialects, in order (DOCTYPE, namespace and root attribute tests first, then root
    /// names).
    pub fn auto_dialects(mut self, ds: Vec<Dialect>) -> Self {
        self.dialects = Dialects::Auto(ds);
        self
    }

    /// No dialect at all, even for DITA and DocBook files.
    pub fn no_dialect(mut self) -> Self {
        self.dialects = Dialects::None;
        self
    }

    /// ITS rules applied before the ones inside the document.
    pub fn its_rules(mut self, r: ItsRules) -> Self {
        self.rules = r;
        self
    }

    /// Read files in this multilingual layout (a configured layout, not auto-detected).
    pub fn multilingual(mut self, c: MlConfig) -> Self {
        self.ml = MlPolicy::Config(c);
        self
    }

    /// Detect multilingual layouts (the default for `XmlFilter::new()`).
    pub fn detect_multilingual(mut self) -> Self {
        self.ml = MlPolicy::Auto;
        self
    }

    pub fn no_multilingual(mut self) -> Self {
        self.ml = MlPolicy::Off;
        self
    }

    fn ml_config(&self) -> Option<MlConfig> {
        match &self.ml {
            MlPolicy::Config(c) => Some(c.clone()),
            _ => None,
        }
    }

    fn pick_dialect(&self, t: &Tree) -> Option<Dialect> {
        match &self.dialects {
            Dialects::None => None,
            Dialects::Fixed(d) => Some((**d).clone()),
            Dialects::Auto(list) => {
                let doctype: &str = t.prolog.iter().find(|i| i.kind == Kind::Doctype && i.s < 4096).map_or("", |i| &t.text[i.s..i.e]);
                let root = t.root;
                let ns = t.el_ns(root);
                for d in list {
                    let det = &d.detect;
                    let by_doctype = det.doctype_contains.iter().any(|s| doctype.contains(s.as_str()));
                    let by_attr = det.root_attr.iter().any(|(n, l)| t.find_attr(root, n, l).is_some());
                    let by_ns = !ns.is_empty() && det.root_ns.iter().any(|n| n == ns);
                    if by_doctype || by_attr || by_ns {
                        return Some(d.clone());
                    }
                }
                if ns.is_empty() {
                    let local = t.local(root);
                    for d in list {
                        if d.detect.root_names.iter().any(|n| n == local) {
                            return Some(d.clone());
                        }
                    }
                }
                None
            }
        }
    }

    /// What the filter finds in a file: its dialect and multilingual layout.
    pub fn inspect(&self, bytes: &[u8]) -> Result<Inspection, Error> {
        let opts = ExtractOptions::default();
        let limits = opts.limits();
        guard::check_input(bytes, &limits)?;
        guard::check_xml(bytes, &limits)?;
        let dec = read_xml(bytes, &opts)?;
        let t = Tree::parse(&dec.text)?;
        if !matches!(self.ml, MlPolicy::Off) {
            if let Some(mp) = ml::plan(&t, self.ml_config().as_ref(), &limits, true, true)? {
                return Ok(Inspection {
                    dialect: Some("multilingual".into()),
                    multilingual: Some(MlInfo { mode: mp.layout.mode, source: mp.layout.source.clone(), lang_attr: mp.layout.lang_attr.clone(), languages: mp.layout.languages.clone() }),
                });
            }
        }
        Ok(Inspection { dialect: self.pick_dialect(&t).map(|d| d.name), multilingual: None })
    }

    fn do_extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        let limits = opts.limits();
        guard::check_input(bytes, &limits)?;
        guard::check_xml(bytes, &limits)?;
        let dec = read_xml(bytes, opts)?;
        let t = Tree::parse(&dec.text)?;
        if !matches!(self.ml, MlPolicy::Off) {
            if let Some(mp) = ml::plan(&t, self.ml_config().as_ref(), &limits, true, true)? {
                let mut doc = skel::build_document(&t, &mp.planned, &[String::new()], &HashSet::new(), opts, dec.encoding.clone(), None, self.id);
                doc.skeleton = Skeleton::text(vec![SkelPart::Raw(dec.text.clone())], dec.encoding.clone(), parser_info!());
                if let Some(u) = doc.units.first_mut() {
                    u.set_hidden(ML_CODE, mp.cfg.encode());
                }
                return guard::finish(doc, dec.warnings, &limits);
            }
        }
        let dialect = self.pick_dialect(&t);
        let props = its::eval_props(&t, dialect.as_ref(), &self.rules.rules, &limits)?;
        let mut pl = plan::Planner::new(&t, &props, &limits);
        pl.container(t.root, 0)?;
        let planned = pl.finish();
        let lang = dialect.as_ref().map(|d| d.lang.clone()).or_else(|| Some(LangStamp { update: vec!["xml:lang".into()], ..Default::default() })).and_then(|s| skel::lang_plan(&t, &s));
        let empty = HashSet::new();
        let alt = dialect.as_ref().map_or(&empty, |d| &d.alt_attrs);
        let doc = skel::build_document(&t, &planned, &props.notes, alt, opts, dec.encoding.clone(), lang, self.id);
        guard::finish(doc, dec.warnings, &limits)
    }
}

/// Decodes an XML file. Like the XML specification and LocOS, a file that is not UTF-8 or UTF-16 must say which
/// encoding it is in: an undeclared legacy byte stream is refused, never guessed.
fn read_xml(bytes: &[u8], opts: &ExtractOptions) -> Result<enc::Decoded, Error> {
    let dec = enc::decode_text(bytes, &DecodeOptions::new(opts).declared())?;
    if enc::is_legacy(&dec.encoding) && enc::declared_encoding(bytes).is_none() {
        return Err(Error::Encoding("This XML file is not UTF-8 and does not declare its encoding (<?xml version=\"1.0\" encoding=\"...\"?>). Re-save it as UTF-8, or add the declaration.".into()));
    }
    Ok(dec)
}

const ITS_RULES_KEY: &str = "xml.its_rules";
const MULTILINGUAL_KEY: &str = "xml.multilingual";

/// The `format_options` keys of the XML filter.
pub const FORMAT_OPTIONS: &[FormatOptionKey] = &[
    FormatOptionKey::text("xml.its_rules", "Content of an external ITS 2.0 rules file (<its:rules>), applied before the rules inside the document."),
    FormatOptionKey::text("xml.multilingual", "Multilingual layout as a JSON object: mode, source, lang_attr, source_xpath, target_xpath, create, enabled."),
];

/// Hidden code on the first unit of a multilingual document: the layout to replay on merge.
const ML_CODE: &str = "ml";

impl Filter for XmlFilter {
    fn id(&self) -> &'static str {
        self.id
    }

    fn extensions(&self) -> &'static [&'static str] {
        self.exts
    }

    /// Loose: any well-formed XML document without a DTD internal subset (XHTML, root `html`, excepted). Put this filter after every filter
    /// that owns a specific XML vocabulary.
    fn sniff(&self, bytes: &[u8]) -> bool {
        let head = bytes.iter().copied().find(|b| !(b.is_ascii_whitespace() || matches!(*b, 0xEF | 0xBB | 0xBF | 0xFF | 0xFE | 0)));
        if head != Some(b'<') && !bytes.starts_with(&[0xFF, 0xFE]) && !bytes.starts_with(&[0xFE, 0xFF]) {
            return false;
        }
        let opts = ExtractOptions::default();
        if guard::check_xml(bytes, &opts.limits()).is_err() {
            return false;
        }
        match read_xml(bytes, &opts) {
            // well-formed XHTML is the HTML filter's (it comes after this one)
            Ok(d) => Tree::parse(&d.text).is_ok_and(|t| !t.local(t.root).eq_ignore_ascii_case("html")),
            Err(_) => false,
        }
    }

    fn format_option_keys(&self) -> &'static [FormatOptionKey] {
        FORMAT_OPTIONS
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        let its = opts.format_option(ITS_RULES_KEY).filter(|t| !t.trim().is_empty());
        let ml = opts.format_option(MULTILINGUAL_KEY).filter(|t| !t.trim().is_empty());
        if its.is_none() && ml.is_none() {
            return self.do_extract(bytes, opts);
        }
        let mut f = self.clone();
        if let Some(text) = its {
            f.rules.rules.extend(ItsRules::parse(text.as_bytes())?.rules);
        }
        if let Some(json) = ml {
            f.ml = MlPolicy::Config(MlConfig::from_json(json)?);
        }
        f.do_extract(bytes, opts)
    }

    /// `stamp_target_always` declares the target language even when nothing is translated; `output_encoding`
    /// UTF-8 writes UTF-8 with the declaration saying so.
    fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        check_parser(doc, &parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("xml needs a text skeleton".into()));
        };
        let tag = doc.target_lang.as_deref().and_then(langs::html_lang);
        let out = match doc.units.first().and_then(|u| u.hidden(ML_CODE)) {
            Some(blob) => {
                let Some(SkelPart::Raw(text)) = parts.first() else {
                    return Err(Error::Write("multilingual xml needs its source text in the skeleton".into()));
                };
                ml::merge(doc, text, blob, tag.as_deref(), &Limits::default())?
            }
            None => {
                let stamp = opts.stamp_lang(doc, doc.units.iter().any(skel::translated)).is_some();
                skel::merge_parts(doc, parts, if stamp { tag } else { None })?
            }
        };
        enc::encode_text_with(&out, encoding, opts)
    }
}

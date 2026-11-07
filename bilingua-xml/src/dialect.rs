//! Rule sets: what is translatable, inline, nested, preformatted and where the language lives, per XML vocabulary.
//!
//! A [`Dialect`] is plain data, matched on element local names (so DocBook 4.5 without a namespace and DocBook 5
//! in its namespace share one set). The generic filter has none; DITA and DocBook ship as [`Dialect::dita`] and
//! [`Dialect::docbook`], and any later XML format builds its own with the same builder.

use std::collections::HashSet;

pub const ITS_NS: &str = "http://www.w3.org/2005/11/its";
pub const DOCBOOK_NS: &str = "http://docbook.org/ns/docbook";
pub const DITA_ARCH_NS: &str = "http://dita.oasis-open.org/architecture/2005/";

/// An attribute that carries translatable text on the named elements (`*` = on any element).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct AttrRule {
    pub attr: String,
    pub elements: Vec<String>,
}

/// How the document's language is written on a target-language merge.
///
/// The root element's first attribute in `update` that is present is changed. When none is present, `add` is
/// added (empty = nothing is added, the generic XML behaviour), or `add_if_root_outside` when the root element is
/// not in that namespace (DocBook 4.x writes `lang`, DocBook 5 and DITA write `xml:lang`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct LangStamp {
    pub update: Vec<String>,
    pub add: String,
    pub add_if_root_outside: Option<(String, String)>,
}

impl LangStamp {
    /// Changes nothing and adds nothing (the generic XML behaviour).
    pub fn new() -> LangStamp {
        LangStamp::default()
    }
    /// The root attributes to change, first present wins.
    pub fn with_update<I: IntoIterator<Item = S>, S: Into<String>>(mut self, attrs: I) -> LangStamp {
        self.update = attrs.into_iter().map(Into::into).collect();
        self
    }
    /// The attribute to add when none of `update` is present.
    pub fn with_add(mut self, attr: impl Into<String>) -> LangStamp {
        self.add = attr.into();
        self
    }
    /// The attribute to add instead when the root element is not in namespace `ns`.
    pub fn with_add_if_root_outside(mut self, ns: impl Into<String>, attr: impl Into<String>) -> LangStamp {
        self.add_if_root_outside = Some((ns.into(), attr.into()));
        self
    }
}

/// How a document is recognised as this dialect (any one test is enough).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Detect {
    /// Substrings of the DOCTYPE declaration (case sensitive), looked for in the first 4096 characters.
    pub doctype_contains: Vec<String>,
    /// Namespaces of the root element.
    pub root_ns: Vec<String>,
    /// An attribute (namespace, local name) on the root element.
    pub root_attr: Vec<(String, String)>,
    /// Local names of a root element that has no namespace.
    pub root_names: Vec<String>,
}

fn strings<I: IntoIterator<Item = S>, S: Into<String>>(v: I) -> Vec<String> {
    v.into_iter().map(Into::into).collect()
}

impl Detect {
    /// Recognises nothing.
    pub fn new() -> Detect {
        Detect::default()
    }
    pub fn with_doctype_contains<I: IntoIterator<Item = S>, S: Into<String>>(mut self, v: I) -> Detect {
        self.doctype_contains = strings(v);
        self
    }
    pub fn with_root_ns<I: IntoIterator<Item = S>, S: Into<String>>(mut self, v: I) -> Detect {
        self.root_ns = strings(v);
        self
    }
    /// Adds an attribute (namespace, local name) on the root element.
    pub fn with_root_attr(mut self, ns: impl Into<String>, local: impl Into<String>) -> Detect {
        self.root_attr.push((ns.into(), local.into()));
        self
    }
    pub fn with_root_names<I: IntoIterator<Item = S>, S: Into<String>>(mut self, v: I) -> Detect {
        self.root_names = strings(v);
        self
    }
}

#[derive(Debug, Clone, Default)]
#[non_exhaustive]
pub struct Dialect {
    pub name: String,
    /// Elements whose content (and their descendants') is never translated.
    pub no_translate: HashSet<String>,
    /// Elements that stay inside the surrounding unit as paired (or, when empty, point) tags.
    pub inline: HashSet<String>,
    /// Elements that are their own unit and leave a point tag in the parent.
    pub nested: HashSet<String>,
    /// Elements whose white space is kept.
    pub preserve: HashSet<String>,
    pub attrs: Vec<AttrRule>,
    /// Attributes (local names) that are image alternative text; their unit style is `a11y:img-alt`.
    pub alt_attrs: HashSet<String>,
    /// An attribute on any element that says `yes` or `no` (DITA `translate`).
    pub translate_attr: Option<String>,
    /// Elements carrying one of these attributes pull their content from elsewhere: their content and descendants are
    /// not translated; their own translatable attributes still are (DITA 1.3 conref attribute rules), unless set to
    /// `-dita-use-conref-target`.
    pub no_translate_if_attr: Vec<String>,
    pub lang: LangStamp,
    pub detect: Detect,
    /// ITS rules that belong to the vocabulary (a localization note on `<note>`, a `locNotePointer`, white space
    /// rules). Applied before the rules given to the filter and the ones inside the document.
    pub its_rules: crate::its::ItsRules,
}

fn set<I, S>(v: I) -> HashSet<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    v.into_iter().flat_map(|s| s.as_ref().split_whitespace().map(str::to_string).collect::<Vec<_>>()).collect()
}

impl Dialect {
    pub fn new(name: impl Into<String>) -> Dialect {
        Dialect { name: name.into(), ..Default::default() }
    }
    /// Each argument may hold several names separated by white space.
    pub fn no_translate<I: IntoIterator<Item = S>, S: AsRef<str>>(mut self, names: I) -> Self {
        self.no_translate.extend(set(names));
        self
    }
    pub fn inline<I: IntoIterator<Item = S>, S: AsRef<str>>(mut self, names: I) -> Self {
        self.inline.extend(set(names));
        self
    }
    pub fn nested<I: IntoIterator<Item = S>, S: AsRef<str>>(mut self, names: I) -> Self {
        self.nested.extend(set(names));
        self
    }
    pub fn preserve<I: IntoIterator<Item = S>, S: AsRef<str>>(mut self, names: I) -> Self {
        self.preserve.extend(set(names));
        self
    }
    /// `attr` is translatable on `elements` (empty = any element).
    pub fn translatable_attr<I: IntoIterator<Item = S>, S: AsRef<str>>(mut self, attr: &str, elements: I) -> Self {
        self.attrs.push(AttrRule { attr: attr.to_string(), elements: elements.into_iter().flat_map(|s| s.as_ref().split_whitespace().map(str::to_string).collect::<Vec<_>>()).collect() });
        self
    }
    pub fn alt_attrs<I: IntoIterator<Item = S>, S: AsRef<str>>(mut self, names: I) -> Self {
        self.alt_attrs.extend(set(names));
        self
    }
    pub fn translate_attr(mut self, name: &str) -> Self {
        self.translate_attr = Some(name.to_string());
        self
    }
    pub fn no_translate_if_attr<I: IntoIterator<Item = S>, S: AsRef<str>>(mut self, names: I) -> Self {
        self.no_translate_if_attr.extend(names.into_iter().map(|s| s.as_ref().to_string()));
        self
    }
    pub fn lang(mut self, l: LangStamp) -> Self {
        self.lang = l;
        self
    }
    pub fn detect(mut self, d: Detect) -> Self {
        self.detect = d;
        self
    }
    /// Adds the rules of an `<its:rules>` document (see [`crate::ItsRules::parse`]).
    pub fn its_rules(mut self, r: crate::its::ItsRules) -> Self {
        self.its_rules.rules.extend(r.rules);
        self
    }

    /// DITA 1.x topics and maps.
    pub fn dita() -> Dialect {
        const CODE: &str = "codeph apiname cmdname filepath msgnum option parmname varname xmlelement xmlatt xmlnsname xmlpi numcharref parameterentity textentity";
        Dialect::new("dita")
            .no_translate([CODE, "codeblock coderef draft-comment required-cleanup data data-about sort-as unknown", "foreign critdates permissions resourceid"])
            .inline([
                CODE,
                "ph b i u sup sub tt line-through overline xref keyword term cite q tm uicontrol",
                "wintitle menucascade shortcut synph systemoutput userinput msgph abbreviated-form",
                "text image state boolean data sort-as markupname delim sep repsep oper kwd var",
                "fragref equation-inline",
            ])
            .nested(["fn indexterm index-see index-see-also index-sort-as draft-comment required-cleanup"])
            .preserve(["codeblock pre lines msgblock screen"])
            .translatable_attr("navtitle", ["*"])
            .translatable_attr("title", ["map bookmap"])
            .translatable_attr("alt", ["image"])
            .alt_attrs(["alt"])
            .translate_attr("translate")
            .no_translate_if_attr(["conref", "conkeyref"])
            .lang(LangStamp { update: vec!["xml:lang".into()], add: "xml:lang".into(), add_if_root_outside: None })
            .detect(Detect {
                doctype_contains: vec!["DITA".into()],
                root_ns: vec![],
                root_attr: vec![(DITA_ARCH_NS.into(), "DITAArchVersion".into())],
                root_names: "topic concept task reference glossentry glossgroup troubleshooting map bookmap subjectScheme dita learningContent learningOverview learningSummary learningAssessment learningPlan"
                    .split(' ')
                    .map(String::from)
                    .collect(),
            })
    }

    /// DocBook 4.5 and 5.x.
    pub fn docbook() -> Dialect {
        const CODE: &str = "literal code computeroutput userinput filename envar varname function parameter constant command option classname methodname property tag sgmltag uri email systemitem keycode keysym markup token symbol type returnvalue errorcode prompt package";
        Dialect::new("docbook")
            .no_translate([CODE, "programlisting screen synopsis cmdsynopsis funcsynopsis classsynopsis fieldsynopsis", "methodsynopsis constructorsynopsis destructorsynopsis remark"])
            .inline([
                CODE,
                "emphasis phrase link ulink xref olink footnoteref anchor quote citetitle firstterm",
                "glossterm acronym abbrev trademark productname productnumber application guilabel",
                "guibutton guimenu guimenuitem guisubmenu guiicon keycap keycombo menuchoice shortcut",
                "mousebutton subscript superscript inlinemediaobject inlinegraphic inlineequation",
                "replaceable wordasword foreignphrase citation citerefentry refentrytitle manvolnum",
                "errorname errortext errortype database hardware lineannotation co coref",
            ])
            .nested(["footnote indexterm remark"])
            .preserve(["programlisting screen literallayout synopsis address funcsynopsisinfo classsynopsisinfo"])
            .translatable_attr("xreflabel", ["*"])
            .lang(LangStamp {
                update: vec!["xml:lang".into(), "lang".into()],
                add: "xml:lang".into(),
                add_if_root_outside: Some((DOCBOOK_NS.into(), "lang".into())),
            })
            .detect(Detect { doctype_contains: vec!["DocBook".into()], root_ns: vec![DOCBOOK_NS.into()], root_attr: vec![], root_names: vec![] })
    }
}

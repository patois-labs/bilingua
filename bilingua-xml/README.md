# bilingua-xml

Generic XML for Bilingua: W3C ITS 2.0 rules, configurable dialects (DITA and DocBook ship with it) and multilingual
XML. It reads any well-formed XML, turns the translatable text into units, and merges translations back changing
nothing else: declaration, DOCTYPE, comments, processing instructions, CDATA, attribute order and quoting, white
space and line ends stay byte for byte. It matches the Localization OS `xml_its` / `xml_multilingual` filters unit for
unit and byte for byte (see "Tested against").

This crate is the base the XML-family formats build on. A format such as DITA, DocBook, a vendor vocabulary or a
resource file is **a `Dialect` (plain data) handed to `XmlFilter`**, not new parsing code.

```rust
use bilingua_model::*;
use bilingua_xml::XmlFilter;

let f = XmlFilter::new();                       // any XML; DITA and DocBook recognised; multilingual detected
let mut doc = f.extract(&bytes, &ExtractOptions::new().with_target_lang("fr-FR"))?;
doc.set_target("u3", vec![Inline::text("Bonjour")])?;
let out = f.merge(&doc)?;                       // root xml:lang becomes fr-FR where the vocabulary has one
```

## What is a unit

Decided the ITS 2.0 way (<https://www.w3.org/TR/its20/>), plus the dialect:

- **Translate**: element content is translatable unless a rule says no, inherited by descendants. Attributes are never
  translatable unless a rule says yes (`its:translateRule` on `//x/@title`, or the dialect). A local `its:translate`
  wins over global rules; among global rules the last wins.
- **Within text**: an element with `withinText="yes"` stays inside the surrounding unit as an `Open`/`Close` tag pair
  (an empty one is one `Standalone` tag); `"nested"` is its own unit and leaves a `Standalone` tag in the parent; `"no"`
  (the default) starts a new unit. A non-translatable element inside text is one `Standalone` tag holding the whole
  element (its markup is in the unit's codes).
- **Localization notes**: `its:locNote` (attribute or `its:locNoteRule`, text or `locNotePointer`), inherited, become
  `Unit::notes` (kind `locNote`) and `context.notes`.
- **Preserve space**: `xml:space="preserve"` or `its:preserveSpaceRule` keeps white space in the unit; elsewhere a run of
  white space shows as one space and the leading and trailing white space of the run is kept outside the unit text.
- Comments, processing instructions and undefined entity references (`&product;`, allowed only with a DOCTYPE) inside
  running text are `Standalone` tags. Predefined entities and character references are decoded to text and written
  back escaped. A run that is one CDATA section stays CDATA (`]]>` in a translation is split safely).
- Unit ids are `u1`, `u2`, ... in document order. `context.key` is the element path (`/a/b/@title` for an attribute),
  `context.style` is `attr:<name>` or `a11y:img-alt` for an attribute unit, `context.location` the line.
- Tag ids are the LocOS token numbers, so `bilingua_model::profile::tokens::to_locos_text` gives exactly `⟦g1⟧`,
  `⟦x2⟧` as the LocOS filter does. Display text: the start tag for an open tag, the end tag for a close
  tag, the raw markup (shortened to `<name>...</name>` when long) for a standalone one.

ITS rules come from three places, later wins: the dialect (`Dialect::its_rules`), the filter (`XmlFilter::its_rules`)
and `<its:rules>` inside the document. Selectors are XPath 1.0 (all axes, predicates, unions, the core functions),
evaluated by this crate's own engine. Linked rules (`xlink:href`) are never fetched. A selector that would be
quadratic (`following`/`preceding` axes, a search from the root inside a predicate, more than two nested predicates,
a pointer that does not stay below the selected element) is refused with a plain message, as is more than 100 rules.

## The configuration API (for DITA, DocBook and the rest)

```rust
use bilingua_xml::{Dialect, Detect, LangStamp, ItsRules, XmlFilter};

let recipe = Dialect::new("recipe")
    .no_translate(["sku code"])                  // never translated (names may be separated by white space)
    .inline(["b i br code"])                     // within text: Open/Close pairs, empty ones Standalone
    .nested(["footnote"])                        // own unit, a Standalone tag in the parent
    .preserve(["pre"])                           // white space kept
    .translatable_attr("title", ["step"])        // attribute text (empty element list = any element)
    .alt_attrs(["alt"])                          // attribute units styled a11y:img-alt
    .translate_attr("tr")                        // an attribute that says yes / no (DITA `translate`)
    .no_translate_if_attr(["ref"])               // content pulled from elsewhere (DITA conref)
    .lang(LangStamp { update: vec!["locale".into()], add: "locale".into(), add_if_root_outside: None })
    .detect(Detect { root_names: vec!["recipe".into()], ..Default::default() })
    .its_rules(ItsRules::parse(its_rules_xml)?); // notes, extra translate or space rules, pointers

let f = XmlFilter::with_id("recipe", &["rcp"]).dialect(recipe);   // id and extensions for the facade
```

| Rule | Where |
|---|---|
| translatable elements | everything, minus `Dialect::no_translate`, `translate_attr`, `no_translate_if_attr`, ITS |
| translatable attributes | `translatable_attr`, ITS `translateRule` on `@attr` |
| within-text elements | `inline`, `nested`, ITS `withinTextRule`, local `its:withinText` |
| notes | ITS `locNoteRule` / `its:locNote`; a dialect ships its own through `its_rules` |
| preserve space | `preserve`, `xml:space`, ITS `preserveSpaceRule` |
| locale attribute | `Dialect::lang` (`LangStamp`): the first attribute of `update` present on the root is changed, else `add` is added (empty = never added), or `add_if_root_outside` when the root is not in that namespace (DocBook 4.x `lang` against DocBook 5 `xml:lang`) |
| recognition | `Dialect::detect`: DOCTYPE text, root namespace, root attribute, root names; `XmlFilter::auto_dialects` tries a list |

`XmlFilter::new()` is the generic `xml` filter with DITA and DocBook recognised and multilingual layouts detected.
`with_id(id, exts)` starts from nothing (no dialect, no multilingual detection) for a format crate;
`.dialect(d)` fixes the dialect, `.auto_dialects(v)` recognises one of several, `.no_dialect()`, `.its_rules(r)`,
`.multilingual(cfg)` / `.detect_multilingual()` / `.no_multilingual()`. `XmlFilter::inspect(bytes)` reports the
dialect and the multilingual layout. Everything on `Dialect` is public data; clone one and change it to derive another.

## Target-language merge

Every translated merge with `Document::target_lang` set writes the language the vocabulary has: the root language
attribute per `LangStamp`, in BCP 47 form (`pt_br` becomes `pt-BR`; an unusable code leaves the file alone). It only
happens when at least one unit has a target, so an untouched merge is always the original bytes. Generic XML changes
an `xml:lang` the root already has and adds none; DITA and DocBook add one. XML has no direction attribute, so none is
written for right-to-left targets.

## Multilingual XML

One file holding the same content in several languages as sibling nodes, as the Trados "Multilingual XML" file type:
`attr` (`<text xml:lang="en">`/`<text xml:lang="de">`), `element` (`<en>`/`<fr>`) and `xpath` (`source_xpath` and
`target_xpath` with a `{lang}` placeholder, evaluated from each source node). Detected on its own when at least two
groups each hold two or more languages, or configured with `MlConfig::from_json` (`mode`, `source`, `lang_attr`,
`source_xpath`, `target_xpath`, `create`, `enabled`) or an in-file `<?loc-multilingual key="value" ...?>` before
the root. Units are the source-language nodes. With `target_lang` set, a merge writes each translation into the target
sibling: an existing one gets its content replaced (an empty `<x lang="fr"/>` is opened), a missing one is created after
the group with the source's indentation and start tag (`id` / `xml:id` dropped), nothing else changes. The root
language is left alone. With no `target_lang` the translation replaces the source node's content in place. A requested
`fr-FR` finds the file's own `fr`; a new language is written in the file's style (`en_US` gives `fr_FR`).

## Safety

- The B6 guards run first: `guard::check_input`, `guard::check_xml` (no `<!ENTITY`, so no entity bombs or external
  entities; depth and element limits; UTF-16 looked into), `check_unit_count` and `guard::finish`. A DTD **internal
  subset** is refused outright (`Error::Parse`), as LocOS does; an external DOCTYPE is allowed and never fetched, and
  an undefined `&name;` is then kept as a tag.
- Well-formedness is checked like libxml2 does: names, characters, duplicate attributes, undeclared namespace
  prefixes, `]]>` in text, `--` in comments, undefined entities without a DOCTYPE, text outside the root, a second
  byte order mark. Everything is a typed `Error` (`Parse`, `Limit`, `Encoding`), never a panic.
- The planner, the skeleton emitter and the merge use explicit stacks, not recursion, so any accepted depth is safe on
  a 1 MB stack (wasm, debug builds; `tests/deep.rs`); nesting past 1024 levels is a `Limit` error whatever
  `Limits::max_xml_depth` says. A merge refuses a skeleton that names a unit twice (`Write`). Every XPath evaluation has a
  step budget and a string budget (16 x the text + 32 MiB per document), and rules x elements and pointer hits x elements are bounded by `Limits::max_units` as in LocOS.

## Skeleton (for tools that read it)

`Skeleton::Text` parts are the file text with `Unit(i)` slots. Inside `Raw` parts and inside a unit's code markup, NUL
(which XML forbids) delimits markers: `\0U<n>\0` the content of unit `n` (a nested unit inside a protected element),
`\0A<n>\0` the value of attribute unit `n`, `\0L<attr>|<0/1>\0 ... \0l\0` around the root start tag. Hidden codes start
with `~`: `~raw` the original markup, `~lead` / `~trail` surrounding white space, `~cdata`, `~q` an attribute's quote.
A multilingual document keeps the whole text in one `Raw` part and the layout in a `~ml` code on its first unit, and
finds its target nodes at merge time (the target language is only known then).

## Tested against

- The Localization OS suites `verify_xml_its_filter` (12 cases), `verify_xml_multilingual_filter` (11) and the
  generic-XML cases of `verify_xml_resource_filters`, ported to `tests/locos_*.rs` and `tests/formats.rs`.
- `tests/locos_golden.rs`: 383 documents (the oracle corpus, the LocOS fixtures, 360 random ones: ITS rules, local ITS
  markup, DITA, DocBook 4.5 and 5, every multilingual layout) with LocOS's units, styles, notes and merged bytes
  (`tests/tools/gen_golden.py`): all agree.
- Okapi `okf_xml` on Okapi's XML and ITS 2.0 test files (`tests/tools/okapi_compare.py`); see the report.
- Round trips, 11 languages, legacy encodings (Windows-1252, Shift_JIS, GBK, EUC-KR, Windows-1256, UTF-16), hostile
  input (proptest over random bytes, XML soup and mutated fixtures), a 6 MB speed test.

## Differences from LocOS (all deliberate)

- A merge with nothing translated is the original bytes, also with a target language (LocOS stamps `xml:lang` even
  when nothing was translated).
- Encodings follow decision B-bytes through `bilingua_model::enc`: a changed file is written back in its original
  encoding when the text fits, otherwise as UTF-8 with the declaration saying so. LocOS's own `build` writes UTF-8
  and lets a later layer restore the envelope; the golden test compares after reading our bytes in that encoding.
- Nesting is limited by `Limits::max_xml_depth` (256) where LocOS refused more than 120 levels.
- Unicode white space: Python's `str.isspace`, exactly (`bilingua_model::py_isspace_char`).
- The LocOS XML dispatcher hands Android `strings.xml` and Java XML properties to their own filters; this crate is only
  the generic filter (those formats are other crates).

## Not done

- ITS data categories a translation filter does not use (provenance, terminology, text analysis, locale filter, target
  pointer, id value, storage size, allowed characters, MT confidence, LQI / LQR, domain, directionality, language
  information, external resource): LocOS ignores them and so does this crate. Okapi's default configuration turns some
  of them into extra units.
- `its:param` variables (`$name`) in selectors: refused, as LocOS does. Linked rules are not fetched.
- `its:locNoteRef` (a note in another file or element by reference) without `locNote` text or a pointer is refused.
- DTD internal subsets; external entities; XInclude.

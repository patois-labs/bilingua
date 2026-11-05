# bilingua-dita

DITA 1.x (`.dita`, `.ditamap`, `.bookmap`) and DocBook 4.5 / 5.x (`.dbk`, `.docbook`) for Bilingua: two filters,
`Dita` (id `dita`) and `DocBook` (id `docbook`), on the `bilingua-xml` engine with a fixed dialect. They match the
Localization OS `DitaFilter` and `DocBookFilter` (`app/filters/xml_its.py`) unit for unit and byte for byte.

```rust
use bilingua_model::*;
let f = bilingua_dita::Dita::new();          // or DocBook::new(); .with_its_rules(ItsRules::parse(rules)?)
let mut doc = f.extract(&bytes, &ExtractOptions::new().with_target_lang("ja-JP"))?;
doc.set_target("u1", vec![Inline::text("...")])?;
let out = f.merge(&doc)?;                   // root xml:lang (DITA, DocBook 5) or lang (DocBook 4.x) = ja-JP
```

## Why one crate on bilingua-xml, with the rule sets in bilingua-xml

The rule sets (`bilingua_xml::Dialect::dita()` and `::docbook()`) stay in `bilingua-xml` because the generic `xml`
filter must recognise both vocabularies on its own, as LocOS's generic filter does for a `.xml` upload. Moving them
here would make `bilingua-xml` depend on this crate or keep two copies. What a format needs beyond the rules is its
identity: the format ids and extensions the facade lists, strict sniffers, a dialect that is fixed rather than
guessed (LocOS's `dita` filter applies the DITA rules to any root), and its own test suite. That is a few lines for
each vocabulary over the same engine, so one crate holds both; two crates would duplicate the harness and the
golden machinery for nothing.

## What is translated (all from LocOS)

DITA: element text everywhere, except code (`codeph`, `codeblock`, `filepath`, `cmdname`, `apiname`, `option`,
`varname`, `msgnum`, `xmlelement`, ...), `draft-comment`, `required-cleanup`, `data`, `foreign`, `unknown`,
`critdates`, `permissions`, `resourceid`; elements with `translate="no"` (and their content, until a
`translate="yes"` inside); elements with `conref` or `conkeyref` (their content is pulled in at publish time and
left untouched). `keyref` is an ordinary attribute: an empty `<keyword keyref="k"/>` is one code, a `<ph keyref="k">`
with fallback text keeps the text translatable, and the attribute is never changed. Inline (`Open`/`Close` codes,
empty ones `Standalone`): `ph b i u sup sub tt line-through overline xref keyword term cite q tm uicontrol wintitle
menucascade shortcut synph systemoutput userinput msgph abbreviated-form text image state boolean ...`; protected code
inside text is one `Standalone` code holding the element. Own units with a `Standalone` code in the parent: `fn`,
`indexterm`, `index-see`, `index-see-also`, `index-sort-as`. Attributes: `navtitle` anywhere, `title` on `map` /
`bookmap`, `alt` on `image` (style `a11y:img-alt`). White space kept in `codeblock pre lines msgblock screen`.

DocBook: element text, except code (`literal code computeroutput userinput filename envar varname function command
option classname ... prompt package`), `programlisting screen synopsis` and the other synopses, and `remark`. Inline:
`emphasis phrase link ulink xref olink footnoteref anchor quote citetitle firstterm glossterm acronym abbrev
trademark productname application gui* keycap keycombo menuchoice shortcut subscript superscript
inlinemediaobject replaceable foreignphrase citation ...`. Own units: `footnote`, `indexterm`, `remark`. Attribute:
`xreflabel` anywhere. White space kept in `programlisting screen literallayout synopsis address`.

ITS 2.0 rules apply on top, later wins: the dialect, the filter's (`with_its_rules`), then `<its:rules>` in the
document; `its:translate`, `its:withinText`, `its:locNote` and `xml:space` locally.

Target language on a translated merge: DITA updates the root `xml:lang` or adds one; DocBook updates `xml:lang`,
else `lang`, else adds `lang` when the root is outside the DocBook 5 namespace (4.x) and `xml:lang` inside it.

## Safety

The `bilingua-xml` guards: no DTD is ever loaded or fetched (the public ids and URLs DITA and DocBook files carry
are kept as text; an entity they would define stays a code), an internal subset is refused (entity bombs, external
entities, parameter entities), XInclude is not followed, depth, element, unit and size limits are typed errors, and
the crate has no file, network or clock access (it builds for `wasm32-unknown-unknown`).

## Tested against

- `tests/locos_its.rs`: the 12 cases of LocOS `verify_xml_its_filter.py` through these filters (six are DITA /
  DocBook cases there; the other six are generic-XML cases run here on DITA and DocBook documents);
  `verify_xml_resource_filters.py` has no DITA or DocBook case.
- `tests/locos_golden.rs`: 390 documents (LocOS fixtures, the oracle corpus, hand-made edge cases, 370 random DITA
  topics, maps, bookmaps, DocBook 4.5 / 5 and unrecognised roots) with LocOS's units, styles, notes and built bytes
  for two mappings, in place and for de-DE, ar and ja-JP (`tests/tools/gen_golden.py`).
- `tests/rules.rs` (conref / conkeyref / keyref, `translate`, inline codes, the DocBook inline set, fixed dialect,
  sniffers, corpus round trips), `tests/languages.rs` (11 languages, nine legacy code pages, UTF-16),
  `tests/hostile.rs` (proptest, DTDs never fetched), `tests/speed.rs` (5 MB).
- Okapi `okf_xmlstream-dita` on Okapi's 48 DITA test files and `okf_xml` (ITS) on its two DocBook samples
  (`tests/tools/okapi_compare.py`); see the report.

## Differences from LocOS (all deliberate, inherited from bilingua-xml)

- A merge with nothing translated is the original bytes, also with a target language (`MergeOptions::stamp_target_always`
  gives LocOS's stamp).
- A changed file is written in its original encoding when the text fits (UTF-16 stays UTF-16), else UTF-8; LocOS
  always writes UTF-8 (`OutputEncoding::Utf8`).
- Depth limit 256 (LocOS 120). A repeated `xml:id` is read (lxml refuses it as an ID error).

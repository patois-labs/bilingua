# bilingua-idml

The Adobe InDesign parser for Bilingua. It opens an `.idml` package, gives you one unit per visible paragraph with
character formatting kept as tags, and writes the translation back into a package InDesign can open. It is a Rust
port of the Localization OS filter (`app/filters/idml_filter.py`) and gives the same units in the same order.

```rust
use bilingua_idml::Idml;
use bilingua_model::*;

let f = Idml;
let mut doc = f.extract(&bytes, &ExtractOptions::new().with_target_lang("de-DE"))?;
doc.set_target("p0", vec![Inline::text("Hallo.")])?;
let out = f.merge(&doc)?;
```

For the facade (`bilingua/src/lib.rs`, `filters()`): `Box::new(bilingua_idml::Idml)`.

## How it works

- The package is read in memory with the shared zip guard (`bilingua-zip`) plus a check of the central directory
  for repeated or backslash member names. Nothing is written to disk.
- `designmap.xml` lists the stories; their order is the unit order. Stories whose every text frame is hidden,
  non-printing or on a hidden or non-printing layer are skipped, as is text under a condition whose `Visible` is
  false, `HiddenText`, notes, and tracked deletions.
- Unit ids are `p<N>`, where N counts every paragraph of every story in unit order, empty ones included (LocOS's
  `p_index`), so ids have gaps. Inside a story the body paragraphs come first, then table cells, footnotes and
  endnotes (breadth first).
- The skeleton holds the original package. An untouched merge returns it unchanged. A translated merge re-reads
  it, rewrites **only the stories that have a translated paragraph** (every other member is copied, deflated bytes
  and time stamps included) and writes `mimetype` first and stored.

## Inline tags

Tag ids are the LocOS token numbers (one counter per paragraph, in source order), so the LocOS adapter maps them
one to one. The `label` of a tag says which token letter LocOS uses.

| Tag | Kind | Label | LocOS token |
|---|---|---|---|
| Character range that differs from the paragraph's main formatting | open/close | `run` | `r` |
| Hyperlink source, `XMLElement`, tracked insertion (`Change`) | open/close | `link`, `xml-tag`, `change` | `g` |
| A range nested in one of those | open/close | `range` | `g` |
| Tab, forced line break (U+2028), paragraph separator (U+2029) | standalone | `tab`, `line-break`, `paragraph-separator` | `x` |
| `<?ACE n?>` special character (page number, indent to here, footnote number) | standalone | `special-char` | `x` |
| `<Br/>` inside a hyperlink or XML tag | standalone | `br` | `x` |
| Index marker, hyperlink destination, text variable, note, anchored frame, table, footnote, hidden text | standalone | the element name in lower case | `x` |

The main formatting of a paragraph (the formatting with the most text) needs no tag. Raw markup of every tag is in
`unit.codes`. A code the translation drops is put back at the end of its paragraph; one it repeats or invents is
ignored: no element is ever lost or duplicated.

## Target language and direction

When at least one unit is translated and `target_lang` is set, every rebuilt range gets `AppliedLanguage` set to the
InDesign language that `designmap.xml` itself declares for the target (never an invented one). For a right-to-left
target (Arabic, Hebrew, Persian, Urdu and the other LocOS right-to-left languages) the paragraph range of every
translated paragraph gets `ParagraphDirection="RightToLeftDirection"` and the Adobe World-Ready composer, and an
explicit left-to-right `CharacterDirection` becomes `DefaultDirection`.

## The story-markup module

`bilingua_idml::story` and `bilingua_idml::xml` are public and know nothing about zips: the same markup is used by
InCopy ICML stories, so `bilingua-icml` can depend on them (`story::collect`, `tokenize`, `rebuild`,
`stamp_language`, `stamp_rtl`).

## Not supported

- Spreads, master pages and resources are never translated (text lives in stories). Text on a path and text in
  frames that have no story are not reachable.
- `ExtractOptions::include_locked` is ignored: hidden and non-printing text is skipped, not shown locked.
- Namespaced default elements (`xmlns="..."` on the story) are not recognised, as in LocOS.

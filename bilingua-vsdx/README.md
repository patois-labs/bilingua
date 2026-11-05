# bilingua-vsdx

The Microsoft Visio parser for Bilingua: `.vsdx` and macro-enabled `.vsdm` (Visio 2013+ OPC packages, [MS-VSDX]).

```rust
use bilingua_vsdx::Vsdx;
use bilingua_model::*;

let mut doc = Vsdx.extract(&bytes, &ExtractOptions::new().with_target_lang("de-DE"))?;
doc.set_target("page2-1", vec![Inline::text("Anfang")])?;
let out = Vsdx.merge(&doc)?;
let with_comments = Vsdx.extract_with_comments(&bytes, &ExtractOptions::default())?; // comments as units too
```

Facade (`filters()`): `Box::new(bilingua_vsdx::Vsdx)`.

## What becomes a unit (LocOS order)

1. **Page names**: the `Name` of each `<Page>` in `visio/pages/pages.xml` (`name-N`). `NameU`, the universal name
   formulas refer to, is never touched; a page that has no `NameU` gets one holding the old name when renamed.
2. For each page, in `pages.xml` order (the part is found through `pages.xml.rels`; without `pages.xml`,
   `page<N>.xml` in numeric order), in document order: every shape `<Text>` and every shape data label
   (`<Section N="Property">` row, `<Cell N="Label" V="..">` with no formula). Ids are `page<N>-<k>`, k counting every
   text and label of the part, so ids have gaps.
3. With `extract_with_comments`: the text of each `<CommentEntry>` of the comments part (`comment-N`, style
   "comment"). Authors, ids and dates are never touched.

Shape text is mixed content: characters between empty `<cp>`, `<pp>`, `<tp>` run markers and `<fld>` fields. Each of
them is a standalone tag (`TagKind::Standalone`, LocOS `x` token) holding its markup; a field keeps its cached value
inside the tag. Markers and white space before the first and after the last character (usually the closing `\n`)
stay outside the unit; a field counts as content. A tag the translation drops is appended at the end of the text, so
no run marker or field is lost. Masters (`visio/masters/`) are left alone, as LocOS does.

## Writing

Only the parts that hold a unit are rebuilt from a template of the original markup; a unit whose target equals its
source is written from its stored original, so an unchanged drawing keeps every part byte for byte (the zip itself
is rewritten with each entry's order, method and time stamp kept; untouched entries keep their deflated bytes).
A changed part keeps its own XML declaration, quotes and white space (LocOS re-serialises it with lxml).

With `target_lang` set, every translated shape also gets (LocOS `build_localized`): literal Character `LangID` cells
set to the target's Windows LCID, and for a right-to-left target Paragraph `Flags=1` in every paragraph row the text
uses (a sparse row, or a whole Paragraph section, is added where the shape inherits). Nothing is stamped when nothing
is translated.

Encodings: UTF-8 (BOM kept), UTF-16 with a BOM, and a legacy charset the XML declaration names are read and written
back in the same encoding. Anything else is refused, as the XML specification and LocOS do.

Safety: the shared zip guard, a central-directory scan that refuses a member listed twice (`zipdir.rs`, until
`bilingua-zip` checks this itself), the XML guard (entity declarations, depth, element count) over every XML part, a
DOCTYPE in a part the filter reads is refused (LocOS), a strict well-formedness check (lxml's: undefined entities,
duplicate attributes, `<` in attributes, illegal characters), a markup budget of 40 `<` per allowed unit over the page
parts, and the model's unit and length limits.

## Known differences from LocOS

- Changed parts keep their original markup instead of lxml's re-serialisation.
- A comment or processing instruction inside a changed `<Text>`: kept outside the unit, dropped inside it (LocOS drops
  both).
- A CDATA section at the edge of a `<Text>` is not trimmed of white space.
- "Has a letter or digit" is Rust's `char::is_alphanumeric` (a little wider than Python's `str.isalnum`).
- Okapi (`okf_openxml`) also extracts master text, keeps leading markers inside the unit and writes `<fld>` as a
  paired code; it has no page names, labels or comments.

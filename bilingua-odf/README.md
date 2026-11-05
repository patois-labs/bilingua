# bilingua-odf

OpenDocument for Bilingua: **Writer** (`.odt`, `.ott`), **Calc** (`.ods`, `.ots`), **Impress** (`.odp`, `.otp`) and
**Draw** (`.odg`, `.otg`). One walker serves all four, because every family keeps its text in `text:p` and `text:h`.
Behaviour follows Localization OS (`odf_filter.py`, `office_comments.py`); `WIRING.md` has the facade lines.

```rust
use bilingua_model::*;
let f = bilingua_odf::Odt::default();            // Ods, Odp, Odg alike; Odt::with_options(OdfOptions { .. })
let mut doc = f.extract(&bytes, &ExtractOptions::new().with_target_lang("de-DE"))?;
doc.set_target("p0", vec![Inline::text("Erste Schritte")])?;
let out = f.merge(&doc)?;
```

## What is extracted

| Where | Units |
|---|---|
| content.xml | every `text:p` / `text:h` that is not skipped; unit id `p<N>` where N is the LocOS `p_index` (it counts paragraphs that are not units: blanks, numbers) |
| styles.xml | only the paragraphs in master-page headers and footers (`style:header`, `footer`, `-left`, `-first`) |
| Calc | the paragraphs of **string cells only**; a cell with `table:formula` or a non-string `office:value-type` (float, date, percentage, currency, time, boolean) is a rendering of a value, never a unit; a repeated cell is one unit |
| Impress / Draw | text boxes and shapes on every page, and speaker notes; master-page placeholders are not units |
| comments | with `OdfOptions { translate_comments: true }`: each paragraph of each `office:annotation`, ids `c<J>`, `p_index` `1000000 + J`, kind `comment`. Off: the comment rides in its host paragraph as a point code |
| skipped | tracked-change regions (deleted text), index templates and index sources, paragraphs of non-string cells, annotations (unless asked) |

meta.xml (`dc:title`, subject, keywords) is **not** extracted: LocOS does not extract it (Okapi does; see below).

Inline codes: `text:span`, `text:a` and `text:meta` are paired tags (`Open` / `Close`, tokens `g`); every other child
(`text:tab`, `text:line-break`, bookmarks, fields, note anchors, frames, change marks, comments, XML comments and
processing instructions) is one `Standalone` tag (token `x`, order kept) and is kept whole. A paragraph nested in a
point element (footnote body, text box in a frame) is its own unit. `{0}` and `%1$s` stay plain text, as in LocOS, so no
tag is `free`.

White space is ODF 1.3 Part 3, 6.1.2 / 6.1.3: tab, CR and LF are spaces, runs collapse to one, leading spaces drop, and
`text:s` (`text:c` repeats, capped at 1000) carries the rest. The unit text has plain spaces; the merge writes them back
(`text:s` for extra spaces, `text:tab` / `text:line-break` for a tab or newline the translation adds).

## Merge

- No target on any unit: the **original bytes**. Otherwise only the XML parts that change are rewritten, and inside a
  part only the changed paragraphs: everything else is copied from the source text. Other zip members keep their
  bytes, order, time stamps and compressed bytes; `mimetype` is written first, stored, with no extra field.
- Codes the translation lost or invented are tolerated: an unknown or repeated code is dropped, a point element
  (footnote, frame, bookmark) the translation left out is kept at the end of the paragraph.
- A part is written in its own encoding (UTF-8 with or without BOM, UTF-16 with BOM, or the declared legacy charset
  while the text fits, else UTF-8 under a UTF-8 declaration). A part that is not UTF-8 and declares no encoding is
  refused.
- **Target language** (when at least one unit is translated and `Document::target_lang` is a usable tag): `fo:language`
  / `fo:country` / `fo:script` in the default paragraph and graphic styles and in every style that already declares a
  language (East Asian targets in `style:language-asian` and friends, right-to-left and complex-script targets in
  `style:language-complex`), `dc:language` in meta.xml. "zxx" / "none" languages (code text) are left alone. A
  right-to-left target also sets `style:writing-mode="rl-tb"` on the default paragraph and graphic styles, turns every
  explicit `lr-tb` / `lr` of paragraph, graphic and presentation styles right to left (modern ODP pins it on every shape,
  #724), and in a spreadsheet turns every sheet right to left (a new `taRtlN` table style where a sheet has none). Left
  to right targets leave styles byte for byte.

## Safety

The zip is read with the B6 guards (`bilingua-zip`: no `..`, absolute or drive paths, no encrypted members or links,
entry, size and ratio caps, forged sizes); a duplicate member name (which the zip reader would silently collapse) is
refused here; every XML part of the package goes through `check_xml_parts` (no entity declarations anywhere); the parts
this crate reads are refused with any DOCTYPE. The XML tree is built iteratively (no recursion on input) with the depth,
element and markup-budget limits, and character, entity, name, attribute and namespace checks like libxml2. Password
protected packages (`manifest:encryption-data`) are refused.

## Tests

| File | What |
|---|---|
| `tests/locos_odf.rs` | the 12 cases of `verify_odf_filter.py`, one test each (12 of 12) |
| `tests/oracle.rs` | unit texts of the oracle corpus and Okapi's 15 test packages against LocOS (`tools/gen_oracle_units.py`), comments on and off, untouched merge, and 12 target languages against the language stamps LocOS writes (`tools/gen_golden.py`) |
| `tests/behaviour.rs` | 11 languages, escaping, point codes, comments, right-to-left sheets, byte-for-byte parts, encodings (BOM, CRLF, Latin-1, UTF-16), segments, JSON skeleton, sniffing |
| `tests/hostile.rs` | hostile zips and XML, XXE and entity bombs, deep and huge input, truncation, proptest (random bytes, mutated XML, arbitrary target text) |
| `tests/speed.rs` | a 5.6 MB `content.xml`, timed (`--release -- --nocapture`); `tools/time_locos.py` times LocOS on the same file |
| `tests/libreoffice.rs` | merged files open in LibreOffice (PDF export) when `soffice` is installed |
| `tools/compare_oracles.py` | Bilingua against LocOS and Okapi `okf_openoffice` on a folder of files |

## Against Okapi (`okf_openoffice`, Okapi 1.48.0, its own 15 test packages)

Bilingua equals LocOS on all 15 files. Against Okapi, 81.7 percent of Bilingua's units are found unchanged in Okapi's
output (230 units; Okapi lists 325). The differences are all Okapi extracting more or coding differently, and LocOS wins:

- Okapi extracts meta.xml (title, description, keywords, user-defined values), sheet names, cell comments, numeric
  and formula cells' text, and text inside a text field (a date's rendered text). LocOS and Bilingua do not.
- Okapi puts the content of a nested element (a script, a footnote body) inside the host unit; Bilingua keeps the
  point code whole and the nested paragraph is its own unit.
- Okapi turns `text:tab` into a space inside a unit ("Heading 1 1"); LocOS and Bilingua keep a point code.
- The `okf_odf` filter (bare XML parts, not packages) is not comparable: it does not open a zip.

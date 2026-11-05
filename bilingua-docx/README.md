# bilingua-docx

The Word parser for Bilingua. It opens a `.docx` (also `.docm`, `.dotx`, `.dotm`), gives you one unit per
paragraph with formatting kept as tags, and writes the translation back into a valid Word file.

```rust
use bilingua_docx::Docx;
use bilingua_model::*;

let f = Docx;
let mut doc = f.extract(&bytes, &ExtractOptions::new().with_target_lang("es-ES"))?;
doc.set_target("p3", vec![Inline::text("Hola.")])?;
let (out, report) = bilingua_docx::merge_with_report(&doc)?;   // or f.merge(&doc) for just the bytes
let html = f.preview(&doc);                                    // approximate HTML, one span per segment
```

For the facade (`bilingua/src/lib.rs`, `filters()`): `Box::new(bilingua_docx::Docx)`.

## How it works

- The file is read in memory with the same zip safety limits `catcore` uses for packages (no `..`, absolute
  paths, drive letters or control characters in names, no encrypted entries or links, caps on entry count,
  size and expansion ratio). Nothing is written to disk.
- Every XML part is parsed into a lossless tree, so an unchanged document comes back with every XML part
  **byte for byte identical** (the zip container itself may differ; entry order, compression method and time
  stamps are kept).
- A paragraph that is not changed is written from its stored original XML. A paragraph that is changed is
  rebuilt from the target.
- Unit ids: `p<N>` for the main document (N = paragraph number, counting every paragraph, so ids have gaps),
  `<part>-p<N>` for other parts (`header1-p2`, `footnotes-p3`), `meta-title` and `meta-subject` for properties.
- One segment per unit. Sentence splitting happens later in `bilingua-seg`; the writer joins segments with the
  unit separators.

## Supported

| Feature | What happens |
|---|---|
| Paragraphs, headings, lists, table cells | One unit each. `context.kind` is `heading`, `list-item`, `cell` or `paragraph`; `context.style` is the style id; `context.heading` is the nearest heading above. Headings come from the style name (Heading N, Title) or an outline level. Lists come from `w:numPr` on the paragraph or its style |
| Headers, footers, footnotes, endnotes | Own units, kind `header`, `footer`, `footnote`, `endnote` |
| Comments | Own units (kind `comment`) and also a note on the paragraph they anchor to ("Comment (Author): text") |
| Text boxes (`w:txbxContent`, VML and DrawingML) | Own units, kind `textbox`; the drawing is a standalone tag in the paragraph that holds it |
| Text box inside `mc:AlternateContent` | The `Choice` text is the unit. The `Fallback` copy is written from the same translation when it has the same paragraphs and text. If the two copies do not line up, the fallback is left in the source language |
| Document properties | `dc:title` and `dc:subject` in `docProps/core.xml` |
| Split runs | Neighbouring runs with the same formatting are merged. Spell-check marks, `w:lang`, `w:noProof` and rsid attributes do not count as formatting. They are dropped from a rebuilt paragraph |
| Formatting | The most common formatting of a paragraph needs no tag. Other runs become paired tags: `<b>`, `<i>`, `<u>`, `<sup>`, `<sub>`, or `<fmt>` for anything else (label says what, e.g. "italic color"). The full `w:rPr` is kept in the tag's code |
| Hyperlinks, smart tags, content controls (`w:sdt`) | Paired tags (`<a>`, `<x>`); the relationship id and all attributes are kept. Formatting inside a link is relative to the link's own most common formatting |
| Tabs, line and page breaks, symbols, images, equations, footnote references | Standalone tags |
| Fields | Simple and complex fields are one locked standalone tag, shown like `{PAGE 3}`. The instruction is never translated. A field that spans paragraphs (a table of contents) locks all of those paragraphs |
| Bookmarks, comment ranges, permission marks | Not shown as tags. Marks at the start of a paragraph stay at the start; all others move to the end of the paragraph when it is rebuilt |
| Tracked changes | Text is extracted as if all changes were accepted, with the note "This paragraph has tracked changes". Unchanged paragraphs keep their tracked markup. A changed paragraph is written as plain accepted text and `merge_with_report` returns a warning for it |
| Target language | `w:lang` of rebuilt runs is set to the target language when the run had one (`w:val` always; `w:eastAsia` for ja/zh/ko; `w:bidi` for ar/he/fa/ur) |
| Writing | Escaping, `xml:space="preserve"` where needed, `\t` and `\n` in a target become `w:tab` and `w:br`, characters XML cannot hold are dropped |
| Preview | `preview()` gives approximate HTML (headings, paragraphs, lists, tables, bold/italic/underline, image placeholders, `dir="rtl"` for bidi paragraphs) with every segment in `<span data-bl-seg="ID">`. Other parts are listed after the body. No scripts. Not pixel perfect |
| Other XML prefixes, default namespace, strict namespace | Detected from the root element |

## Not supported (and what happens instead)

| Not supported | What happens |
|---|---|
| SmartArt, charts, embedded objects, text inside DrawingML shapes that are not text boxes | Left untouched, not extracted |
| Right-to-left setup for a target language | Translating into Arabic, Hebrew etc. does not add `w:bidi` or `w:rtl`. Fix the paragraph direction in Word, or start from a template that has it |
| `w:ins`/`w:del` inside a changed paragraph | Accepted (see above); the reviewer sees plain text |
| A field that starts inside a hyperlink or content control and ends outside it | Both parts stay valid XML but the order of the pieces in a rebuilt paragraph may differ from the original |
| Documents that contain the characters U+E000 to U+E003 in the main part | Refused with an "unsupported" error (they are used as internal markers) |
| Side parts that are not well-formed XML or not UTF-8 | Left untouched (the main document must parse) |
| DOCTYPE or entity declarations in any part | The main document is refused; side parts are left untouched |
| Macros, digital signatures | Kept byte for byte, but a signature does not stay valid after any change |
| Style-based translation of style names, table of contents entries, index entries, glossary documents | Not extracted |
| Per-run language detection (mixed-language paragraphs) | The target language is set on every run that had a `w:lang` |
| Comments as part of the same unit | They are separate units; edit the comment unit to translate the comment |

## Zip details

The skeleton is `Skeleton::Zip`: untouched entries are `ZipData::Raw`, rewritten XML parts are `ZipData::Parts`.
`bilingua-model`'s `ZipEntry` has no field for the compression method or the time stamp, so the filter adds one
extra entry named `"\0bilingua-docx-meta"` (a NUL in the name, impossible in a real file) as the last skeleton
entry. It holds those details and the namespace prefix, and it is never written to the output. Do not drop it
when you store or copy a `Document`.

Skeleton `Raw` strings and unit codes may contain internal markers (U+E000 to U+E003). Only this crate's
`merge` resolves them; do not edit the skeleton or `Unit::codes` whose names start with `_`.

## Tests

`cargo test -p bilingua-docx`. Fixtures are built in the tests from made-up XML (no binary files). The
LibreOffice test converts a merged document to PDF when `soffice` is installed in a standard location and is
skipped otherwise.

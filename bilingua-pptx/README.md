# bilingua-pptx

The PowerPoint parser for Bilingua. It opens a `.pptx` (also `.pptm`, `.potx`, `.potm`, `.ppsx`, `.ppsm`), gives
you one unit per paragraph with formatting kept as tags, and writes the translation back into a valid file.

```rust
use bilingua_pptx::Pptx;
use bilingua_model::*;

let f = Pptx;
let mut doc = f.extract(&bytes, &ExtractOptions { target_lang: Some("es-ES".into()), ..Default::default() })?;
doc.set_target("slide1-p1", vec![Inline::text("Hola.")])?;
let (out, report) = bilingua_pptx::merge_with_report(&doc)?;   // or f.merge(&doc) for just the bytes
let html = f.preview(&doc);                                    // approximate HTML, one span per segment
```

For the facade (`bilingua/src/lib.rs`, `filters()`): `Box::new(bilingua_pptx::Pptx)`.

## How it works

- The file is read in memory with the same zip safety limits as `bilingua-docx`. Nothing is written to disk.
- Every XML part that holds text is parsed into a lossless tree. An unchanged presentation comes back with every
  XML part **byte for byte identical** (the zip container itself may differ; entry order, compression method and
  time stamps are kept). A paragraph that is not changed is written from its stored original XML; a changed
  paragraph is rebuilt from the target.
- Unit ids: `<part>-p<N>`, where the part is the file name without `.xml` (`slide3-p5`, `notesSlide1-p1`,
  `data1-p2`) and N counts every `a:p` in the part, so ids have gaps.
- Slides are numbered the way the deck orders them (`sldIdLst`), not by file name. Units come out in slide order:
  slide text, then SmartArt, then notes; layouts and masters last.
- One segment per unit. Sentence splitting happens later in `bilingua-seg`.

## Context

| Field | Value |
|---|---|
| `kind` | `title`, `subtitle`, `body`, `textbox`, `cell`, `notes` |
| `style` | The placeholder type (`title`, `ctrTitle`, `body`, `ftr`, ...) when the shape is a placeholder |
| `location` | `slide 3, shape "Title 1"`; cells add `, row 2, column 1`; notes `slide 3, notes, shape "..."`; layouts `layout 1 "Title Slide", shape "..."` |
| `heading` | Text of the slide's title, for the other units of that slide |
| `extra` | `part` (`slide`, `notes`, `diagram`, `layout`, `master`), `slide` (number), and for cells `table`, `row`, `col` |

## Supported

| Feature | What happens |
|---|---|
| Paragraphs in slides | One unit each, including grouped shapes and text boxes. Paragraphs with no letters (empty, only a slide number field, "2026") are not units |
| Tables (`a:tbl`) | Each cell paragraph is a unit with kind `cell`. The covered cells of a merged range (`hMerge`, `vMerge`) are not shown by PowerPoint and are skipped; paragraph numbers still count them |
| Notes slides | Own units, kind `notes` |
| Layouts and masters | Only shapes that are not placeholders (a brand line, a confidentiality note). Placeholder prompt text ("Click to edit Master title style") is left out |
| SmartArt | Text in `ppt/diagrams/data*.xml` is extracted. The cached drawing (`drawing*.xml`) that PowerPoint shows is updated to match: a drawing paragraph is matched to the data paragraph with the same text and rewritten as one run with the first run's properties |
| Runs | The most common formatting of a paragraph needs no tag. Other runs become paired tags: `<b>`, `<i>`, `<u>`, `<sup>`, `<sub>`, `<link>`, or `<fmt>`. The raw `a:rPr` is kept in the tag's code. Language, spell-check and dirty marks do not count as formatting |
| Line breaks (`a:br`) | Standalone tag `<br/>`. A `\n` in a target becomes a new `a:br` |
| Fields (`a:fld`: slide number, date) | One locked standalone tag shown like `{slidenum}`. Never translated |
| Other elements inside a paragraph (math, custom XML) | Standalone tags, kept as they are |
| `mc:AlternateContent` | The `Choice` content is read; the `Fallback` copy is not (see below) |
| Target language | `lang` of rebuilt runs is set to the target language when the run had a `lang` attribute. Runs without one stay without one |
| Right-to-left | For an Arabic, Hebrew, Persian or Urdu target, a rebuilt paragraph gets `rtl="1"` on its `a:pPr` (the element is added when missing). A paragraph that already has an `rtl` attribute keeps it. Unchanged paragraphs are never touched |
| Preview | `preview()` gives simple HTML: one `<section>` per slide (title as `h2`, bullets, tables), notes after the slide, layouts and masters at the end, `dir="rtl"` for right-to-left targets. Every segment is in `<span data-bl-seg="ID">`. `render_segment()` gives the inner HTML of one span. No scripts. Not pixel perfect |

## Not supported (and what happens instead)

| Not supported | What happens |
|---|---|
| Chart text (titles, axis labels, series names) | Left out on purpose; charts are not touched |
| Alt text, slide names, comments, handout and notes masters, document properties | Not extracted |
| Fallback copy of `mc:AlternateContent` | Not read. If it holds text it stays in the source language (PowerPoint shows the Choice copy) |
| SmartArt drawing paragraphs whose text differs from every data paragraph | Left in the source language |
| SmartArt drawing: several runs with different formatting | A translated drawing paragraph uses the first run's formatting for all of the text |
| Embedded OLE objects, images with text, video captions | Not extracted |
| Spell-check marks (`err`, `dirty`) | Dropped from rebuilt runs. `a:endParaRPr` is kept |
| Presentations that contain U+E000 to U+E004 in a slide | Refused with an "unsupported" error (used as internal markers); in other parts that part is left alone |
| DOCTYPE or entity declarations | A slide that has one is refused; side parts are left untouched |
| Side parts that are not well-formed XML or not UTF-8 | Left untouched |
| Digital signatures, macros | Macros are kept byte for byte; a signature does not stay valid after any change |

## Zip details

The skeleton is `Skeleton::Zip`: untouched entries are `ZipData::Raw`, rewritten XML parts are `ZipData::Parts`.
`bilingua-model`'s `ZipEntry` has no field for the compression method or the time stamp, so the filter adds one
extra entry named `"\0bilingua-pptx-meta"` (a NUL in the name, impossible in a real file) as the last skeleton
entry. It is never written to the output. Do not drop it when you store or copy a `Document`.

Skeleton `Raw` strings may contain internal markers (U+E000 to U+E004). Only this crate's `merge` resolves them.
Unit codes whose names start with `_` belong to the writer; do not edit them.

## Tests

`cargo test -p bilingua-pptx`. The fixture deck is built in code from made-up XML (no binary files). The
LibreOffice test converts a merged deck to PDF when `soffice` is installed in a standard location and is
skipped otherwise.

# Bilingua

File parsers for translation, in Rust. Bilingua opens a source file, gives you its translatable text as units and
sentences with the formatting kept as inline tags, and writes the translated file back in its original format.
It has no dependency on any editor or service, so it can run in a desktop app, on a server, from the command line or
(later) in the browser.

Status: early (0.1). Private until the name is cleared and the API settles. Licensed under Apache 2.0.

## Formats

| Crate | Formats |
|---|---|
| `bilingua-text` | Plain text |
| `bilingua-json` | JSON (string values; URLs left out) |
| `bilingua-markdown` | Markdown (code spans kept, markup as tags) |
| `bilingua-html` | HTML (attributes such as `alt`, `title`, `placeholder`, `aria-label`) |
| `bilingua-po` | Gettext PO |
| `bilingua-subtitle` | SRT and WebVTT |
| `bilingua-docx` | Word DOCX |
| `bilingua-xlsx` | Excel XLSX (shared strings, inline strings, headers and footers) |
| `bilingua-pptx` | PowerPoint PPTX (slides and notes; not layout or master prompt text) |
| `bilingua-xliff` | XLIFF 1.2 and 2, SDLXLIFF, MQXLIFF, MXLIFF (with GlobalLink status); byte-faithful writing |
| `bilingua-sdl` | Trados project and return packages (SDLPPX, SDLRPX), memoQ packages (MQXLZ) |

Shared crates: `bilingua-model` (the document model every parser fills), `bilingua-seg` (SRX 2.0 sentence
segmentation, tag-aware), `bilingua` (one API over all of them), `bilingua-cli` (the `bilingua` command).

## Use

```rust
use bilingua::*;

let opts = ExtractOptions { source_lang: Some("en".into()), target_lang: Some("es-ES".into()),
                            granularity: Granularity::Sentence, ..Default::default() };
let mut doc = extract("page.md", &bytes, &opts)?;
doc.set_target("u3#1", vec![Inline::text("Primera frase.")])?;
let out = merge(&doc)?; // the original format, with the translations in place
```

From the command line: `bilingua extract page.docx --target es-ES -o page.xlf`, then
`bilingua merge page.xlf -o page.es.docx` (see `bilingua-cli/README.md`).

## Guarantees and tests
- Merging with nothing translated gives back the input byte for byte.
- Every format round-trips a full pseudo-translation (merge, reopen, compare).
- Test text covers English, Spanish, French, Italian, German, Dutch, Portuguese, Japanese, Chinese, Korean and
  Arabic. `cargo test` runs about 250 tests on Windows and Linux.
- `oracle/` compares Bilingua's output with another open-source filter set on a made-up corpus; the findings are in
  `oracle/REPORT.md`. The comparison is a test aid only, never a dependency.

## Not done yet
- WebAssembly build for the browser.
- More formats (IDML, DITA, and others as users ask).
- A published crate on crates.io.

## Where it is developed
Bilingua is developed inside the CAT OS repository (`bilingua/`), which uses it, and mirrored here with its history.

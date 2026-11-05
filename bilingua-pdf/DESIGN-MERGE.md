# PDF merge: design note (needs Brent's decision)

## What Localization OS does

`app/filters/pdf_filter.py`:

- **Extract** (`parse`): pypdfium2 (PDFium) opens the file; for each page `FPDFText_GetText` gives the page text.
  A page whose stripped text is shorter than 20 characters is a scanned page: it is rendered at 150 dpi (capped by
  `PDF_OCR_MAX_PIXEL_DIM`) and sent to a vision engine (`services/ocr.py`). Either text is split into paragraphs on
  blank lines, the lines of a paragraph joined with one space; one unit per paragraph, no inline codes. In practice
  PDFium never returns blank lines, so a text page is one unit.
- **Refuses**: a file PDFium does not open (not a PDF, truncated before its trailer, no catalog: "Data format
  error"), a file that needs a password ("Incorrect password error"), more than `PDF_MAX_PAGES` (1000) pages, more
  page objects than 80 x `MAX_DOCUMENT_SEGMENTS`, scanned pages over `PDF_OCR_MAX_TOTAL_PIXELS`, more units than
  `MAX_DOCUMENT_SEGMENTS`. A file with only an owner password opens.
- **Merge** (`build`, `build_localized`): **re-typesets**. It ignores the original file entirely and writes a new A4
  PDF with typst (typst-py, Apache-2.0): one paragraph per unit, 11 pt, system fonts with fallback (Noto in the
  container), the target language declared (`#set text(lang:, region:)`, which also sets RTL). There is no overlay
  and no bilingual output. `passthrough_untouched = False`: even an untouched export is a new typst PDF.

## What Bilingua does now

- Extract: the same units. The PDFium text extraction is re-implemented in Rust (no dependency beyond `flate2`),
  matching PDFium's page text exactly on 80 of 80 fixture pages and 2669 of 2671 pages of 146 real PDFs. Scanned
  pages get no units and a `pdf_ocr_page` warning; OCR stays in LocOS (the adapter OCRs those pages and splices the
  units in page order).
- Merge: an untouched document gives the original bytes (the Bilingua contract); a translated document, or
  `stamp_target_always`, is `Error::Unsupported`. The adapter keeps calling LocOS's typst `build` with the targets.

## Why the merge is not in the crate

Matching LocOS means a typesetter: typst itself (`typst`, `typst-layout`, `typst-pdf`, `typst-library` and about 150
more crates; tens of MB of build, several MB of code in the binary), plus fonts. LocOS gets its fonts from the host
(`fonts-noto-core`, `fonts-noto-cjk`, about 100 MB); in WASM there are no system fonts, so they would have to be
bundled or passed in by the caller. A small hand-written writer (Helvetica, WinAnsi) would only cover Latin text and
would not match LocOS's output, so it buys nothing.

## The options

1. **Keep the merge in LocOS (recommended for 1.0).** Bilingua stays extract-only for PDF, the adapter calls the
   existing typst build. No new dependency, nothing changes for users, WASM unaffected. Cost: Bilingua alone
   (CAT OS web, the CLI) cannot write a translated PDF; it can export the units as XLIFF or DOCX instead.
2. **typst behind a cargo feature `typeset`, off by default and off for WASM.** `merge` would build the same typst
   source LocOS builds (the string-literal paragraphs, the language line) and compile it with fonts from the system
   (desktop, server) or given by the caller. Matches LocOS on the desktop and server. Cost: a large optional
   dependency tree to audit and keep building, and the typst version must follow typst-py's to keep the output
   identical; on WASM the feature stays off (or a font bundle is needed).
3. **A real in-place writer (replace the text in the original page content).** What users would like most
   (layout kept), but it is a different product from LocOS's and a large piece of work (font subsetting and
   re-embedding, re-flowing text that does not fit, RTL and complex scripts). Not a 1.0 item.

Decision needed: option 1 for 1.0 (and 2 later if CAT OS desktop needs PDF out), or 2 now.

Note for the lead: `oracle/tests/corpus.rs` pseudo-translates every corpus file and merges it; with option 1 a PDF
cannot pass that, so the PDF corpus needs an exception there (like `NO_TEXT_FORMATS`), which this branch may not
edit. The corpus files are in `oracle/corpus/pdf/` in the worktree, not committed.

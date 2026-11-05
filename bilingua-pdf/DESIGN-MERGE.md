# PDF merge: design note

## Decision (Brent, 11-05)

**Option 1: the merge stays in Localization OS for 1.0.** Bilingua's PDF filter extracts the text layer and merges
untouched documents only: an untouched merge returns the original bytes; a translated merge (or one with
`stamp_target_always`) is `Error::Unsupported`. The LocOS adapter keeps calling LocOS's typst `build` with the unit
targets. No typesetter, no `typeset` feature and no typst dependency in this crate. The oracle corpus test lists
`pdf` in `MERGE_UNSUPPORTED_FORMATS` (`oracle/tests/corpus.rs`) for this reason.

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

## What Bilingua does

- Extract: the same units. The PDFium text extraction is re-implemented in Rust (no dependency beyond `flate2`),
  matching PDFium's page text exactly on 80 of 80 fixture pages and 2669 of 2671 pages of 146 real PDFs. Scanned
  pages get no units and a `pdf_ocr_page` warning; OCR stays in LocOS (the adapter OCRs those pages and splices the
  units in page order).
- Merge: an untouched document gives the original bytes (the Bilingua contract); a translated document, or
  `stamp_target_always`, is `Error::Unsupported`.

## Why the merge is not in the crate

Matching LocOS means a typesetter: typst itself (`typst`, `typst-layout`, `typst-pdf`, `typst-library` and about 150
more crates; tens of MB of build, several MB of code in the binary), plus fonts. LocOS gets its fonts from the host
(`fonts-noto-core`, `fonts-noto-cjk`, about 100 MB); in WASM there are no system fonts, so they would have to be
bundled or passed in by the caller. A small hand-written writer (Helvetica, WinAnsi) would only cover Latin text and
would not match LocOS's output, so it buys nothing. Cost of the decision: Bilingua alone (CAT OS web, the CLI) cannot
write a translated PDF; it can export the units as XLIFF or DOCX instead.

## Options that were considered (not taken for 1.0)

2. typst behind a cargo feature `typeset`, off by default and off for WASM, compiling the same typst source LocOS
   builds with fonts from the system or the caller. A large optional dependency tree to audit, and typst must follow
   typst-py's version to keep the output identical.
3. A real in-place writer (replace the text in the original page content): what users would like most (layout
   kept), but a different product from LocOS's and a large piece of work (font subsetting and re-embedding,
   re-flowing text that does not fit, RTL and complex scripts).

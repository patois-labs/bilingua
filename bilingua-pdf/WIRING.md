# Wiring for the lead

- Workspace: `"bilingua-pdf"` in `members`, `bilingua-pdf = { path = "bilingua-pdf" }` in `[workspace.dependencies]`
  (done in the last commit, replace at merge). Dependencies: `bilingua-model`, `flate2` 1.1 with
  `default-features = false, features = ["zlib-rs"]` (the same build `zip` already pulls in; no new crate in
  `Cargo.lock` beyond `bilingua-pdf`). Dev: `proptest`, `serde_json`.
- Facade (`bilingua/Cargo.toml` and `filters()`): `Box::new(bilingua_pdf::Pdf)`. Format id `pdf`, extension `pdf`.
  `sniff` is strict (`%PDF-` within the first 1024 bytes, PDFium's header search), so its place in `filters()` does
  not matter; it is next to MO.
- `bilingua/tests/contract.rs`: `"pdf" => "bilingua-pdf"` in `crate_of`.
- `tools/check-portable.sh` and `.ps1`: `pdf` in `CRATES`.
- Skeleton: `Skeleton::Binary` with the original file, parser `bilingua-pdf`, schema 1.
- Options (`format_option_keys`): `pdf.max_pages`, `pdf.max_stream_bytes`, `pdf.ocr_min_chars`,
  `pdf.ocr_max_pixel_dim`, `pdf.ocr_max_total_pixels`, all integers.
- Warnings: `pdf_ocr_page`, part `page N` (1-based), for every scanned page; the adapter OCRs those pages
  (`bilingua_pdf::page_texts` gives page sizes for LocOS's render scale) and inserts their units at that page.
- Merge: untouched = original bytes; translated or `stamp_target_always` = `Error::Unsupported` (the adapter keeps
  LocOS's typst `build`). See `DESIGN-MERGE.md` for the decision this needs.
- Corpus: `oracle/corpus/pdf/` is ready in the worktree but **not committed**: `oracle/tests/corpus.rs` merges a
  pseudo-translated copy of every corpus file, which a PDF cannot pass while merge is extract-only. Add the corpus
  together with an exception for `pdf` there (like `NO_TEXT_FORMATS`), or after the merge decision. `oracle/run.py`
  needs `pdf` in `FORMATS` and `scoreboard.py` the LocOS name `pdf`.

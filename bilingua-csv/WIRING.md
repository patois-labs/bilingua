# Wiring bilingua-csv (the lead replaces the throwaway wire commit at merge)

- Format ids and types: two filters in one crate.
  - `csv`, extension `csv`: `bilingua_csv::CsvFilter` (`Default`, `Filter` impl).
  - `tsv`, extensions `tsv` and `tab`: `bilingua_csv::TsvFilter` (`Default`, `Filter` impl).
- Facade `filters()`: add
  `Box::new(bilingua_csv::CsvFilter::default())` and `Box::new(bilingua_csv::TsvFilter::default())`
  **after `bilingua_properties::Properties` / `Xaml` / `Ftl` and just before `bilingua_html::Html`** (the two loose
  sniffers, HTML and text, stay last).
- Extension conflicts: none. `bilingua-text` claims only `txt` and `text`; no other filter claims `csv`, `tsv` or `tab`.
  A `.txt` file that is really tab-separated stays plain text (as in LocOS); LocOS also sends `.tab` to TSV.
- Sniffing (a file with no known extension): strict. At least three non-blank lines in the first 8 KB, one delimiter
  (`,` `;` TAB `|`) giving the same width of two or more columns on at least 90% of them, no NUL, not starting with
  `<` or `{`. `TsvFilter::sniff` only accepts TAB. Prose, JSON, YAML, INI and key=value files do not match.
- Facade `Cargo.toml`: `bilingua-csv = { workspace = true }`. Workspace: member `bilingua-csv` and
  `bilingua-csv = { path = "bilingua-csv" }`. Dev-dependency `proptest = "1"` is crate-local.
- `bilingua/tests/contract.rs` `crate_of`: `"csv" | "tsv" => "bilingua-csv"`.
- `bilingua/tools/check-portable.sh` (and `.ps1` if present): add `csv` to `CRATES`.
- Options the facade cannot express (the column mapping and the project's target locales):
  `CsvFilter::with_options(CsvOptions { columns: Some("source=English, target.de=German, key=ID".into()),
  target_locales: vec!["de-DE".into(), "fr-FR".into()] })`, same for `TsvFilter`. The facade, `bilingua-py` and the
  CLI need a `columns` argument (LocOS: the `columns` upload field, `--columns`, the API `columns`) that builds the
  filter this way; without it the header row is read as LocOS does.
- `ExtractOptions`: `source_lang` (header detection, legacy code page), `target_lang` (adds it to the target list and
  selects the column the document writes into; with a multilingual sheet extract once per target, as LocOS builds
  once per target), `include_locked` (adds non-blank source cells that are formulas or have no letter as locked
  units `k1`, `k2`, ... that never move the ids of translatable ones), `limits`.
- Unit shape: one unit per source cell (`u1`, `u2`, ...), text exactly as the cell reads, no inline tags (LocOS
  makes none). `context.key` = the key column, `context.notes` = `["Key: ...", "<header>: ...", "Max length: ..."]`
  (LocOS joins them with ` | `), `context.max_len`, `context.location` = `row N`, `context.extra["column"]` = the
  source column's header name. Each unit's `$csv` hidden code carries the record; nothing else needs the facade.
- Merge: nothing translated returns the original bytes. A unit with a target is written into the target column (or
  over the source cell when the file has no target column). A unit whose target equals its source leaves an
  already-filled target cell alone (LocOS rule). There is no language stamp in CSV, so `target_lang` at merge changes
  nothing; the column is chosen at extract.
- Encodings: UTF-8 (BOM kept), UTF-16 with BOM, legacy code pages via the model (source language picks the page). A
  legacy file goes back in its page when everything fits, else UTF-8 **with a BOM** (LocOS; the model's own
  `encode_text` would drop the BOM). Written line breaks stay `\n`; the model's newline restoring is switched off
  because LocOS leaves them alone. The "always write UTF-8" merge option the model is getting is not used here yet.
- Oracle corpus: `oracle/corpus/csv/` (33 files, made by `tests/tools/gen_corpus.py`).
- Python wheel: nothing format-specific except the `columns` argument above.

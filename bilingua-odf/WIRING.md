# Wiring for the lead

Four filters in one crate: `Odt` (id `odt`, extensions `odt` `ott`), `Ods` (`ods` `ots`), `Odp` (`odp` `otp`) and `Odg`
(`odg` `otg`). `bilingua_odf::filters()` returns all four.

- Workspace: `"bilingua-odf"` in `members`, `bilingua-odf = { path = "bilingua-odf" }` in `[workspace.dependencies]`
  (done in the wire commit, replace at merge). Normal dependencies: `bilingua-model`, `bilingua-zip`. Dev-dependencies:
  `proptest = "1"`, `serde_json`.
- Facade `Cargo.toml`: `bilingua-odf = { workspace = true }`.
- Facade `filters()`: `all.extend(bilingua_odf::filters());` next to the other packages, before the loose sniffers.
  **Sniff is strict**: the bytes must start with `PK\x03\x04` and carry the `mimetype` member
  `application/vnd.oasis.opendocument.<kind>` within the first 4 KB, and each filter only claims its own family
  (`text`, `text-template`, `text-master`, `text-web` for Odt). `detect` by extension finds them first anyway.
- `bilingua/tests/contract.rs`: `"odt" | "ods" | "odp" | "odg" => "bilingua-odf"` in `crate_of`. The paragraph-mode test
  merges an untouched file and expects the original bytes for non-zip names; for ODF an untouched merge is also
  exact, so `is_zip` need not list them (add `.odt` `.ods` `.odp` `.odg` if you want the weaker check).
- `tools/check-portable.sh` and `.ps1`: `odf` in `CRATES` (done in the wire commit).
- Skeleton: `Skeleton::Zip` with one raw entry named `"\0bilingua-odf-archive"` holding the whole original package
  (like `xcloc`); the merge reads it again, so the skeleton is as large as the file. Parser stamp `bilingua-odf`,
  schema 1.
- Unit ids: `p<N>` with N = the LocOS `p_index` (also in `context.extra["p_index"]`), `c<J>` for comments
  (`p_index` = 1000000 + J). `context.kind` is `heading`, `paragraph`, `cell` or `comment`; `context.style` the
  `text:style-name`; `context.extra["part"]` the XML part.
- Options: `ExtractOptions` has no room for the LocOS "translate comments" option, so it lives in `OdfOptions`
  (`Odt::with_options(OdfOptions { translate_comments: true })`). The adapter builds the filter with it. `include_locked`
  is not used (the format marks no text as not translatable).
- Inline codes: paired `Open` / `Close` for `text:span`, `text:a`, `text:meta` (LocOS `g`); every other child element is a
  `Standalone` (LocOS `x`). No tag is `free`: LocOS keeps `{0}` and `%1$s` as plain text in ODF.
- Merge: nothing translated returns the original bytes; target-language stamps need `Document::target_lang` and at least
  one translated unit (lead decision 11-04). The model options "stamp even when nothing is translated" and "always write
  UTF-8" are not used yet; when they land the adapter needs them to match LocOS (`build_localized` always stamps and
  LocOS always re-serialises an edited part as UTF-8). Each part is otherwise written in its own encoding.
- Oracle: `oracle/corpus/odf/` (4 files built by `tools/gen_corpus.py`); add `odt`, `ods`, `odp`, `odg` to `FORMATS` in
  `oracle/run.py` and to `scoreboard.py` (not touched here).
- Known difference: unit texts are kept when they hold a letter by Rust `char::is_alphabetic`, LocOS by Python
  `str.isalpha`; Rust is wider only for combining marks (a paragraph made only of marks), until the shared helper lands.

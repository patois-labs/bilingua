# Wiring bilingua-scc (the lead replaces the throwaway wire commit at merge)

- Format id: `scc`. Extension: `scc`. Filter type: `bilingua_scc::SccFilter` (unit struct, `Filter` impl).
- Facade `filters()`: add `Box::new(bilingua_scc::SccFilter)` anywhere before the text filter. `sniff` is strict: the
  first non-blank line must start with `Scenarist_SCC V` (UTF-8 or UTF-16 with BOM), so it cannot take another format.
- Facade `Cargo.toml`: `bilingua-scc = { workspace = true }`. Workspace: member `bilingua-scc` and
  `bilingua-scc = { path = "bilingua-scc" }`. The crate depends on `unicode-normalization = "0.1"` (NFC, as LocOS
  `unicodedata.normalize`), which is new to the workspace lock file; pure Rust, builds for wasm32.
- `bilingua/tests/contract.rs` `crate_of`: `"scc" => "bilingua-scc"`. `tools/check-portable.sh`/`.ps1` `CRATES`: add `scc`.
- Options: none. A merge never stamps a language. `MergeOptions::locos()` (write UTF-8) turns a UTF-16 file into
  UTF-8 as LocOS does; the default keeps the file's encoding.
- Adapter note: the LocOS `_byte_guard` (return the upload when every written text equals the unit's own text) is
  adapter work; the filter's untouched/echo merge is already byte exact.
- Errors: a refused merge is `Error::Write` whose text is the LocOS `CaptionTargetError` text (after the
  `cannot write: ` prefix). Parse errors are `Error::Parse`, size errors `Error::Limit`.
- Oracle corpus: `oracle/corpus/scc/` (12 files, made by `tests/tools/gen_corpus.py`).
- Warning kind `scc_lines_kept_verbatim` carries the count LocOS keeps in `skeleton["scc_lines_kept_verbatim"]`.

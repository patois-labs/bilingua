# Wiring for the lead

- Workspace: `"bilingua-stl"` in `members`, `bilingua-stl = { path = "bilingua-stl" }` in `[workspace.dependencies]`
  (done in the last commit, replace at merge). New external dependency: `unicode-normalization = "0.1"` with
  `default-features = false` (NFC and NFD for the table 00 floating diacritics and for Greek, Cyrillic and Arabic
  text; pulls in `tinyvec`; both are portable to wasm). Dev-dependencies: `proptest = "1"`, `serde_json`.
- Facade (`bilingua/Cargo.toml` and `filters()`): `Box::new(bilingua_stl::Stl)`. Format id `stl`, extension `stl`.
  `sniff` is strict (bytes 3 to 11 read `STL25.01` or `STL30.01`), so its place in `filters()` does not matter. It
  separates the format from 3D-model `.stl` files, which LocOS also refuses.
- `bilingua/tests/contract.rs`: `"stl" => "bilingua-stl"` in `crate_of`.
- `tools/check-portable.sh` and `.ps1`: `stl` in `CRATES`.
- Skeleton: `Skeleton::Zip` with one entry `subtitles.stl` holding the file bytes (`ZipData::Raw`, base64 in JSON), as
  `bilingua-mo` does; the model has no binary skeleton. The skeleton `parser` is `bilingua-stl`, schema 1.
- Oracle: `oracle/corpus/stl/`; add `stl` to `FORMATS` in `oracle/run.py` and the LocOS name in `scoreboard.py` (not
  touched here).
- Options: none beyond `target_lang` (the LC language byte), `limits` and, on merge, `stamp_target_always`.
  `output_encoding` has no effect: the file's own code table is kept (a table is switched only when a translation
  needs it, as LocOS does).
- Tags: control codes inside a row are `Standalone`, not free, `x` tokens in LocOS, one to one. No tag is a
  placeholder, so no `label` is set to `placeholder`.

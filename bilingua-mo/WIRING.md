# Wiring for the lead

- Workspace: `"bilingua-mo"` in `members`, `bilingua-mo = { path = "bilingua-mo" }` in `[workspace.dependencies]`
  (done in the last commit, replace at merge). Dev-dependencies: `proptest = "1"`, `serde_json`, `bilingua-po`
  (the units-equal-PO test).
- Facade (`bilingua/Cargo.toml` and `filters()`): `Box::new(bilingua_mo::Mo)`. Format id `mo`, extensions `mo` and
  `gmo`. `sniff` is strict (the 4-byte magic number in either byte order and at least 28 bytes), so its place in
  `filters()` does not matter; it is before YAML.
- `bilingua/tests/contract.rs`: `"mo" => "bilingua-mo"` in `crate_of`.
- `tools/check-portable.sh` and `.ps1`: `mo` in `CRATES`.
- Skeleton: `Skeleton::Zip` with one entry `catalog.mo` holding the file bytes (`ZipData::Raw`, base64 in JSON); the
  model has no binary text skeleton (see the report). The skeleton `parser` is `bilingua-mo`, schema 1.
- Oracle: `oracle/corpus/mo/`; add `mo` to `FORMATS` in `oracle/run.py` and the LocOS name in `scoreboard.py` (not
  touched here).
- Options: none beyond `target_lang` (stamp and plural fill) and `limits`. Merge always keeps the file's encoding;
  LocOS's "always UTF-8" and "stamp even when nothing changed" are the two merge options the adapter will need.

# Wiring for the lead

- Workspace: `"bilingua-resjson"` in `members`, `bilingua-resjson = { path = "bilingua-resjson" }` in
  `[workspace.dependencies]` (done in the last commit, replace at merge). Dev-dependencies: `proptest = "1"`,
  `serde_json`.
- Facade (`bilingua/Cargo.toml` and `filters()`): `Box::new(bilingua_resjson::Resjson)`, placed before
  `bilingua_json::JsonFilter`. Format id `resjson`, extension `resjson`. `sniff` is conservative (comment or
  `_id.comment` required).
- `bilingua/tests/contract.rs`: `"resjson" => "bilingua-resjson"` in `crate_of`, and the corpus walker skips
  dot files (the corpus folder has a `.gitattributes` with `* -text`).
- Skeleton `encoding` values: `utf-8`, `utf-8-bom`, `utf-16le`, `utf-16be` (UTF-16 always has its BOM).
- Oracle: `oracle/corpus/resjson/`; add `resjson` to `FORMATS` in `oracle/run.py` and the LocOS name in
  `scoreboard.py` (not touched here).
- Options: none. Limits: `Resjson.extract_with(bytes, opts, &Limits)`.

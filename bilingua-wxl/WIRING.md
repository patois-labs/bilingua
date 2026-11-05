# Wiring for the lead

- Format id `wxl`, extension `wxl`. `Box::new(bilingua_wxl::Wxl)` in `filters()`.
- `sniff` is tight: only a file whose root element (any prefix) is `WixLocalization`. It can go anywhere, but before
  any generic XML filter (wave 3 `bilingua-xml`, DITA, DocBook) that would otherwise claim the file; the extension
  `.wxl` already decides by extension.
- Workspace: `"bilingua-wxl"` in `members`, `bilingua-wxl = { path = "bilingua-wxl" }` in `[workspace.dependencies]`;
  facade `bilingua/Cargo.toml` dependency; `crate_of`: `"wxl" => "bilingua-wxl"` in `bilingua/tests/contract.rs`;
  `wxl` in the `CRATES` lists of `tools/check-portable.sh` and `.ps1` (done in the wire commit, replace at merge).
- Dev-dependencies: `proptest = "1"`, `serde_json`, `bilingua-seg`.
- Options: none. Limits come from `ExtractOptions::limits`. `target_lang` drives the `Culture` rewrite on merge.
- Skeleton `encoding` values are the model's `enc` tags (`utf-8`, `utf-8-bom`, `utf-16le`, `windows-1252`, ...).
- Oracle: corpus in `oracle/corpus/wxl/`; add `wxl` to `FORMATS` in `oracle/run.py` and the LocOS name in
  `scoreboard.py` (not touched here).
- LocOS adapter: references are standalone free tags with `codes[id].label == "placeholder"` and the exact text as
  `standalone`; flatten them to plain text. Hidden codes `$raw`, `$lead`, `$trail`, `$form` are not tags.

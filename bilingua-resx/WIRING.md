# Wiring for the lead

- Workspace: `"bilingua-resx"` in `members`, `bilingua-resx = { path = "bilingua-resx" }` in `[workspace.dependencies]`
  (the last commit of this branch does it for building; replace at merge). Dependencies: `quick-xml` (workspace),
  dev: `proptest`, `serde_json`.
- Facade: dependency `bilingua-resx = { workspace = true }`; in `filters()`: `Box::new(bilingua_resx::Resx)`;
  `crate_of`: `"resx" => "bilingua-resx"`; add `resx` to `CRATES` in `tools/check-portable.sh` and `.ps1`.
- Format id `resx`. Extensions `resx`, `resw`. Detection by extension. `sniff` is moderately strict (needs `<root` and
  `<resheader`, `<data name=` or `microsoft-resx` in the first 8 KB) but it sees a generic XML file as a match when it
  has those words: put it before any generic XML filter (wave 3 `bilingua-xml`) and before `Html`/`Text`.
- Options: none. `Resx.extract_with(bytes, opts, &Limits)` takes custom limits.
- Skeleton `encoding` values: `utf-8`, `utf-8-bom`, `utf-16le`, `utf-16be` (BOM), `windows-1252`.
- Oracle: `oracle/corpus/resx/` holds the made-up corpus; add `resx`/`resw` to `FORMATS` in `run.py`. Placeholder
  tags carry `label: placeholder` and display = the exact text: the adapter must flatten them, because LocOS keeps
  `{0}` as plain text in resx.
- `src/io.rs` holds crate-local stand-ins for the B6 limits, XML guard and encodings (replaced by the shared ones).

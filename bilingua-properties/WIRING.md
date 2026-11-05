# Wiring for the lead

- Workspace: add `"bilingua-properties"` to `members` and `bilingua-properties = { path = "bilingua-properties" }` to
  `[workspace.dependencies]` (the last commit of this branch does it for building; replace at merge). The crate
  also needs `regex` (already a workspace dependency); dev-dependencies are `proptest = "1"` and `serde_json`.
- Facade filter list: `Box::new(bilingua_properties::Properties)` (a unit struct).
- Format id: `properties`. Extensions: `properties`. Detection: by extension; `sniff` is conservative, put it last.
- Options: none needed. `Properties.extract_with(bytes, opts, &Limits)` takes custom limits.
- Skeleton `encoding` values: `utf-8`, `utf-8-bom`, `ascii`, `ascii-bom`, `iso-8859-1`.
- Oracle: `oracle/corpus/properties/` holds the made-up corpus; add `properties` to `FORMATS` in `run.py`.

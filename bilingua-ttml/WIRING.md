# Wiring for the lead

- Workspace: `"bilingua-ttml"` in `members`, `bilingua-ttml = { path = "bilingua-ttml" }` in `[workspace.dependencies]`
  (done in the wire commit), plus the facade dependency, the `filters()` entry and the `crate_of` row.
- Facade filter list: `Box::new(bilingua_ttml::Ttml)`. Format id `ttml`, extensions `ttml` and `dfxp`.
- `sniff`: tight. XML whose first 4 KB contains `www.w3.org/ns/ttml`, `www.w3.org/2006/10/ttaf1`,
  `www.w3.org/2006/04/ttaf1` or `www.w3.org/2004/11/ttaf1` (the first DFXP draft; refused under the locos profile). It must come before any generic XML filter (wave 3 `xml`, `xslt`) and before `Html`;
  after the extension-specific formats is fine. (An XLIFF or IMSC1 `.xml` that names the TTML namespace is taken by it.)
- Options: none. `merge_with` honours `MergeOptions` (`stamp_target_always`, `output_encoding`). The adapter should
  pass `MergeOptions::locos()`.
- Oracle: `oracle/corpus/ttml/` holds the made-up corpus plus the LocOS suite fixture. `oracle/run.py` needs
  `"ttml": "TTML", "dfxp": "TTML"` in `FORMATS` and `"TTML"` in `ORDER` (not edited here).
- Unit tags: `Open`/`Close` for `<span>` (LocOS `g`), `Standalone` for `<br/>` and whole other elements (LocOS `x`).
  No tag is `free` and none is a `placeholder`: LocOS keeps `{0}` and `%1$s` as text and so do we.
- Skeleton `encoding` values: `utf-8`, `utf-8-bom`, `utf-16le`, `utf-16be`, or an `encoding_rs` name.
- Skeleton: text parts, one unit per captioned `<p>` content; hidden codes `~raw` (the content as written) and `~meta`
  (`caption index|xml:space preserve|br element name`).

# Wiring for the lead

- Workspace: `"bilingua-ttx"` in `members`, `bilingua-ttx = { path = "bilingua-ttx" }` in `[workspace.dependencies]`
  (done in the wire commit), the facade dependency in `bilingua/bilingua/Cargo.toml`, `ttx` in the `CRATES` of
  `tools/check-portable.sh`.
- Facade filter list: `Box::new(bilingua_ttx::Ttx::default())`. Format id `ttx`, extension `ttx`.
- `sniff`: tight. The first 4 KB (UTF-16 or UTF-8) must hold a `<TRADOStag` start tag. Its place in `filters()` does
  not matter.
- `own_segmentation`: add `"ttx"` to the list in `bilingua/src/lib.rs` (the `<Tu>` is already the segment, as in XLIFF);
  otherwise sentence segmentation splits the source of a unit and loaded targets are lost. Merge works either way.
- `crate_of` in `tests/contract.rs`: `"ttx" => "bilingua-ttx"`.
- Options: `Ttx { skip_translated: bool }`. The LocOS adapter should use `Ttx::locos()` (a finished translation is not
  a unit). The facade has no options hook yet.
- Adapter notes: a file with no `<Tu>` has no units and a warning of kind `ttx_not_segmented` (LocOS raises
  `UnsegmentedTtxError`); with `Ttx::locos()` and zero units and a translated `<Tu>` LocOS raises "already has a
  translation". The language stamp when nothing is translated (LocOS behaviour) needs the coming merge option.
- Skeleton `encoding` values: `utf-8`, `utf-8-bom`, `utf-16le`, `utf-16be` (BOM), `utf-16le-nobom`, `utf-16be-nobom`,
  plus `;nl=crlf` and `;us=<part>`. The model has no BOM-less UTF-16 base, so the crate writes its own.
- Inline codes map one to one onto LocOS tokens (`g` pairs and `df` wrappers, `x` points); none is a placeholder.
- Oracle: `oracle/corpus/ttx/` is the made-up corpus (11 languages, four encodings), the LocOS suite sample and one
  Okapi resource. Files with no `<Tu>` (the LocOS unsegmented fixture, Okapi `Issue_164`) are not in the corpus because
  the corpus test wants units; they are in `bilingua-ttx/tests/data/`. Add `ttx` to `FORMATS` in `run.py`. LocOS
  refuses `okapi_test01.ttx` (all translated); Bilingua gives units with their targets loaded.

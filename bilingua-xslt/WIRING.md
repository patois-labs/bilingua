# Wiring for the lead

- Workspace: `"bilingua-xslt"` in `members`, `bilingua-xslt = { path = "bilingua-xslt" }` in
  `[workspace.dependencies]`, `bilingua-xslt = { workspace = true }` in `bilingua/Cargo.toml` (all done in the wire
  commit). The crate needs only `bilingua-model` (`enc`, `guard`, `locos`); no other dependencies.
- Facade filter list: `Box::new(bilingua_xslt::Xslt)` (a unit struct, `Default`). Format id `xslt`, extensions
  `xsl` and `xslt`. Put it with the other XML formats, after `Xaml`, before the loose sniffers.
- `sniff`: tight. The bytes (BOM and UTF-16 aware) must start with `<` and name `http://www.w3.org/1999/XSL/Transform`
  in the first 4 KB. A plain `.xsl` file is found by extension anyway; the sniffer only matters for files without
  one. It does not match XHTML, XAML, RESX or generic XML.
- `crate_of` row in `bilingua/tests/contract.rs`: `"xslt" => "bilingua-xslt"` (done).
- `tools/check-portable.sh` and `.ps1`: `xslt` added to the crate lists (done).
- Options: none. Skeleton `encoding` values: `utf-8`, `utf-8-bom`, `utf-16le`, `utf-16be`, or an `encoding_rs` name,
  with the usual `;nl=` and `;label=` suffixes from `enc::decode_text`.
- Oracle: `oracle/corpus/xslt/` holds the LocOS suite fixtures that parse plus made-up language and encoding files;
  add `xsl` and `xslt` to `FORMATS` in `run.py` (the oracle code is yours; not edited here). LocOS reads the `xsl`
  extension; the hostile and empty LocOS fixtures are in the crate's `tests/data`, not in the corpus.
- Tags: a paired tag (`Open`/`Close`, same id) maps to LocOS `g`, a standalone tag to `x` (nothing here is free, so
  no `p`); ids are the LocOS token numbers, so `to_locos_text` reproduces LocOS's text and the adapter needs no
  flattening. Plain-text `{0}` / `%s` are not tags (as in LocOS).
- Hidden codes (ids starting with `~`: `~raw`, `~meta`, `~nest`, `~v<id>`, `~n<id>`) are part of the skeleton
  contract of this crate; an adapter that copies units must copy `codes` whole.

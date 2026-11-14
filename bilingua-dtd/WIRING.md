# Wiring bilingua-dtd (the lead replaces the throwaway wire commit at merge)

- Format id: `dtd`. Extension: `dtd`. Filter type: `bilingua_dtd::DtdFilter` (unit struct, `Filter` impl).
- Facade `filters()`: add `Box::new(bilingua_dtd::DtdFilter)` after the Strings filter. The sniffer is strict: after
  an optional BOM, white space, comments and processing instructions, the file must start with `<!ENTITY`,
  `<!ELEMENT` or `<!ATTLIST` (only the first 16 KB are read; a head that ends inside a leading comment counts only
  when an `<!ENTITY` is in that head). It never claims XML, HTML or key=value text, so its place is not critical.
- Facade `Cargo.toml`: `bilingua-dtd = { workspace = true }`. Workspace: member `bilingua-dtd` and
  `bilingua-dtd = { path = "bilingua-dtd" }`. Dev-dependency `proptest = "1"` is crate-local.
- `bilingua/tests/contract.rs` `crate_of`: `"dtd" => "bilingua-dtd"`. `tools/check-portable.sh` and `.ps1`: add `dtd`.
- Options: none. `ExtractOptions::include_locked` adds values that are only references, markup or symbols as locked
  units. `DtdFilter::extract_with(bytes, opts, &Limits)` takes explicit size limits; `extract` uses
  `ExtractOptions::limits()` (B6 `bilingua_model::Limits`, re-exported as `bilingua_dtd::Limits`): `max_input_bytes`,
  `max_lines`, `max_units` (also the declaration budget, 4 x units), `max_unit_chars`. Refusals are `Error::Limit`.
- Inline tags (the LocOS adapter flattens them): every tag has label `placeholder` and its display is the exact source
  text, because LocOS keeps references and markup as plain text. Entity references (`&brandShortName;`, `&lt;`) are
  standalone free tags; markup (`<html:b>`, `&lt;i>`) is an open/close pair when it has its partner, else standalone.
- Notes: the comments directly above an entity (no blank line between) are `context.notes`; key = entity name.
- Oracle corpus: `oracle/corpus/dtd/` (15 files, made by `tests/tools/gen_corpus.py`; includes an Okapi test file).
- Python wheel: nothing format-specific; it goes through the facade.

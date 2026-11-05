# Wiring bilingua-mif (the lead replaces the throwaway wire commit at merge)

- Format id: `mif` (the LocOS format name). Extension: `mif`. Filter type: `bilingua_mif::Mif` (unit struct, `Filter`).
- Facade `filters()`: add `Box::new(bilingua_mif::Mif)`. `sniff` is strict: it reads the first 8 KB and wants an
  optional BOM, white space and `#` comment lines, then `<MIFFile version`. It never claims XML, text or other MIF
  statement files (`<Book`, `<MIFFile` only), so it can go anywhere; before the loose text sniffer is fine.
- Facade `Cargo.toml`: `bilingua-mif = { workspace = true }`. Workspace: member `bilingua-mif` and
  `bilingua-mif = { path = "bilingua-mif" }`. `proptest` is a crate-local dev-dependency.
- `bilingua/tests/contract.rs` `crate_of`: `"mif" => "bilingua-mif"`.
- `bilingua/tools/check-portable.sh` (and `.ps1` when present): add `mif` to `CRATES`.
- Options: none. `ExtractOptions::limits` is honoured: `max_input_bytes`, `max_units` (also the statement budget,
  200 per unit), `max_unit_chars`, `max_xml_depth` (statement nesting, default 256).
- Merge: when at least one unit has a target and `Document::target_lang` is set, every `PgfLanguage` and `FLanguage`
  keyword is set to FrameMaker's keyword for that locale (`NoLanguage` when FrameMaker has none). LocOS stamps even
  when nothing is translated; that waits for the model option the lead announced ("stamp even when nothing is
  translated"). The file's encoding (UTF-8, BOM kept) is always kept; LocOS also writes UTF-8 here, so the "always
  write UTF-8" option changes nothing for MIF.
- Python wheel (`bilingua-py`): nothing format-specific.

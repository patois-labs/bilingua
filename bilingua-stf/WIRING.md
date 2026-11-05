# Wiring bilingua-stf (the lead replaces the throwaway wire commit at merge)

- Format id: `stf`. Extension: `stf`. Filter type: `bilingua_stf::StfFilter` (unit struct, `Filter` impl).
- Facade `filters()`: add `Box::new(bilingua_stf::StfFilter)` before the INI filter. The sniffer is strict (a
  `Language code:` header plus a `# KEY<TAB>...LABEL` column line or a `Type:` header, in the first 8 KB), so its
  position is not critical; it must not come after a loose key=value sniffer.
- Facade `Cargo.toml`: `bilingua-stf = { workspace = true }`. Workspace: member and `[workspace.dependencies]` path entry.
- `bilingua/tests/contract.rs` `crate_of`: `"stf" => "bilingua-stf"`. `tools/check-portable.sh` and `.ps1` CRATES lists: `stf`.
- Options: `stf.load_targets` in `ExtractOptions::format_options` (default `true`: existing Bilingual translations
  become targets, `Translated` or `Draft` when out of date). The LocOS adapter sets `false` (exact LocOS unit list,
  no targets). Constant: `bilingua_stf::LOAD_TARGETS`. `profile::locos().merge_options("stf")` gives the LocOS behaviour (UTF-8 out, `Language code:` stamped even when nothing is translated).
- Extra public API: `bilingua_stf::records(bytes)` (LocOS `records()` for key projects), `over_limit(&doc)` (Salesforce label
  limits; also `context.max_len` on each unit), `max_length_for_key`, `salesforce_language_code`.
- Oracle corpus: `oracle/corpus/stf/` (22 files, made by `tests/tools/gen_corpus.py`).
- Python wheel: nothing format-specific; it goes through the facade.

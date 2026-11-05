# Wiring bilingua-ini (the lead replaces the throwaway wire commit at merge)

- Format id: `ini`. Extension: `ini`. Filter type: `bilingua_ini::IniFilter` (unit struct, `Filter` impl).
- Facade `filters()`: add `Box::new(bilingua_ini::IniFilter)` after the Markdown filter and before HTML and text
  (sniffing is conservative: a `[section]` line or `key=value` lines, no prose lines).
- Facade `Cargo.toml`: `bilingua-ini = { workspace = true }`. Workspace: member `bilingua-ini` and
  `bilingua-ini = { path = "bilingua-ini" }`. Dev-dependency `proptest = "1"` is new to the workspace (crate-local).
- `bilingua/tests/contract.rs` `crate_of`: `"ini" => "bilingua-ini"`.
- Options: none. `ExtractOptions::include_locked` adds values that are only placeholders or symbols (`%d`, `---`)
  as locked units. `IniFilter::extract_with(bytes, opts, &Limits)` takes explicit size limits (defaults are the
  LocOS ones: 50000 units, 100000 characters per value, 1000000 line breaks).
- Extra public API: `bilingua_ini::records(bytes)` is the LocOS `records()` view (key, value, comment, section,
  unit index, duplicate), for key-based projects.
- Oracle corpus: `oracle/corpus/ini/` (21 files, made by `tests/tools/gen_corpus.py`).
- Python wheel: nothing format-specific; it goes through the facade.

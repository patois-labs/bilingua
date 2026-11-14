# Facade wiring for bilingua-toml (the lead replaces the wire commit with the real one)

- Workspace `Cargo.toml`: member `bilingua-toml`; `[workspace.dependencies] bilingua-toml = { path = "bilingua-toml" }`.
- `bilingua/Cargo.toml`: `bilingua-toml = { workspace = true }`.
- `bilingua/src/lib.rs` `filters()`: `Box::new(bilingua_toml::TomlFilter)` before `bilingua_json::JsonFilter`.
- Format id `toml`; extension `toml`; `sniff` is true only for a valid TOML file that has at least one pair or table
  header, so it is safe in the extension-less fallback. No options. Needs nothing new in `bilingua-model`.
- `bilingua/tests/contract.rs` `crate_of`: `"toml" => "bilingua-toml"`.
- `bilingua-py`: nothing format specific.
- `bilingua/tests/contract.rs` reads every file in `oracle/corpus/*`, so the facade must know `toml` before the
  corpus folder `oracle/corpus/toml/` is merged.

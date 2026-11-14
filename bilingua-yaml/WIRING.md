# Wiring bilingua-yaml into the facade (the lead does this at merge)

- Format id: `yaml`
- Extensions: `yaml`, `yml`
- Filter type: `bilingua_yaml::Yaml` (unit struct, implements `bilingua_model::Filter`)
- Detection (`sniff`): file starts with `%YAML`, `---`, a `- ` entry or a `key:` line; JSON-looking and HTML-looking
  files are not claimed, nor `key = ...` lines (TOML, .strings, JS) and `//` comments. Still loose (any `word: text`
  line), so it goes after the stricter key-value formats and before properties. Extension wins; `sniff` is only for
  files with no extension.
- Options: `ExtractOptions::target_lang` matters at merge time. When it is set and the file is a Rails locale file
  (single root key that is a locale, holding a mapping) the root key is renamed to the target and each plural map
  gets the target's missing CLDR categories. Nothing else in `ExtractOptions` is used.
- Extra entry point: `Yaml::extract_with(bytes, &opts, &Limits)`. `Limits::default()` is the LocOS policy (50000
  units, 100000 characters per value, nesting 200, 64 MiB input); `extract` uses it. `Limits::unlimited()` lifts the
  unit, length and size caps (nesting stays 200).
- Workspace: `bilingua-yaml` as a member and in `[workspace.dependencies]`; the crate depends on `bilingua-model` and
  `regex` only (`proptest` and `serde_json` for tests and examples).
- Skeleton: text skeleton, encoding label one of `utf-8`, `utf-8-bom`, `utf-16le`, `utf-16be`, `cp1252`.
- Oracle formats table (`oracle/run.py` FORMATS and ORDER): `yaml`, `yml` -> `YAML`.

## Exact lines (tried locally against the facade; the whole workspace passes with them)

- `bilingua/Cargo.toml` dependencies: `bilingua-yaml = { workspace = true }`
- `bilingua/src/lib.rs` `filters()`, after `bilingua_jsmodule::JsModule` and before `bilingua_properties::Properties`
  (the sniff is loose; see Detection): `Box::new(bilingua_yaml::Yaml),`
- `bilingua/tests/contract.rs` `crate_of`: `"yaml" | "yml" => "bilingua-yaml",`
- `oracle/run.py`: `FORMATS` gets `"yaml": "YAML", "yml": "YAML"`, and `ORDER` gets `"YAML"`.
- The facade's `contract.rs` reads every file under `oracle/corpus/*/`, so the corpus folder `oracle/corpus/yaml/`
  (5 files, no notes file inside) fails that test until the facade knows YAML.

## Difference from LocOS to decide on

LocOS `build_localized` renames the Rails root key even when nothing was translated. Bilingua's contract says an
untouched document merges to its original bytes even with a target language set (the facade contract test opens every
corpus file with `target_lang = es-ES`), so the root rename and the plural fill happen only once at least one unit
has a target (the same rule PO uses for its header). Translated documents are byte-identical to LocOS.

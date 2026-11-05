# Wiring bilingua-yaml into the facade (the lead does this at merge)

- Format id: `yaml`
- Extensions: `yaml`, `yml`
- Filter type: `bilingua_yaml::Yaml` (unit struct, implements `bilingua_model::Filter`)
- Detection (`sniff`): file starts with `%YAML`, `---`, a `- ` entry or a `key:` line; JSON-looking and HTML-looking
  files are not claimed. Extension wins; `sniff` is only for files with no extension.
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

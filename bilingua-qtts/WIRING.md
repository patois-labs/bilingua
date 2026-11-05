# Wiring bilingua-qtts (the lead replaces the throwaway wire commit at merge)

- Format id: `ts` (the LocOS format name). Extension: `ts`. Filter type: `bilingua_qtts::QtTs` (unit struct).
- Workspace: member `bilingua-qtts` and `bilingua-qtts = { path = "bilingua-qtts" }` in `[workspace.dependencies]`;
  facade `bilingua/Cargo.toml`: `bilingua-qtts = { workspace = true }`. Dev-dependency `proptest` is crate-local.
- `bilingua/tests/contract.rs` `crate_of`: `"ts" => "bilingua-qtts"`. `tools/check-portable.sh` and `.ps1`: `qtts`.
- **Facade `filters()`:** add `Box::new(bilingua_qtts::QtTs)` right before `Box::new(bilingua_jsmodule::JsModule)`.
  `QtTs::sniff` is strict (XML whose first element is `<TS`), so it is safe anywhere before `Html`; it must come
  before the loose sniffers (YAML, properties, HTML, text) for files without an extension.
- **Facade `detect()` (the `.ts` change, made in the wire commit):** the extension loop picks the first filter that
  claims `ts`, which would send every TypeScript file to the Qt filter (it sits before JsModule) or every Qt file to
  JsModule. `.ts` is therefore decided by sniffing, before the loop:

  ```rust
  if ext == "ts" {
      return Some(if bilingua_qtts::QtTs.sniff(bytes) { Box::new(bilingua_qtts::QtTs) } else { Box::new(bilingua_jsmodule::JsModule) });
  }
  ```

  A Qt file is XML with a `<TS` root; anything else with a `.ts` extension goes to JsModule, which refuses XML with
  "This file is XML (a Qt Linguist .ts file?)" and reads TypeScript modules. `mts` and `cts` stay JsModule's.
  `QtTs::extensions()` is `["ts"]` so listings of supported extensions are unchanged. Test:
  `bilingua/tests/ts_sniff.rs`.
- Options: none. Target language through `ExtractOptions::target_lang` (stamps `language`, fills numerus forms).
  Skeleton `encoding` values: `utf-8`, `utf-8-bom`, `utf-16le`, `utf-16be`, or an `encoding_rs` name.
- Oracle: `oracle/corpus/ts/` (the made-up languages and edge files; copies of `tests/data`); add `ts` to
  `FORMATS` in `oracle/run.py` and the LocOS name `ts` in `scoreboard.py` (the oracle code is the lead's, not
  touched here). The corpus holds `.ts` files only, no dot files.
- Python wheel: nothing format-specific; it goes through the facade.
- Model needs: none.

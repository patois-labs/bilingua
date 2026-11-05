# Wiring bilingua-jsmodule (the lead replaces the throwaway wire commit at merge)

- Format id: `js` (the LocOS format name). Extensions: `js`, `cjs`, `mjs`, `ts`, `mts`, `cts`. Filter type:
  `bilingua_jsmodule::JsModule` (unit struct, `Filter` impl).
- Facade `filters()`: add `Box::new(bilingua_jsmodule::JsModule)` after the Markdown filter and before HTML and text.
  Sniffing is strict (a whole-file parse with at least one export or declaration), so it never claims prose, JSON,
  XML or an empty file.
- Facade `Cargo.toml`: `bilingua-jsmodule = { workspace = true }`. Workspace: member `bilingua-jsmodule` and
  `bilingua-jsmodule = { path = "bilingua-jsmodule" }`. Dev-dependency `proptest = "1"` is crate-local.
- `bilingua/tests/contract.rs` `crate_of`: `"js" => "bilingua-jsmodule"`.
- **The `ts` extension is shared with Qt Linguist `.ts` (XML, a later wave).** `JsModule::sniff` recognises JS/TS
  module syntax and refuses XML (first non-blank byte `<`, with or without a BOM or UTF-16), and `extract` on XML
  returns `Error::Parse` ("This file is XML (a Qt Linguist .ts file?)"). `detect()` today takes the first filter that
  claims an extension, so when the Qt filter lands the facade must decide `.ts` by sniffing: ask `JsModule::sniff`
  first, and give the file to the Qt filter when it says no and the bytes are XML. LocOS does the same (its js filter
  hands XML to the registered `ts` filter). Until then an XML `.ts` is refused with a plain message.
- Options: none. `ExtractOptions::include_locked` adds string values that are only placeholders or symbols
  (`{{count}}`, `---`) as locked units. `JsModule::extract_with(bytes, opts, &Limits)` takes explicit size limits
  (defaults are the LocOS ones: 50000 strings, 100000 characters per value; plus 128 MiB per file).
- Extra public API: `bilingua_jsmodule::records(bytes)` is the LocOS `records()` view (dotted key, value, unit
  index), for key-based projects.
- Oracle corpus: `oracle/corpus/js/` (16 files, made by `tests/tools/gen_corpus.py`; it holds CRLF, BOM, Windows-1252 and UTF-16 files, which the
  root `.gitattributes` rule `bilingua/oracle/corpus/** -text` already protects; no file may sit in the folder that is
  not a corpus file, because the facade contract test opens everything there). `oracle/run.py` lists its FORMATS by
  extension: add `js cjs mjs ts mts cts` there to see the files in `REPORT.md` and the scoreboard (not done here:
  the oracle code is the lead's).
- Python wheel: nothing format-specific; it goes through the facade.

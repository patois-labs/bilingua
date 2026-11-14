# Wiring for the lead

Two filters in one crate: `Xcstrings` (id `xcstrings`, extension `xcstrings`) and `Xcloc` (id `xcloc`, extension
`xcloc`). `bilingua_xcstrings::filters()` returns both.

- Workspace: `"bilingua-xcstrings"` in `members`, `bilingua-xcstrings = { path = "bilingua-xcstrings" }` in
  `[workspace.dependencies]` (done in the wire commit, replace at merge). Dev-dependencies: `proptest = "1"`,
  `serde_json`. Normal dependencies: `bilingua-model`, `bilingua-zip`, `bilingua-xliff`, `regex`.
- Facade `Cargo.toml`: `bilingua-xcstrings = { workspace = true }`.
- Facade `filters()`: `Box::new(bilingua_xcstrings::Xcloc)` before `Docx` (a zip; its sniff wants `PK`,
  `contents.json` and `Localized Contents/` in the bytes), and `Box::new(bilingua_xcstrings::Xcstrings)` **before
  `bilingua_json::JsonFilter`**. Xcstrings is JSON: its sniff is strict (parses the file and needs a string
  `sourceLanguage` and an object `strings`), so it is safe to put early, and the generic JSON filter must not get
  there first for a file without an extension.
- Facade `own_segmentation`: add `"xcloc"` (the XLIFF inside keeps the file's own segments, merge finds them by
  id). `xcstrings` is a source file and is segmented like JSON.
- `bilingua/tests/contract.rs`: `"xcstrings" | "xcloc" => "bilingua-xcstrings"` in `crate_of`.
- `tools/check-portable.sh` and `.ps1`: `xcstrings` in `CRATES`.
- Skeletons: `xcstrings` is `Skeleton::Text` with one `Raw` part (the whole file without its BOM) and encoding
  `utf-8` or `utf-8-bom`; `xcloc` is `Skeleton::Zip` with one raw entry holding the whole archive.
- Options: none. `target_lang` on the `Document` picks the language a merge writes (see the crate docs);
  `include_locked` lists stale, `shouldTranslate: false` and placeholder-only strings as locked units.
- Oracle: `oracle/corpus/xcstrings/` and `oracle/corpus/xcloc/`; add `xcstrings` and `xcloc` to `FORMATS` in
  `oracle/run.py` and the LocOS names in `scoreboard.py` (not touched here). `.xcloc` files are binary zips.
- Facade corpus test: the `.xcloc` corpus files are zips; the facade finds them by extension.

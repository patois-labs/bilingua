# Wiring bilingua-ass (the lead replaces the throwaway wire commit at merge)

- Format id: `ass`. Extensions: `ass`, `ssa`. Filter type: `bilingua_ass::AssFilter` (unit struct, `Filter` impl).
- Facade `filters()`: add `Box::new(bilingua_ass::AssFilter)` next to the other subtitle filters (`Vtt`, `Srt`).
  Extension lookup is by `extensions()`, so nothing else is needed for `.ass` and `.ssa`.
- Detection (`sniff`): strict. True only when one of the first 40 lines (after a BOM, UTF-16 BOM read too) is exactly
  `[Script Info]`, `[V4+ Styles]`, `[V4 Styles]`, `[V4++ Styles]` or `[Events]` (case-insensitive). Prose and SRT/VTT
  never match, so it can sit anywhere among the subtitle filters; it does not need to go last.
- Facade `Cargo.toml`: `bilingua-ass = { workspace = true }`. Workspace: member `bilingua-ass` and
  `bilingua-ass = { path = "bilingua-ass" }`. `bilingua/tests/contract.rs` `crate_of`: `"ass" | "ssa" => "bilingua-ass"`
  (the `filters()` id is `ass`; the corpus has `.ssa` files too). `tools/check-portable.sh` and `.ps1`: add `ass`.
- Options: `ExtractOptions::include_locked` (or `format_options["ass.include_locked"] = "true"`) also extracts
  `Comment:` events as locked units (`translate = false`); default off, as LocOS. Merge: `merge_with` honours
  `MergeOptions` (`output_encoding`, `stamp_target_always`); `merge` is `merge_with(default)`.
- Inline codes and the LocOS adapter: every Bilingua tag maps to one LocOS token (`g` pair, `x` point). No code is a
  plain-text placeholder, so no tag needs the `placeholder` label. Labels: `italic`, `bold`, `underline`,
  `strikeout`, `style`, `style reset`, `override`, `line break` (`\N`), `soft break` (`\n`), `hard space` (`\h`).
  A tag's `display` is its exact source text (a reset block written for an inner pair of a shared `{\r}` shows the
  block it writes).
- Oracle corpus: `oracle/corpus/ass/` (25 files, made by `tests/tools/gen_corpus.py`; `.ass` and `.ssa`).
- Python wheel: nothing format-specific; it goes through the facade.

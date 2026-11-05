# Wiring for the lead

- Workspace: `"bilingua-wordfast"` in `members`, `bilingua-wordfast = { path = "bilingua-wordfast" }` in
  `[workspace.dependencies]`; facade dependency `bilingua-wordfast = { workspace = true }` (done in the wire commit,
  with `Cargo.lock`, the `CRATES` lists of `tools/check-portable.sh` and `.ps1`, and the corpus folder
  `oracle/corpus/wordfast/`). The crate uses `bilingua_model::{enc, guard}` (B6).
- Two filters, ids `txml` and `txlf`, extensions `txml` and `txlf` (nothing else). In `filters()`:
  `Box::new(bilingua_wordfast::Txml::default())` and `Box::new(bilingua_wordfast::Txlf::default())`.
- Sniff: `txml` = the first 4 KB contains `<txml` (tight). `txlf` = contains `<xliff` and an `xmlns` declaration
  whose URI names `gs4tr` (tight; the word in a note does not count). `detect` sends a `.xlf` / `.xliff` that sniffs
  as TXLF to `Txlf` (lead decision 11-05, same special-case block as `.ts` and ARB); a plain `.xlf` stays XLIFF. Because a
  TXLF is also an XLIFF, **`Txlf` must come before the `bilingua_xliff` filters** in `filters()`; by extension the
  match is exact either way. Neither is loose.
- `own_segmentation` in the facade: add `"txml" | "txlf"` (the files carry their segmentation, like XLIFF).
- `contract.rs` `crate_of`: `"txml" | "txlf" => "bilingua-wordfast"`.
- Options: `wordfast.load_targets` in `ExtractOptions::format_options` (wins), or `Txml { load_targets }` / `Txlf { load_targets }`, default `true`: confirmed
  segments are listed as ordinary units with their translation as the target, state `Translated`, as Okapi does (Brent, 11-05); sign-off and locks are read-only. The LocOS adapter uses
  `load_targets: false` (`Txml::without_targets()`), which gives exactly the LocOS unit list. `include_locked` in
  `ExtractOptions` also lists locked and localizable segments (read-only).
- Unit ids are LocOS's `p_index` as text (stable; `1000000000 + n` for `seg-source` segments). The adapter can use
  `int(unit.id)` as `p_index`.
- Tokens: TXML `ut` start/end pairs are `Open`/`Close` (`g`), other `ut`/foreign elements `Standalone` (`x`); TXLF
  `g`/`mrk` are `Open`/`Close`, `bpt`/`ept`/`it` etc. `Standalone` (`x`), `<x id>`/`<ph id>` free `Standalone` (`p`).
  No placeholder is flattened (every code is a real element), so no `placeholder` labels.
- Skeleton `encoding` values: `utf-8`, `utf-8-bom`, `utf-16le`, `utf-16be` (with `;nl=crlf` when the file uses CRLF
  throughout).
- `merge_with` honours `MergeOptions` (`stamp_target_always`, `output_encoding`). LocOS keeps a Wordfast file's
  UTF-16 on build, so for exact LocOS parity the adapter passes `stamp_target_always: true` and keeps the encoding.
- Oracle: `oracle/corpus/wordfast/` is the made-up corpus plus the LocOS suite fixtures and Okapi's three `okf_txml`
  resources; add `txml` and `txlf` to `FORMATS` in `run.py` (LocOS dump works as is).

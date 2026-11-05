# Wiring for the lead

- Workspace: add `"bilingua-xtg"` to `members` and `bilingua-xtg = { path = "bilingua-xtg" }` to
  `[workspace.dependencies]`. The crate needs only `bilingua-model` (and `encoding_rs` through it). Dev-dependencies:
  `proptest = "1"` and `serde_json`.
- Facade: dependency `bilingua-xtg = { workspace = true }`; filter `Box::new(bilingua_xtg::Xtg)` (a unit struct).
- Format id: `xtg`. Extensions: `xtg`, `tag` (LocOS registers `.tag` and the id `xtg`; `.txt` stays plain text).
- `sniff`: strict. The file must start (after an optional BOM, or as UTF-16) with the header tags `<vN...>` and/or
  `<eN>`. It can sit early in `filters()` (the wire commit puts it right after rc). It must come before HTML,
  generic XML and plain text, which accept `<`.
- Options used: `source_lang` (only the garbled-letters warning of a legacy code page, and the code page choice of a
  file with no `<e>` tag, by the shared text decode), `limits`. `target_lang` is not used (the format carries no
  language). `include_locked` is ignored (nothing is locked).
- Inline codes: every tag is a code. Labels: `bold`, `italic`, `font`, `size`, `colour`, `style sheet`, `special
  character`, `link`, `index`, `paragraph format`, `format`, `tag`... None is a `placeholder` (LocOS tokenises all
  of them). Pairs are `Open`/`Close` (`g`), the rest `Standalone` (`x`, not free).
- Oracle: `oracle/corpus/xtg/` holds the made-up corpus (16 files); add `xtg` and `tag` to `FORMATS` in `run.py`.
- Merge options still wanted from the model: "stamp the target language even when nothing is translated" is not
  needed (no language in the file); "always write UTF-8" is not needed (LocOS keeps the file's encoding here).

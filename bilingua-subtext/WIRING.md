# Wiring for the lead

- Workspace: `"bilingua-subtext"` in `members` and `bilingua-subtext = { path = "bilingua-subtext" }` in
  `[workspace.dependencies]` (done in the wire commit). Needs only `bilingua-model`. Dev-dependencies: `proptest`,
  `serde_json` (both already used).
- Facade: dependency `bilingua-subtext = { workspace = true }`; filters `Box::new(bilingua_subtext::Sub)` and
  `Box::new(bilingua_subtext::Sbv)` (unit structs, in that order).
- Format ids and extensions: `sbv` (`sbv`) and `sub` (`sub`; the dialect, SubViewer 2.0 or MicroDVD, is decided
  by content and kept per unit in the skeleton). `crate_of`: `"sbv" | "sub" => "bilingua-subtext"`.
- `sniff`: TIGHT, but not signature-based. `Sub`: MicroDVD `{n}{n}` cues, or SubViewer 2.0 timecode lines together
  with an `[INFORMATION]` or `[SUBTITLE]` header; never VobSub. `Sbv`: the first non-blank line is an
  `H:MM:SS.fff,H:MM:SS.fff` timecode pair and there is no MicroDVD cue. A headerless SubViewer file and an SBV file
  look the same; they are told apart by extension only. Wire commit places both after XML and before CSV/TSV (a
  timecode pair is not a CSV table but the order is cheap insurance); files with an extension never use the sniff.
- Options used: `source_lang` (code page of a non-UTF-8 file), `limits`. `target_lang`, `include_locked` and
  `format_options` change nothing. Merge: `MergeOptions::output_encoding` (default keep; `Utf8` for LocOS) is honoured;
  `stamp_target_always` has no effect (no language in the format).
- Skeleton `encoding`: the shared `bilingua_model::enc` tags.
- Hidden codes (ids start with `~`): `~raw`, `~kind`, `~joiner`, `~heads`, `~fps`.
- Inline codes: MicroDVD only. Pairs (`Open`/`Close`, same id, numbered by one counter) for line-scoped style codes,
  standalone (not free) tags for the rest; labels `style`, `colour`, `font`, `size`, `position`, `charset`,
  `control code`. None is labelled `placeholder`: `{0}` and `%s` are plain text in LocOS and here.
- Merge refusals are `Error::Write("cue N: ...")` (see the README list). The adapter should show the message; LocOS
  has no equivalent (it writes the text).
- Oracle: `oracle/corpus/subtext/` (8 files); add `sbv` and `sub` to `FORMATS` in `oracle/run.py`
  (`"sbv": "SBV", "sub": "SUB (SubViewer / MicroDVD)"` and the names to `ORDER`).
- Model needs: none blocking. (1) The reader falls back to the code page for a UTF-8 BOM followed by invalid UTF-8,
  where LocOS decodes UTF-8 with replacement characters. (2) `enc::encode_text` drops one leading U+FEFF; the crate
  works around it for a file that has a BOM and a U+FEFF character.

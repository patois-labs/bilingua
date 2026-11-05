# bilingua-stf

Salesforce Translation Workbench `.stf` filter for Bilingua (Source, Outdated and untranslated, Bilingual exports).
Port of the Localization OS STF filter (the baseline; Okapi has none).

- Every data row whose LABEL has text outside its placeholders is a unit; KEY, comments, headers, section markers and
  layout are skeleton, byte for byte. A translation goes into the TRANSLATION column when the section has one, else
  in place of the LABEL. An unchanged unit is not touched.
- Existing translations (the TRANSLATION cell of the Bilingual TRANSLATED section) load as segment targets by
  default: `Translated`, or `Draft` when OUT OF DATE is `*`. `stf.load_targets` = `false` gives the LocOS unit list
  with no targets. A loaded target merged back unchanged writes the original cell, and does not count as
  translated for the `Language code:` stamp, so an untouched merge stays byte for byte in both modes.
- Placeholders (`\n \t \r \`, `{0}`, `{!merge.field}`) are standalone inline tags (the LocOS `x` token).
- `Language code:` is stamped in Salesforce form when a target language is set and something is translated.
- Tests: `tests/locos_suite.rs` (10 of the 12 LocOS cases), `tests/locos_golden.rs` (352 files, 2000+ builds against
  recorded LocOS output from `tests/tools/gen_golden.py`), round trips, hostile input (proptest), speed
  (`cargo test -p bilingua-stf --release --test speed -- --nocapture`, then `tests/tools/speed_locos.py`).

Speed (5.4 MB, 76 697 units, every unit translated): Rust release on the M5 extracts in about 116 ms and translates and
merges in about 114 ms. LocOS parses the same file in 875 ms; its build splices each cell into one growing string
(quadratic): 0.27 MB takes 0.26 s, 0.54 MB 3.3 s, 1.08 MB 16.3 s, and the whole file did not finish in 50 minutes.

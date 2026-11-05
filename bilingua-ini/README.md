# bilingua-ini

INI and Joomla language file filter for Bilingua. Port of the Localization OS INI filter (the baseline for this
format; Okapi has no INI filter).

- Every `key = value` whose value has a letter or digit outside its placeholders is a unit. Keys, sections,
  comments, layout and line endings are skeleton, byte for byte.
- Quoted values keep their quote dialect (`\"` or the legacy Joomla `"_QQ_"`); a line break in a translation
  becomes a space (INI values are one line).
- UTF-8 (BOM kept), UTF-16 with BOM, Windows-1252 (written back as 1252 when every character fits, else UTF-8).
- Tests: `tests/locos_suite.rs` (the five LocOS cases), `tests/locos_golden.rs` (429 files against recorded LocOS
  output, made by `tests/tools/gen_golden.py`), round trips, hostile input (proptest), speed
  (`cargo test -p bilingua-ini --release --test speed -- --nocapture`, then `tests/tools/speed_locos.py`).

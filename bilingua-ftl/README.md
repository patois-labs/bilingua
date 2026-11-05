# bilingua-ftl

Mozilla Project Fluent (`.ftl`, syntax 1.0) filter for Bilingua. Port of the Localization OS Fluent filter (the
baseline; Okapi has no Fluent filter), with its own parser that follows `fluent.syntax` step for step.

- Units: message values and message attributes, cut into runs of text and simple placeables; each select-expression
  variant is its own unit (`ftl:emails[one]`). Terms are skipped unless `Limits::translate_terms` is set.
- Placeables are free inline tags (`placeholder`); `{ "{" }` style literals read as the plain character.
- Comments, Junk, identifiers, selectors, keys, indentation of untouched text, line endings and the BOM stay in the
  skeleton byte for byte. A translation can never change the file's structure: braces and line-start `[ * .` are
  escaped.
- `merge_localized` also adds the target language's missing plural variants to plural selects; plain `merge` does not.
- Tests: `tests/locos_suite.rs` (the ten LocOS cases), `tests/locos_golden.rs` (275 files against recorded LocOS
  output, made by `tests/tools/gen_golden.py`), round trips, hostile input (proptest), speed
  (`cargo test -p bilingua-ftl --release --test speed -- --nocapture`, then `tests/tools/speed_locos.py`).

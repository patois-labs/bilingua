# Wiring for the lead

- Workspace: `"bilingua-plist"` in `members`; `bilingua-plist = { path = "bilingua-plist" }` in
  `[workspace.dependencies]` (done in the wire commit), and in `bilingua/Cargo.toml`.
- Two filters, two format ids, one crate:
  - `Box::new(bilingua_plist::Stringsdict)`: id `stringsdict`, extension `stringsdict`.
  - `Box::new(bilingua_plist::Plist)`: id `plist`, extension `plist`.
  - `bilingua_plist::filters()` returns both in that order.
- `sniff`: `Plist` is true for `bplist00` and for XML whose first 4 KB holds `<plist` (UTF-8 or UTF-16). It does not
  accept XML without `<plist`, HTML, JSON, text or OpenStep. `Stringsdict` is `Plist` plus one of the stringsdict
  markers (`NSStringPluralRuleType`, `NSStringLocalizedFormatKey`, `NSStringDeviceSpecificRuleType`,
  `NSStringVariableWidthRuleType`) in the first 64 KB (anywhere in a binary file). Tight enough for the middle of
  `filters()`; `Stringsdict` must come before `Plist`, and both before any generic XML filter (wave 3 `xml`) and
  before `Html`.
- Options: none. `ExtractOptions::target_lang` drives the stringsdict plural fill at merge; `source_lang` is not used
  (XML plists are UTF-8 or UTF-16 only). `Plist.extract_with(bytes, opts, &Limits)` takes custom limits.
- Skeleton `encoding`: `utf-8`, `utf-8-bom`, `utf-16le`, `utf-16be` (with `;nl=crlf` where the file uses CRLF), or
  `bplist` for a binary file (the skeleton then holds the original file as base64 in its first part).
- Facade contract test: `"plist" | "stringsdict" => "bilingua-plist"` in `crate_of` (done in the wire commit).
- Python wheel: nothing format specific.
- Oracle: `oracle/corpus/plist/` holds both extensions; add `plist` and `stringsdict` to `FORMATS` in `run.py`. Note that
  the oracle's pseudo-translation step merges for `es-ES`, so a stringsdict whose plural dictionary lacks Spanish
  `many` re-opens with more units than it had: that is the plural fill, not a defect.
- `oracle/tests/corpus.rs` counts only the plural forms the source has (the last `/` step of a stringsdict key), so
  the Spanish `many` the fill adds is not a unit-count mismatch; all stringsdict files live in `oracle/corpus/plist/`.

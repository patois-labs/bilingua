# bilingua-xcstrings

Xcode localization for Bilingua: **String Catalogs** (`.xcstrings`) and **localization bundles** (`.xcloc`).
Behaviour follows Localization OS (`apple_filter.py`, `xcloc_filter.py`); see `src/lib.rs` for the rules and
`WIRING.md` for the facade lines.

| Test file | What it checks |
|---|---|
| `tests/locos_xcstrings.rs`, `tests/locos_xcloc.rs` | the two LocOS suites, case by case |
| `tests/golden.rs` | bytes against files written by the LocOS filter (`tools/gen_golden.py`): 5 catalogs x 11 languages + source, full and half translations |
| `tests/oracle.rs` | unit texts of the oracle corpus against LocOS (`tools/gen_oracle_units.py`), untouched and pseudo-translated merges |
| `tests/stringset.rs` | `stringSet` localizations (App Shortcuts phrases): units, stale, in-place and target merges, inside plurals |
| `tests/behaviour.rs`, `tests/xcloc_behaviour.rs` | tags, notes, CRLF, BOM, 11 languages, CLDR plural forms, byte-for-byte splices, language stamps |
| `tests/hostile.rs` | malformed, truncated, deep and huge input; proptest over random bytes, JSON-like text, mutated fixtures, generated catalogs |
| `tests/speed.rs` | 5.6 MB catalog and a 7 MB XLIFF in a bundle, timed (`--release -- --nocapture`) |

`tools/time_locos.py` times the LocOS filters on the files `tests/speed.rs` saves (`XC_SPEED_OUT=dir`).
`tools/gen_corpus.py` writes the made-up oracle corpus.

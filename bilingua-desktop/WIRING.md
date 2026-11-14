# Wiring bilingua-desktop (the lead replaces the throwaway wire commit at merge)

Two filters, one crate.

| | `desktop` | `catkeys` |
|---|---|---|
| Type | `bilingua_desktop::DesktopFilter` (unit struct) | `bilingua_desktop::CatkeysFilter` (unit struct) |
| Format id | `desktop` | `catkeys` |
| Extension | `desktop` | `catkeys` |
| `sniff` | tight: a line `[Desktop Entry]` in the first 4 KB | tight: first line is `1<TAB>language<TAB>signature<TAB>digits` |

* Facade `filters()`: put `Box::new(bilingua_desktop::DesktopFilter)` **before the INI filter** (a `.desktop` file is
  also a valid INI sniff: `[section]` plus `key=value`), and `Box::new(bilingua_desktop::CatkeysFilter)` anywhere
  before the text filter. Both sniffers are tight enough that order matters only for INI.
* Facade `Cargo.toml`: `bilingua-desktop = { workspace = true }`. Workspace: member `bilingua-desktop` and
  `bilingua-desktop = { path = "bilingua-desktop" }`. Dev-dependencies `proptest = "1"`, `serde_json`, `bilingua-seg`.
* `bilingua/tests/contract.rs` `crate_of`: `"desktop" => "bilingua-desktop"`, `"catkeys" => "bilingua-desktop"`.
* `tools/check-portable.sh` and `.ps1` CRATES: add `desktop`.
* Options: none. `ExtractOptions::include_locked` adds letterless `.catkeys` entries as locked units. Limits come
  from `ExtractOptions::limits`.
* Extra public API: `desktop_records`, `catkeys_records` (LocOS `records()`), `catkeys_fingerprint`, `posix_locale`,
  `language_name`, `catkeys_escape` / `catkeys_unescape`.
* Oracle corpus: `oracle/corpus/desktop/` and `oracle/corpus/catkeys/` (the crate's `tests/data` files).
  Add `desktop` and `catkeys` to `FORMATS` in `oracle/run.py` and the LocOS names in `scoreboard.py`.
* LocOS adapter: units are plain text (no tags). To get LocOS's behaviour the adapter needs the coming merge options:
  "stamp the target language even when nothing is translated" (catkeys header language) and "always write UTF-8"
  (not needed here, both formats are UTF-8 only). Without them an untouched merge returns the original bytes.
* Skeleton: `Skeleton::Text`; hidden code `$desktop` / `$catkeys` per unit holds the raw source value / raw
  translation. `catkeys` skeleton part 1 is the header language (convention used by `merge`).

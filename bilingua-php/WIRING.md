# Wiring bilingua-php (the lead replaces the throwaway wire commit at merge)

- Format id: `php` (the LocOS format name). Extension: `php`. Filter type: `bilingua_php::Php` (unit struct, `Filter`
  impl).
- Facade `filters()`: add `Box::new(bilingua_php::Php)` after the JsModule entry (any place before HTML and text works).
  `sniff` is strict: the bytes must open with `<?php` (after a BOM and white space; UTF-16 files are decoded first), the
  whole file must parse as a static language array and hold at least one `return` or `$var = ...` statement. It
  rejects other formats after looking at the first five bytes, so it costs nothing in `detect`.
- Facade `Cargo.toml`: `bilingua-php = { workspace = true }`. Workspace: member `bilingua-php` and
  `bilingua-php = { path = "bilingua-php" }`. `proptest` is a crate-local dev-dependency.
- `bilingua/tests/contract.rs` `crate_of`: `"php" => "bilingua-php"`.
- `bilingua/tools/check-portable.sh` and `.ps1`: add `php` to `CRATES`.
- Options: none. `ExtractOptions::include_locked` adds strings with no letter or digit as locked units.
  `Php::extract_with(bytes, opts, &Limits)` takes explicit limits (defaults are the LocOS ones).
- Extra public API: `bilingua_php::records(bytes)` is the LocOS `records()` view (dotted key, value, unit index) for
  key-based projects.
- LocOS routes `.php` to a content router (fmt/serverpages) that sends anything that is not a clean language array to a
  page filter; Bilingua has only the array filter, so any other `.php` file is refused with `Error::Parse` naming the
  line.
- Oracle corpus: `oracle/corpus/php/` (20 files, made by `tests/tools/gen_corpus.py`; 11 languages, BOM + CRLF,
  Windows-1252, UTF-16 LE and BE, CodeIgniter 3, heredoc / nowdoc, escapes, old-style arrays, closing tag). The root
  `.gitattributes` rule `bilingua/oracle/corpus/** -text` protects the bytes. `oracle/run.py` lists its FORMATS by
  extension: add `php` there to see the files in `REPORT.md` and the scoreboard (the oracle code is the lead's).
- Python wheel: nothing format-specific; it goes through the facade.

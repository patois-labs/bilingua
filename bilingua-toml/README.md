# bilingua-toml

TOML 1.0 filter for Bilingua (`.toml`): Hugo i18n files, go-i18n message files and any other TOML resource file.
LocOS (`app/filters/toml_filter.py`) is the baseline; Okapi has no TOML filter.

- Only string **values** are units. Keys (bare, quoted, dotted), table headers, numbers, booleans, dates, comments,
  blank lines, indentation, line endings and the UTF-8 BOM are never touched. `context.key` is the key path
  (`fruits[0].physical.color`, `dog."tater.man".inline.a.b`); ids are `u1`, `u2`, ... in file order.
- Merge splices each translation into the exact byte span of its original string token. The quoting style is kept
  when the translation fits it: basic `"..."`, literal `'...'`, multi-line basic `"""..."""`, multi-line literal
  `'''...'''`. A literal that cannot hold the translation (an apostrophe, a newline, a control character) becomes
  the matching basic string with escapes; a multi-line literal that cannot (`'''`, a trailing apostrophe) becomes
  multi-line basic. New line breaks inside a multi-line value follow the file (LF or CRLF); a line break right after
  the opening delimiter is kept. A value that is unchanged is written back as its original token, so escapes such as
  `é` and line-ending backslashes survive.
- Every rebuilt file is scanned again and each unit's key and value are checked before it is returned
  (`Error::Write` otherwise).
- go-i18n message tables (every key is `id`, `description`, `hash`, `leftdelim`, `rightdelim`, a plural category
  `zero|one|two|few|many|other` with a string value, or `translation`) get `context.style` `toml:<category>` on the
  plural forms and `description` as the unit's note. `id`, `hash`, `leftdelim`, `rightdelim` are never units.
- Go template actions `{{ ... }}` (or the table's own `leftdelim`/`rightdelim`) are standalone inline tags (LocOS `x`
  tokens); the text between them is the unit. Values with no letter or digit outside actions (`---`) are skipped
  (locked units with `include_locked`).
- Reading: an own validating scanner (no TOML library) covering the whole TOML 1.0 grammar and the table rules
  (duplicate keys, redefined tables, arrays of tables, inline tables are closed). It agrees with the `toml` crate on
  tens of thousands of random and mutated inputs (`tests/hostile.rs`). TOML 1.1 syntax is refused, like LocOS does.
- Only UTF-8 (a BOM is kept). UTF-16, Latin-1, Shift-JIS and so on give `Error::Encoding` (code `encoding`), with
  LocOS's sentences.
- Limits: `ExtractOptions::limits` (LocOS's numbers by default: 50,000 units, 100,000 characters per unit, 20 lines
  per unit) plus LocOS's pre-parse budget (quotes over 40 x the unit cap, `= , [ {` over 20 x the unit cap); the
  crate's own hard caps stay (256 MB file, nesting 64, 2 million strings, 8 million values). Every refusal is
  `Error::Limit` (code `limit`).
- TOML has no language field, so a target locale changes nothing but the values.

Tests: `tests/locos_toml.rs` (the 10 LocOS cases), `roundtrip.rs` (corpus, 11 languages, encodings), `hostile.rs`
(proptest and the differential check), `speed.rs`. Tools: `tools/compare.py` (units and translated bytes against
LocOS), `tools/bench_locos.py`. Corpus: `../oracle/corpus/toml/`.

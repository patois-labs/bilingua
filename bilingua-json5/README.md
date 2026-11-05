# bilingua-json5

JSON5 and JSONC filter for Bilingua (`.json5`, `.jsonc`; format id `json5`).

- One scanner reads JSONC (JSON with comments and trailing commas, as VS Code reads it) and JSON5 1.0.0 (unquoted and
  single-quoted keys, single-quoted strings, `\` line continuations, hex numbers, `Infinity`, `NaN`, a leading `+`).
- Every string **value** with at least one letter or digit is a unit (the Localization OS rule, which is also what a
  plain JSON file gives). Keys, numbers, `true`, `false`, `null` and strings with no letter or digit are never units;
  with `ExtractOptions::include_locked` the non-blank ones are listed as locked units for context.
- `context.key` is the dotted path (`menu.items[0].label`, `list[2]`); the comment block directly above a key or
  array item is `context.notes`. A comment on the same line as the previous token is not attached to the next key.
- Nothing is re-serialised. The skeleton holds every byte between string values: comments, keys, commas, number
  spellings, whitespace, line endings, the BOM. A translated value is written into its own span in its own quote
  style, escaping only what that style needs (JSON escapes; U+2028 and U+2029 always). A value whose text did not
  change keeps its original spelling, escapes and line continuations included. Duplicate names are each a unit.
- Placeholders (`{name}`, `{{user}}`, `${x}`, `%s`, `%1$s`, `%(n)s`) and HTML tags become inline tags, the same set as
  `bilingua-json`; named placeholders are free-order.
- Encodings: UTF-8 (BOM kept), UTF-16 with a BOM (kept), otherwise the Windows code page of the source language
  (Windows-1250 to 1258, 874, Shift_JIS, GBK, Big5, EUC-KR; Windows-1252 when the language has none). The output keeps
  the encoding when every character fits and is UTF-8 when it does not. A file that looks binary, or whose legacy
  bytes do not decode and encode back to the same bytes, is refused (`Error::Unsupported`).
- Limits (`Limits`, `Json5Filter::with_limits`): nesting 256, 500 000 strings, 100 000 characters per string,
  256 MiB per file, plus the Localization OS pre-parse budgets (quotes and separators per allowed string). Anything
  malformed, truncated or over a limit is `Error::Parse` with line and column; nothing panics, nothing recurses.
- Portable: no file system, threads, network or clock. `cargo check --target wasm32-unknown-unknown` passes.

```rust
use bilingua_json5::Json5Filter;
use bilingua_model::*;

let f = Json5Filter::default();
let mut doc = f.extract(&bytes, &ExtractOptions::new().with_source_lang("en"))?;
doc.set_target("u1", vec![Inline::text("Hallo")])?;
let out = f.merge(&doc)?;
```

## Differences from Localization OS

- A UTF-16 file is written back as UTF-16 (LocOS converts it to UTF-8).
- The text-encoding guess for legacy files does not do LocOS's "letters look garbled" check or its warning.
- `\d` in a number is ASCII digits only (LocOS's regex also accepts other Unicode decimal digits).
- Unit selection and whitespace use `bilingua_model::py_isalnum`/`py_isspace` (Python 3.13 `str.isalnum`/`isspace`, tables generated from `unicodedata`): no known difference. Identifier characters follow Python's `\w`.
- Limits default higher than LocOS's 50 000 segments (an engine, not a web upload); configurable.

## Tests

`tests/locos_json5.rs` is the port of `verify_json5_filter.py`; `languages.rs` (11 languages, legacy code pages),
`hostile.rs` (proptest, truncation, depth), `facade.rs`, `speed.rs`. `examples/dump.rs` prints units, keys and
comments for comparing with the LocOS filter.

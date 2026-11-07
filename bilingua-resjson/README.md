# bilingua-resjson

Windows `.resjson` filter for Bilingua (WinJS and Windows Store resources: one JSON object of id to string).

```rust
use bilingua_resjson::Resjson;
use bilingua_model::*;

let mut doc = Resjson.extract(&bytes, &ExtractOptions::default())?;
doc.set_target("u1", vec![Inline::text("Hallo")])?;
let out = Resjson.merge(&doc)?;
```

## What it does

- Every string value with a letter or digit outside its placeholders is a unit. `{0}`, `%s`, `---` stay in the
  file (locked units with `include_locked`). Blank and empty values are never units. Keys are never translated.
- `_<id>.comment` is the note of `<id>` (`context.notes`) and is never translated. `_<id>.maxLength` sets
  `context.max_len`; any other `_<id>.<attr>` member goes to `context.extra[attr]`.
- `//` and `/* */` comments, a trailing comma, key order, spacing, line endings and the BOM are kept byte for byte.
  An unchanged value keeps its original spelling (`\u00e9`, `\/`); a changed one is JSON-escaped once.
  White space around the text of a value stays outside the unit.
- Nested objects and arrays are walked. `context.key` is the JSON pointer; `extra["path"]` and `extra["name"]` are
  the LocOS context (`menu`) and key (`open`; empty for an array element). A repeated key is marked
  `extra["duplicate"] = "true"`.
- UTF-8 (with or without BOM) and UTF-16 with a BOM are read and written back the same way. UTF-32 and other
  encodings are refused (`Error::Encoding`, code `encoding`). Under the LocOS profile a UTF-16 file is
  written back as UTF-16 with its BOM, as LocOS does.
- Placeholders stay plain text (no inline tags): LocOS does not tokenise them in its filter and neither does
  Okapi's JSON filter.
- A value with a lone surrogate escape (`"\ud800"`) cannot be a Rust string: it is left in the file, not a unit.
- Limits (`Limits`, LocOS defaults): 50 000 units, 100 000 characters per unit, 64 levels, 256 MiB; the LocOS
  budgets of lines, quotes and separators relative to the unit limit. Nothing recurses, so deep input cannot
  overflow the stack. `Filter::extract` takes them from `ExtractOptions::limits`; `Resjson.extract_with(bytes,
  opts, &limits)` takes custom ones. Every refusal is `Error::Limit` (code `limit`).
- `sniff` is true only for a file that parses and has a JavaScript comment or an `_id.comment` member, so plain
  JSON stays JSON.

# bilingua-strings

Apple `.strings` filter for Bilingua (iOS and macOS `Localizable.strings`, `InfoPlist.strings`, `genstrings` output).

```rust
use bilingua_strings::Strings;
use bilingua_model::*;

let mut doc = Strings.extract(&bytes, &ExtractOptions::default())?;
doc.set_target("s2", vec![Inline::text("Speichern")])?;
let out = Strings.merge(&doc)?;
```

## What it does

- Reads `"key" = "value";` entries, unquoted keys and values (`plain = word;`), an optional `{ ... }` wrapper,
  `/* */` and `//` comments. Escapes `\" \\ \n \r \t \a \b \f \v \'`, `\Uxxxx` (surrogate pairs across two
  escapes) and up to three octal digits are resolved in the unit text.
- One unit per entry that has a letter or digit outside its placeholders (the LocOS rule). Keys are never
  translated; they are `context.key`. The comments above an entry are its `context.notes`; `context.location`
  is the line. Unit ids are `s<N>` where N counts every entry in the file, so they do not move when a value
  becomes translatable. `include_locked` also lists the entries without translatable text as locked units.
- Format specifiers (`%@`, `%d`, `%ld`, `%.2f`, `%1$@`, `%#@files@`) are standalone tags. Positional ones
  (`%1$@`, `%#@x@`) are free and may move in the translation; the others keep their order. `%%` stays text.
- Encodings: UTF-8, UTF-8 with BOM, UTF-16 LE/BE with BOM, and BOM-less UTF-16 (sniffed from the NUL pattern).
  The merge writes the same encoding, BOM and byte order. Binary plists, UTF-32 and other code pages return
  `Error::Unsupported`.
- Merge writes only the changed values (`\ " newline CR tab` are escaped; leading and trailing white space of the
  value is kept). Everything else, comments, key spelling, line endings (LF/CRLF), the trailing newline and the
  BOM, comes back byte for byte; an untouched or unchanged value keeps its original spelling. A translated
  unquoted word becomes a quoted string.
- `.strings` has no language field (the `xx.lproj` folder names it), so a target language changes nothing in
  the file.
- `records(bytes)` lists every entry (key, value, comment, `unit_index`, `duplicate`) for key-based projects.

## Limits

`Limits::default()` matches Localization OS: 50,000 units, 100,000 characters per value; also 64 MiB of input,
20 lines and 8 quotes per allowed unit. `Strings.extract_with(bytes, opts, &Limits { .. })` sets your own. Over a
limit gives `Error::Unsupported`, malformed text `Error::Parse`. A translation with a NUL character is refused
at merge unless the file is UTF-16 with a BOM.

## Differences from LocOS

None on the units of the test corpus (20 of 20 files give the same texts; 18 of 18 pseudo-translated merges are
byte-identical). Placeholders are tags here, while LocOS keeps them as plain text. A lone `\UD800` surrogate
escape reads as U+FFFD (LocOS keeps the lone surrogate in its Python string, which cannot be written back).

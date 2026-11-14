# bilingua-properties

Java and Mozilla `.properties` filter for Bilingua. It follows `java.util.Properties.load` and matches the
Localization OS filter unit for unit and byte for byte.

```rust
use bilingua_properties::Properties;
use bilingua_model::*;

let mut doc = Properties.extract(&bytes, &ExtractOptions::default())?;
doc.set_target("u1", vec![Inline::text("Willkommen")])?;
let out = Properties.merge(&doc)?;
```

## What it does

- One unit per entry whose value has a letter or digit outside its placeholders. Keys are never translated
  (`context.key`); the comment lines above an entry go to `context.notes`; a repeated key is marked
  `extra["duplicate"]`. Unit ids are `u1`, `u2`, ... in file order.
- Grammar: LF, CR and CRLF line ends; `#` and `!` comments; `=`, `:` or blanks as separator; an odd run of
  backslashes continues the line; `\t \n \r \f \uXXXX` (exactly four hex digits, else a typed error) and `\x`.
  Surrogate pairs are joined; a lone surrogate reads as U+FFFD (the file is not changed unless that value is).
- Encoding like Java's `PropertyResourceBundle`: UTF-8 first (a BOM is kept), ISO-8859-1 when the bytes are not UTF-8
  (never Windows-1252). UTF-16, UTF-32 and binary files return `Error::Encoding` (code `encoding`).
- Writing: an all-ASCII file stays ASCII (everything above U+007E becomes `\uXXXX`, surrogate pairs for emoji); an
  ISO-8859-1 file stays ISO-8859-1 (only characters above U+00FF are escaped); a UTF-8 file with non-ASCII text, and a
  Mozilla-dialect file, are written as literal UTF-8. A leading blank, `=` or `:` in a value is escaped.
- MessageFormat apostrophes: a value with `{n}` arguments whose apostrophes are all doubled shows single ones and
  is doubled again on merge; an apostrophe between two letters in a translation never opens a quote.
- Placeholders become tags: `{0}`, `{1,number}`, `{name}`, `%s`, `%1$d`, Mozilla `%S` and `%1$S`, `${var}` and, in
  the Mozilla dialect, the plural `#1`. Named ones (`{0}`, `%1$s`) may move in the target; `%s` may not. `%%` is text.
- Merge changes only the translated values. Comments, keys, separators, blank lines, line ends, the BOM and the
  final newline come back byte for byte; an unchanged value keeps its original spelling (continuations included).
- `Properties.records(bytes)` gives the key-based view (key, value, comment, unit index, duplicate), as the
  LocOS `records()` does. `include_locked` lists the values left alone (numbers only, `---`, `{0}` only) as
  locked units.
- Limits (`Limits`, `extract_with`): input 256 MiB, 1,000,000 units, 1,000,000 characters in one value, and no more
  than 20 line ends per allowed unit. They return `Error::Limit` (code `limit`, message "input limit: ...").

## Not done

- Okapi's localization directives (`#_skip`, `#_text`, key conditions) and its HTML sub-filter: LocOS does not have
  them, and LocOS wins where the two differ.
- The LocOS "looks garbled" warning for ISO-8859-1 files (it needs the project's source language).
- No language tag lives in a `.properties` file, so a target-locale merge changes nothing but the translated values;
  the caller names the file (`messages_de.properties`).
- Detection is by extension; `sniff` is deliberately conservative.

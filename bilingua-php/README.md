# bilingua-php

PHP array resource files for Bilingua: Laravel `lang/*.php` (`return [...]`), CodeIgniter 4 (same shape) and
CodeIgniter 3 (`$lang['key'] = '...';`, `$lang = array(...)`). Same rules as the Localization OS `php_array_filter`.

```rust
use bilingua_php::Php;
use bilingua_model::*;

let mut doc = Php.extract(&bytes, &ExtractOptions::default())?;
doc.set_target("u3", vec![Inline::text("Hallo")])?;
let out = Php.merge(&doc)?;
```

## What it does

- The file is **tokenised, never executed**: a lexer for single-quoted, double-quoted, heredoc and nowdoc strings,
  numbers, variables, identifiers, `//`, `#` and `/* */` comments, and a parser that accepts only array literals
  (`[...]` and `array(...)`), `key => value` pairs (string, number or auto keys), string values with static
  `'a' . "b"` concatenation, numbers, `true` / `false` / `null`, `declare(strict_types=1);` and the CodeIgniter guard
  `defined('BASEPATH') OR exit('...');`. A closing `?>` may end the file (only white space after it).
- Anything else (a call such as `__()` or `env()`, a constant, a variable or `{$x}` interpolated into a string,
  `namespace` / `use`, inline HTML after `?>`, attributes, a missing closing quote or heredoc label) refuses the
  whole file with `Error::Parse` and a line number. Nothing is half translated. Oversized files give
  `Error::Limit`; unreadable bytes `Error::Encoding`.
- Units: every string value with a letter or digit, in source order. Keys never. A concatenation is one unit. Values
  with no letter or digit (`'...'`, `'---'`, `''`) stay in the skeleton; `ExtractOptions::include_locked` lists the
  non-blank ones as locked units. `context.key` is the dotted path (`nested.deep.title`, `$lang` dropped, list
  items numbered as PHP does), `context.location` the line. Unit ids are `u1`, `u2`, ...
- Placeholders become standalone tags with the LocOS split: Laravel `:name` and positional printf `%1$s` are `free`
  (LocOS `p`, may move); plural conditions (`{0}`, `[1,19]`) at the start of a segment or after `|`, the `|`
  separator itself, `%s`-style printf, `{name}` / `{0, number}` and HTML tags are ordered (`x`). Every tag has the
  label `placeholder` and its exact source text as display, so the adapter can flatten it.
- Merge replaces only the literals that changed: a double-quoted string stays double-quoted (`\\`, `\"`, `\$`, `\n`,
  `\r`, `\t`, `\xNN` escaped); a single-quoted, heredoc or nowdoc string is written single-quoted (`\\` and `\'`
  escaped); a translated concatenation becomes one literal in the first piece's style. Comments, code and every
  untranslated string are byte for byte.
- Encodings (B6): UTF-8 (BOM kept), UTF-16 with a BOM, the Windows code page of `source_lang` (Windows-125x, Shift_JIS,
  GBK, EUC-KR, ...) and Windows-1252. An untouched merge is the original bytes. A changed file keeps its encoding,
  BOM and line ends when the text fits, else it is written as UTF-8. PHP files carry no language, so there is
  nothing to stamp for a target locale and no plural forms to fill; `target_lang` changes nothing.

## Differences from LocOS (all deliberate or neutral)

- A changed file keeps its own encoding and line ends instead of always being written as UTF-8 (B6). A raw line
  break in a translated single-quoted string follows the file's line ends.
- Letter and digit tests use Rust's `char::is_alphanumeric` and `is_whitespace` (Python's `str.isalnum` and
  `\s` differ for some combining marks and non-decimal digits); `\d` in numbers and plural conditions is ASCII only.
- A string made of unknown or paired `⟦..⟧` tokens in a translation cannot occur: Bilingua targets are tags and
  `set_target` refuses an invented one. The LocOS adapter drops them before they get here.
- "Nested too deeply" (more than 32 levels) is `Error::Unsupported`; `bilingua-model` has no depth `LimitKind`.

## Differences from Okapi `okf_phpcontent`

Okapi scans any PHP code for string literals with regular expressions and reads a `|` as text. This filter reads only
static language arrays and refuses code, and treats `|` as the plural separator, as LocOS does. On Okapi's own
resource the text units are the same seven (the resource needs a `<?php` line first); see `tests/okapi.rs`.

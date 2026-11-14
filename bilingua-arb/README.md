# bilingua-arb

Flutter ARB (Application Resource Bundle) filter for Bilingua (`.arb`).

An ARB file is a JSON object: plain keys hold ICU MessageFormat messages, `@key` objects describe the message
before them, `@@` keys are file-wide (`@@locale`). This crate follows Localization OS's ARB dialect.

## What becomes a unit

- Every top-level plain key (no leading `@`) with a string value and some text is a message. `context.key` is the key.
  Keys, `@key` and `@@` entries, numbers, lists and nested objects are never units; they stay in the skeleton.
- A message with `plural`, `selectordinal` or `select` arguments is split like LocOS does it, in file order: one
  unit for the text around the argument (if it has words), then one unit per branch, recursively. A branch unit
  has `context.extra["icu"]`, for example `count: plural one` (nested: `a: select x / b: plural one`).
- `{name}` and typed arguments (`{n, number}`, `{when, date, yMMMd}`) are **free** standalone tags (they name
  their argument, a translation may move them). `#` in a plural branch and a complex argument in the outer text are
  standalone tags that do not move (LocOS tokens `x`). Quoted text (`'{'`, `''`) stays raw text, as in LocOS.
- `@key.description` and `@key.context` become `context.notes`, followed by one note per described placeholder
  (`{name}: String, e.g. Bob`). Every `{name}` tag has the label `placeholder` and its exact source text as display,
  so the LocOS adapter can flatten it back to the text LocOS keeps.
- A message the ICU parser rejects (unbalanced braces, nesting over 40) is one opaque unit; `{name}` in it is still a tag.
- A message with no letter or digit is left alone. Duplicate keys give a unit each, like LocOS. A string with a lone
  surrogate escape (`\ud800`) cannot be held in Rust and is left in the skeleton.

## Merge

- Untouched: the original bytes (BOM, CRLF, indentation, escapes, key order).
- Only a changed message is rewritten (JSON-escaped, other characters as they are). A translation that drops a
  complex argument, repeats it, uses a tag the source lacks, or breaks the ICU braces falls back to the source text
  of that unit, as LocOS does.
- With `Document::target_lang` set and at least one translated unit: `@@locale` names the target (`pt-BR` becomes
  `pt_BR`; added as the first entry, indented like the first key, when the source has none), every plural and
  selectordinal follows the target language's CLDR categories (a missing category copies the `other` text, an unused
  one is dropped, `=N` branches stay), and suffix-style plurals (`itemCountOne`, `itemCountOther`) gain the target's
  missing categories before `...Other`. The category table is generated from Babel 2.18 (what LocOS uses),
  with Babel's aliases (`deu`, `iw`, `tl`, `und`); an unknown language leaves plurals as they are.

## Limits

`Filter::extract` takes the unit cap and file size from `ExtractOptions::limits` (LocOS's numbers by default) and runs
LocOS's `check_json_budget` before parsing (quotes over 40 x the unit cap, `, [ {` over 20 x). `Limits::default()` (256 MiB,
1,000,000 units, JSON nesting 200) is for `Arb.extract_with(bytes, opts, &limits)`. Every refusal is `Error::Limit`
(code `limit`). Input must be UTF-8 (a BOM is kept); anything else is `Error::Encoding` (code `encoding`), never decoded
with replacement characters.

## Differences from Localization OS

- Bilingua shows `{name}` as a tag; LocOS leaves it as text and protects it later.
- A line break inserted with `@@locale` or a suffix plural follows the file's own line ending (LocOS writes `\n`).
- A non-string `@@locale` value is replaced by the string (LocOS inserts a second `@@locale`).
- When `@@locale` and an added suffix plural land at the same position, `@@locale` comes first.
- A target language with no translated unit changes nothing (LocOS rebuilds anyway; with nothing translated that
  only differs by the locale and plural fill).

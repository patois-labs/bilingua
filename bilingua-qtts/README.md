# bilingua-qtts

Qt Linguist `.ts` filter for Bilingua (the translation source files `lupdate` writes and `lrelease` reads).

```rust
use bilingua_qtts::QtTs;
use bilingua_model::*;

let opts = ExtractOptions::new().with_target_lang("de-DE");
let mut doc = QtTs.extract(&bytes, &opts)?;
doc.set_target("u1", vec![Inline::text("Datei &öffnen...")])?;
let out = QtTs.merge(&doc)?;
```

## What it does

- Format id `ts`, extension `ts`. Every `<message>` of a top-level `<context>` whose `<source>` has a letter or a digit
  and whose `<translation>` is not `vanished` or `obsolete` is a unit. The unit is the **source** text; existing
  translations are not read (the filter is monolingual, as in LocOS: a merge replaces the translation).
- A message with no `<translation>` element is skipped (LocOS; Okapi lists it). A `<context>` inside a `<context>`
  is not read (LocOS; Okapi reads it). Both are listed under `include_locked` as locked units (`k<n>` ids) when
  they have a source with text.
- Everything inside `<source>` that is not text (`<byte value="x9"/>`, a comment, a processing instruction) is one
  standalone inline tag (`x` in the LocOS text); its original markup is written back as it was. `%1`, `%n` and
  `&amp;` accelerators stay plain text.
- Notes: `<comment>` and `<extracomment>` (`context.notes`). `context.key` is the message `id` or the source text,
  `context.location` the `<location>` as `file:line`, `context.extra["context"]` the context name,
  `["plural"] = "numerus"` for numerus messages, `["p_index"]` the LocOS unit index (it counts skipped messages).
- **Merge** replaces the whole `<translation>` element and nothing else: its start tag loses `type="unfinished"`
  (every other attribute stays), the text is escaped once, and the white space around the source text is kept.
  Everything outside the translations (declaration, DOCTYPE, comments, attribute quotes, empty-tag style, line ends,
  `<location>`, `<oldsource>`, `<userdata>`, unknown elements) is the original bytes. New text uses CRLF when the
  whole file does.
- **Numerus** messages: every existing `<numerusform>` gets the translation, with its layout kept; with no forms,
  one form per plural form of the target language (CLDR categories that whole numbers reach, from Babel 2.18, as
  LocOS: Polish 3, Russian 3, Arabic 6, Japanese 1, unknown language 2). With a target language that has more forms
  than the file, the missing ones are added after the last form, each holding the translation.
- **Target language** (`doc.target_lang`, from `ExtractOptions::target_lang`): `<TS language="..">` is set to the
  target as Qt spells it (`de-DE` becomes `de_DE`), keeping the quote style, or added when missing. An unusable tag
  (`*`, `a b`) leaves the attribute alone. As in the ARB crate, a merge with **nothing translated** changes nothing,
  so an untouched merge is always the original bytes even when a target language is set.
- Encodings: UTF-8 (BOM kept), UTF-16 with a BOM, and the legacy charsets a file declares (`encoding="windows-1252"`,
  `Shift_JIS`, `GBK`, ...), written back in the same encoding. LocOS converts to UTF-8 on export; Bilingua keeps the
  file's encoding. A character the code page cannot hold, and every non-ASCII character in a file declared
  `us-ascii`, is written as a character reference.
- Safety: `guard::check_xml` first (entity declarations, depth 256, 4 million elements), then a strict well-formedness
  scan of its own: one root, balanced tags, unique attributes, only the five predefined entities and character
  references, declared namespace prefixes, XML characters only, and **no DTD internal subset** (`<!DOCTYPE TS>` is
  fine). Nothing recurses. Limits (`Limits`, LocOS defaults): 50 000 units, 100 000 characters per unit, the LocOS
  markup budget (40 `<` per allowed unit). `QtTs.extract_with(bytes, opts, &limits)` takes custom limits.
- `sniff` is strict: XML whose first element, after the declaration, comments and DOCTYPE, is `<TS`. TypeScript
  never starts that way. See `WIRING.md` for how the facade decides `.ts`.

## Known differences from LocOS

- Existing encoding kept (LocOS always writes UTF-8 and rewrites the declaration). Untouched merge is exact bytes.
- A line break inside a translation is written as CRLF when the whole file uses CRLF (LocOS writes LF).
- Rust's `char::is_alphanumeric` is a little wider than Python's `str.isalnum` for the "has a letter or digit"
  test (combining marks of some scripts). Shared helper pending.
- Okapi: see the report; LocOS wins where they disagree.

## Tests

`tests/locos_ts.rs` (the LocOS Qt cases), `tests/formats.rs` (round trips, 11 languages and encodings, target
merges, LocOS golden file, Okapi resources), `tests/hostile.rs` (malformed input, limits, proptest), `tests/speed.rs`
(over 5 MB, timed; `tests/tools/speed_locos.py` times LocOS on the same bytes). Fixtures are made by
`tests/tools/gen_fixtures.py`; the golden files by `gen_golden.py` (LocOS, read only) and `gen_okapi.py` (Okapi
oracle). `cargo run -p bilingua-qtts --example qtts_dump` and `--example qtts_pseudo`
are the tools behind the differential check.

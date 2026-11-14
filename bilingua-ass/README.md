# bilingua-ass

Advanced SubStation Alpha (`.ass`) and SubStation Alpha v4 (`.ssa`) subtitle filter for Bilingua. Port of the
Localization OS ASS filter (the baseline for this format; Okapi has no ASS filter).

```rust
use bilingua_ass::AssFilter;
use bilingua_model::*;

let mut doc = AssFilter.extract(&bytes, &ExtractOptions::default())?;
doc.set_target("l13", vec![Inline::text("Bonjour")])?;
let out = AssFilter.merge(&doc)?;
```

## What it does

- Units: the `Text` field of every `Dialogue:` line in `[Events]`, and the `Title:` header of `[Script Info]`
  (`<untitled>` and blank titles are skipped). `Text` is the field the section's `Format:` line names last, so the line
  is split on the first `fields - 1` commas only and the text may hold commas. A `Format:` whose last name is not
  `Text`, a line with too few fields, `Comment:` lines, lowercase `dialogue:` and `Title:` outside `[Script Info]` are
  left alone.
- Ids: `l<line number>` (stable for the same file). `context.kind` is `subtitle` (or `title`); `extra` carries
  `start`, `end`, `style`, `name`, `layer`, `effect` from the Format columns and `max_cpl = 42`.
- Inline codes: override blocks `{...}` and the escapes `\N` (hard break), `\n` (soft break), `\h` (hard space) are
  codes. Leading and trailing codes and white space stay in the skeleton. A style switch turned on and later off in
  the same line (`{\i1}..{\i0}`, `{\b1}..{\b0}`, `{\u1}`, `{\s1}`, `{\b700}`, anything closed by `{\r}`) is a paired code
  (`Open` ... `Close`) that moves with its words; a pair with one end inside the text pulls the other in. Writing is
  balanced: a close with no open is dropped, an open left unclosed is closed at the end, a pair a translation emptied is
  dropped. A line in drawing mode (`\p1` and up) is never a unit.
- Safe text: a translation can never make a cue, a header, an override or a control code. A typed line break becomes
  `\N`; a typed `{` is written as the full-width `U+FF5B`; a backslash that would make `\N`, `\n`, `\h`, `\{` or `\}` is
  written as the full-width `U+FF3C`. A `Title` is one line (a break becomes a space).
- Untouched units (no target, or a target equal to the source) are written as the original bytes; so a merge with
  nothing translated returns the file exactly (BOM, UTF-16, code page, CR / LF / CRLF per line, no final newline).
- Language: when something is translated and `target_lang` is set (or `stamp_target_always`), the `[Script Info]`
  `Language:` header (the libass shaping hint) is set to the two-letter target language, added after the section header
  when missing, dropped when the target has no two-letter code, untouched when the target is no language tag.
- Encodings: UTF-8 (BOM kept), UTF-16 with BOM, the code page of the project's source language, Windows-1252 default;
  merge keeps the file's encoding (or writes UTF-8 with `MergeOptions::locos()`).

## Differences from LocOS (all intended)

- Default merge keeps the file's encoding; LocOS always writes UTF-8 (`MergeOptions::locos()` does too). With that
  option a UTF-16 upload comes back as UTF-8 without a BOM (LocOS adds one).
- A `[Script Info]` header on the last line with no line end: LocOS glues `Language:` onto it, Bilingua adds the line end.
- A Title translation is always one line; LocOS keeps a typed `\N` when the header is spelled with a Turkish dotted or
  dotless i.
- Text typed into a translation that looks like markup is made safe (see above); LocOS writes it as it is.
- A literal `⟦x9⟧`-looking text is plain text here and is written; LocOS drops markers that name no code.

## Tests

`tests/locos_suite.rs` (the ten LocOS cases), `tests/locos_golden.rs` (293 files against recorded LocOS output made by
`tests/tools/gen_golden.py`), `tests/roundtrip.rs` (corpus, 11 languages, code pages, markup safety, ids, stamping),
`tests/hostile.rs` (proptest + deep and huge inputs), `tests/speed.rs` with `tests/tools/speed_locos.py`
(`BILINGUA_ASS_SPEED_OUT=big.ass cargo test -p bilingua-ass --release --test speed -- --nocapture`).

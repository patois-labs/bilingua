# bilingua-wordfast

Wordfast Pro **TXML** (`.txml`, also GlobalLink TransStudio) and **TXLF** (`.txlf`, XLIFF 1.2 with the gs4tr
extension) for Bilingua. It matches the Localization OS filter (`app/filters/wordfast.py`, with the XLIFF engine
`cat_xliff.py` for TXLF) unit for unit, token for token and byte for byte on write.

```rust
use bilingua_model::*;
use bilingua_wordfast::Txml;

let mut doc = Txml::default().extract(&bytes, &ExtractOptions::default())?;
doc.set_target("0", vec![Inline::text("Willkommen")])?;
let out = Txml::default().merge(&doc)?;
```

## What it reads

- **TXML**: one unit per `<segment>` of a `<translatable>` block that has a `<source>`. Unit ids are LocOS's
  `p_index` as text (`"0"`, `"1"`, ...: the position among all such segments, protected ones counted), so they never
  move when a protected segment changes. `ut` elements are inline codes: a `type="start"` / `type="end"` pair with the
  same `x` is a paired tag, every other element a standalone one. Each carries its own bytes. `<ws>`, `<comments>`
  (units get them as notes), `<revisions>` and `<skeleton>` are never text.
- **TXLF**: one unit per `<source>` of a `trans-unit`; a trans-unit with `<seg-source>` gives one unit per
  `<mrk mtype="seg">` (ids `1000000000 + n`, as LocOS). `g` / `mrk` are paired tags, `bpt` / `ept` / `it` and the other
  point elements standalone tags whose order matters, and an `<x id>` / `<ph id>` placeholder that does not stand for
  one side of a formatting pair is a *free* tag (`{0}`, `%1$s`, an image).
- Protected, never overwritten (LocOS rules): TXML `locked="true"` on the segment or its block, `<localizable>`
  blocks and segments that already carry somebody's confirmed work (a target that is not raw machine translation
  `gtmt="true"` and not a fuzzy match below 100); TXLF `translate="no"`, `approved="yes"`, `final` / `signed-off`,
  `gs4tr:locked`, a gs4tr target past `needs-*`, and the memoQ, Phrase and WorldServer status attributes.
- **Existing translations are loaded as targets** (Brent, 11-05). A fuzzy or machine target on a segment you can
  translate is a draft target with its origin (`Tm` with the match score, or `Mt`). A segment somebody already
  confirmed (LocOS never touches it) is listed as an ordinary unit with its translation as the target and state
  `Translated`, as Okapi does; changing that target rewrites it, leaving it alone does not. Reviewer sign-off
  (TXLF `final`, `signed-off`, `approved`) and locks are read-only (`translate = false`, `lock_reason`).
  `Txml { load_targets: false }` (also `Txml::without_targets()`, or the format option `wordfast.load_targets` =
  `false` in `ExtractOptions::format_options`, which wins over the struct) leaves the confirmed segments out: that is
  exactly the list LocOS translates, and what the LocOS adapter uses. `include_locked` in the extract options lists
  locked and localizable segments too, read-only.
- A target's codes are matched to the source's by element and `x` / `id`; one with no partner is dropped (its text
  stays).
- Notes of a `seg-source` trans-unit go to each of its segments; when that would copy more than 10 000 notes (a
  crafted file) only the first listed segment carries them.

## What it writes

A merge splices only the `<target>` content of translated units; everything else (declaration, BOM, line ends,
attribute quoting, `<ws>`, comments, revisions, segment attributes such as `score`) is the original bytes.

- No translation, or a target equal to the one that was loaded: the unit's bytes are left alone. A merge in which
  nothing was written returns the original file byte for byte.
- A missing `<target>` is created after the source and its trailing `<ws>` (TXML) or after `<seg-source>` / `<source>`
  (TXLF, with the source's own namespace prefix). `<target/>` is opened. For `seg-source` a target mirroring the
  segments is created once, with the translated segments filled and the others empty.
- TXLF: a written target whose `state` is `new`, `needs-translation` (or any `needs-*` in a gs4tr file) becomes
  `translated`.
- Text is escaped (`&`, `<`, `]]>`; a bare `>` stays), XML-illegal characters are dropped, pasted Word breaks
  become newlines. Codes are written from the source's own bytes; an invented, repeated or stray code is dropped
  and anything left open is closed, so the result is always well-formed.
- White space around the source text comes back around the translation.
- **Language**: when at least one unit was written and `target_lang` is set, a `<txml>` with no `targetlocale` gets
  one (in the file's own `en_US` style when its `locale` is written that way), and each TXLF `<file>` with no
  `target-language` gets one. An existing value is never changed. Nothing written: no stamp, original bytes.
- `merge_with` takes the model's `MergeOptions`: `stamp_target_always` declares the target language even when
  nothing was translated (LocOS `build_localized`); `output_encoding: Utf8` writes UTF-8 and rewrites the XML
  declaration of a UTF-16 file to say so (LocOS itself keeps a Wordfast file's UTF-16, so the adapter can keep the
  default for these two formats).
- Every merge that writes something is parsed again before it is returned (as LocOS does); a document whose hidden
  codes were tampered with gives a write error, never a broken file. Target tags naming a hidden code (`~...`) are
  ignored.
- Encodings: UTF-8 (BOM kept), UTF-16 LE / BE with a BOM, CRLF kept. Like LocOS, a declared legacy encoding
  (`ISO-8859-1`, ...) or bytes that are not UTF-8 are refused with a "re-save as UTF-8" error.

## Differences from LocOS (written down)

- A comment inside a TXLF `<source>` is dropped with its text kept around it; LocOS turns it into an empty `g` pair
  that carries the comment text (a LocOS quirk, not copied).
- A TXLF file's SDL `seg-defs` locks and memoQ / Phrase *write* marks (`mq:status`, `sdl:seg conf`) belong to those
  dialects; this crate applies the read-side protection attributes only.
- Not-well-formed input is refused by a hand-written checker, not libxml2, so a handful of edge messages differ.
- The guards are Bilingua's (B6) and apply to the whole file; LocOS's per-segment caps map to `Limits`.

## Tests

`tests/locos_wordfast.rs` is the LocOS suite (10 cases), `tests/behaviour.rs` loaded targets, `seg-source`,
inline edge cases and Okapi's `okf_txml` test resources, `tests/formats.rs` every fixture (round trips, 11
languages, encodings, language stamps), `tests/hostile.rs` malformed, truncated, deep and huge input plus proptest
over random bytes and mutated fixtures, `tests/speed.rs` two generated 5 MB files. `examples/wordfast_dump.rs` and
`examples/wordfast_bench.rs` feed the oracles and the timing.

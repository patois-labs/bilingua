# bilingua-stl

EBU STL subtitle (`.stl`, Tech 3264-E) filter for Bilingua. Spec and baseline: the Localization OS filter
`app/filters/stl_filter.py` and its suite `tests/verify_stl_filter.py`.

```rust
use bilingua_stl::Stl;
use bilingua_model::*;

let mut doc = Stl.extract(&bytes, &ExtractOptions { target_lang: Some("fr-FR".into()), ..Default::default() })?;
doc.set_target("c1", vec![Inline::text("Bonjour,\ncomment ça va ?")])?;
let out = Stl.merge(&doc)?;
```

## What it does

- One 1024-byte GSI block and 128-byte TTI blocks. The DFC must read `STL25.01` or `STL30.01`; the CCT (`00` to `04`,
  blank = `00`) picks the code table: Latin (ISO 6937 of Tech 3264 Appendix 2, floating diacritics `C1h` to `CFh`
  before the letter), 8859-5 Cyrillic, -6 Arabic, -7 Greek, -8 Hebrew.
- One unit per subtitle (`c1`, `c2`, ...): its extension blocks joined (comment blocks, user data and reserved EBN
  values are skeleton), rows joined with `\n`. `context.key` is the subtitle number, `extra["start"]` and `["end"]`
  the timecodes, `extra["max_cpl"]` the GSI MNC. Leading and trailing control codes and spaces of each row stay with
  the row in the skeleton; a control code inside a row is a standalone, non-free tag (`x` in LocOS), shown as `<01>`
  with a label such as "colour red"; as in LocOS it takes a space cell, so a space is added before it in the text.
- Everything else (timecodes, VP, JC, CF, GSI, untouched blocks) is written back byte for byte.

## Merge

- Nothing translated: the original bytes. With `target_lang` and something translated the LC byte (Appendix 3) gets the
  target language (`00` when unlisted); `MergeOptions::stamp_target_always` stamps it even when nothing changed.
- A changed subtitle keeps its rows when they fit in MNC characters, otherwise it is re-wrapped, in at most as many
  rows as it had (two at least); the vertical position moves with a changed number of double-height rows; a subtitle
  that needs more bytes gets extension blocks and TNB is updated.
- A translation the declared table cannot encode switches the CCT to the first table that holds every changed
  subtitle and every untouched one (Russian into a Latin file). Quotes, dashes, ellipsis and thin spaces are folded to
  ASCII only when the table lacks them. Text that still does not fit, or characters no table holds, refuse the whole
  merge with `Error::Write` naming each subtitle (the LocOS message).
- A translation is text: it can never write a control byte (a character that is not in the table is refused), cannot
  start a new subtitle (blocks of one subtitle share its number), and cannot touch the header.

## Differences from LocOS

- A tag with an id the original does not have is dropped, as LocOS drops an unknown token. Literal `⟦x1⟧` text in a
  translation is plain (unencodable) text here, not a code.
- Unicode normalisation uses the `unicode-normalization` crate (Unicode 16); LocOS uses Python's (15.1).
- Row, unit and block caps come from `Limits`; more than 99 999 blocks is `Error::Limit`.
- Linear where LocOS is quadratic (gate 11-05): the list of refused subtitles, the unsupported-character check
  (100 000 distinct characters) and the row wrap (zero-width codes such as italics in long words). Same output.
- Since LocOS issue 798 (fixed in both): a table switch re-encodes comment subtitles and refuses bytes that would read as other
  letters; a floating diacritic on a letter with no precomposed form (`C1 78`, x with grave) is a base and mark pair
  that takes one cell and is written as its two bytes, in the locos profile too.
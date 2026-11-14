# bilingua-rst

reStructuredText filter for Bilingua (`.rst`, `.rest`). It follows the Localization OS filter (`rst_filter.py`)
unit for unit: a line-grammar walk over the source, no docutils, nothing executed or followed.

## What becomes a unit

| Source | `context.kind` | Notes |
|---|---|---|
| paragraph, block quote, definition-list definition, footnote and citation body, list-item body | `paragraph` | lines are joined; a trailing `::` stays outside and the literal block after it is skipped |
| definition-list term, line-block line, directive title (`topic`, `sidebar`, `rubric`, `admonition`, `list-table`, ...) | `text` | |
| section title | `title` | underline and overline are resized to the translated title (CJK counts 2 columns, combining marks 0) |
| admonition, `versionadded` text, container and domain-directive bodies | `paragraph` | the version number and the signature are not translated |
| grid and simple table cell | `cell` | a cell is re-wrapped inside its column |
| `.. meta::` `:description:` and `:keywords:` values | `paragraph`, `style` `meta:description` / `meta:keywords` | appended after every body unit; written back on one line |

Kept byte for byte: literal blocks (indented and quoted), `code-block`, `code`, `sourcecode`, `literalinclude`, `raw`,
`math`, `csv-table` and every unknown directive, comments, hyperlink targets, substitution definitions, directive
options, field lists, option lists, doctest blocks, transitions, `include` (never followed).

## Inline codes

Each code is an `Inline::Tag`; the markup is in the unit's `codes` (hidden codes start with `~`).

| Markup | Tag |
|---|---|
| `*emphasis*`, `**strong**`, `` `text <url>`_ `` | paired (`Open`/`Close`); the words are translated, the markup is not |
| inline literals, roles, interpreted text, `\|substitutions\|`, footnote and citation references, `word_` and `` `text`_ `` references, inline targets, bare URLs, backslash escapes | standalone, order kept (LocOS `x`) |

Labels: `strong`, `emphasis`, `link` (pairs); `literal`, `role`, `escape`, `substitution`, `footnote-ref`, `url`,
`reference` (standalone). None is a flattenable placeholder: LocOS tokenises all of them.

On merge a unit is written on one line. Markup characters in the translated words are escaped (`\*`, `` \` ``, `\|`,
`\\`, and `_` after a word), a paragraph that would start another block gets a leading backslash, and markup glued to a
letter (Japanese, Chinese) gets an escaped space (`\ `) so docutils still reads it (#695). A pair the translation
leaves open is closed; a lone close is dropped; unknown tokens are dropped.

## Tables

Column widths never change. A translated cell is re-wrapped inside its column; a grid row grows by a line when its last
line is a plain content line, a simple-table row grows by continuation lines. A translation that cannot fit (a word wider
than the column, a first simple-table column that needs a second line, a row that shares its last line with a spanning
cell) makes `merge` return `Error::Write` with a message naming the table line and the cell. Tables the walk cannot map
(malformed grids, text across simple-table column gaps, grids over 400,000 cells) are kept as written and reported as
warnings of kind `rst_skipped_table`.

## Encoding and limits

The shared text decode: UTF-8 (BOM kept), UTF-16 with a BOM, then the code page of the source language. The file keeps
its encoding, BOM and line endings on merge. reStructuredText declares no document language, so `target_lang` changes
nothing. Limits: `Limits` from `bilingua-model` (units counted per walked slot, as LocOS), plus the walk's own caps
(40 levels of nesting, 1,000 inline codes per unit, 4 million grid probes).

## Tests

`tests/locos_suite.rs` is the LocOS `verify_rst_filter` suite, ported (9 of 9). `tests/locos_golden.rs` compares 2,409
generated and mutated files (9,864 units, 19,272 builds) with what the LocOS filter makes of them; regenerate the
golden with `tests/tools/gen_golden.py` and the Unicode tables with `tests/tools/gen_tables.py` (both run on the LocOS
venv, read only). Also `roundtrip.rs`, `hostile.rs` (proptest), `speed.rs` (+ `tests/tools/speed_locos.py`).

Differences from LocOS on purpose (see MIGRATION.md): a literal block under a list item stays literal (LocOS
offered its lines as text), and a literal that ends in a backtick (``` ``:role:`x``` ```) ends at the run of three
backticks. The golden test skips exactly those cases (`LITERAL_UNDER_ITEM`, `LITERAL_BACKTICK`).

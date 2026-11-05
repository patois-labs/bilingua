# bilingua-xtg

QuarkXPress Tags (XPress Tags tagged text, `.xtg` and `.tag`) for Bilingua. A port of the LocOS filter
(`app/filters/quark_tags_filter.py`), checked against it with a golden file of LocOS output.

- **Header and encoding.** `<vNN.NN><eN>` is kept byte for byte. `<eN>` picks the code page: 0 Mac Roman,
  1 Windows-1252, 2 ISO-8859-1, 3 Japanese Windows (cp932), 6 Big5, 7 GBK, 8 UTF-16, 9 UTF-8, 19 Korean (cp949).
  A BOM or a BOM-less UTF-16 start wins over the tag. `<e20>`, `<e21>` and unknown numbers are refused. A file
  with no `<e>` goes through the shared text decode (UTF-8, else Windows-1252). The file is written back in its own
  encoding; a translation the code page cannot hold switches the whole file to UTF-8 and the header to `<e9>`
  (no header: a UTF-8 BOM). The Python code pages (Mac Roman, cp1252, cp932, Big5, GBK, cp949) are reproduced from
  tables generated from Python's codecs (`tests/tools/gen_tables.py`), including what Python refuses.
- **Units.** One unit per paragraph line. `@Name=` and `~Name=<` definitions are not text; `@Name:`, `@$:` (Normal)
  and `@:` are the unit's style (`context.style`) and carry over to later paragraphs.
- **Codes.** Tags become standalone codes (`x`), consecutive tags share one. Character attributes switched on and
  reset in the same paragraph (`<B>..<B$>`, `<I>..<P>`, `<f"Arial">..<f$>`, `<@Emph>..<@$>`, `..<$>`) are
  Open/Close pairs (`g`). `<\@> <\<> <\\> <\>>` are text. Leading and trailing tags stay outside the unit.
- **Merge.** Only changed paragraphs are rewritten; an untouched merge is the original bytes. Text is re-escaped,
  a line break becomes `<\n>`, a pair a translation does not write whole loses its formatting and keeps its
  words, dropped standalone codes are appended, `~Name=<` is never produced at the start of a paragraph.
- **Skeleton `encoding`.** `utf-8`, `utf-8-bom`, `utf-16le`/`utf-16be` (with BOM), `utf-16le-nobom`,
  `utf-16be-nobom`, `mac_roman`, `windows-1252`, `iso-8859-1`, `shift_jis`, `big5`, `gbk`, `euc-kr`, or an
  `encoding_rs` name (a Windows code page chosen by the project's source language, for a file with no `<e>`).
- **Hidden codes on a unit** (ids start with `~`): `~raw` (the paragraph as written), `~r<N>` (the reset tags of
  pair N), `~groups` (closes written together), `~all` (pairs whose close was a reset-all `<$>`).

Tests: `cargo test -p bilingua-xtg`. The golden (`tests/data/locos_golden.json`, `codec_golden.json`) is made with
`tests/tools/gen_golden.py` and `gen_codec_golden.py` (LocOS venv Python, read only).

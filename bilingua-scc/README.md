# bilingua-scc

Scenarist Closed Captions (SCC, CEA-608 line 21 captions as hex byte pairs) filter for Bilingua. Port of the
Localization OS SCC filter (`app/filters/scc_filter.py`), which is the baseline for this format.

- One unit per caption line (pop-on, roll-up and paint-on alike), rows joined with a line break. Mid-row codes
  (colour, italics, underline), tab offsets and unknown inline codes are standalone, non-free inline tags (LocOS
  `⟦xN⟧`, kind `x`). Timecodes, mode, position and every other code are skeleton.
- Special and extended characters are decoded to Unicode; the redundancy rule (a doubled control code is one code)
  is applied; parity is not checked on read (as LocOS).
- An untouched caption is written back byte for byte. A changed caption is re-encoded: every character must exist
  in the 608 basic, special or extended set (typographic quotes, dashes and ellipses are mapped first), rows are at
  most 32 cells, the translator's own breaks are kept when they fit, otherwise the text is re-wrapped (balanced) into
  at most 4 rows (pop-on, paint-on) or the roll-up depth, rows move up or left to fit. If any caption cannot be
  written the merge is refused with the list of captions (timecode and reason), never a broken caption.
- Kept verbatim and not units: lines with text on data channel 2, lines that backspace or erase in the middle of
  text, several captions on one line, lines with no text.
- A translation is text only: its characters can never become a caption, a control code, a timecode or a header
  (the token characters `⟦ ⟧` are not 608 characters, so a literal token in text is refused).
- Encodings: UTF-8 (BOM kept), UTF-16 with BOM, Windows-1252 before the header. Line endings are kept per line.
  There is no language field in SCC, so nothing is stamped.
- Tests: `tests/locos_suite.rs` (the LocOS suite: 8 test functions, ported 8 of 8), `tests/locos_golden.rs`
  (157 files against recorded LocOS output for ten mappings, bytes and refusal texts identical, made by
  `tests/tools/gen_golden.py`), round trips in eleven languages, hostile input (proptest), speed
  (`cargo test -p bilingua-scc --release --test speed -- --nocapture`, then `tests/tools/speed_locos.py`).

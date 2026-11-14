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

## Differences from LocOS

Everything else is LocOS byte parity (units, written bytes and refusal texts; `tests/locos_golden.rs`).

- **Channel 2 on read-back (lead decision 11-05, the rc precedent).** The data channel carries from line to line,
  so a caption that starts with a character before any channel-1 code is read on the channel the line before it
  ended on. A translation that moves or drops an inline code that switches to channel 2 can put its own caption,
  or the next one, on channel 2. LocOS writes that file and the caption vanishes on the next read. Bilingua re-reads
  the merged file whenever a caption was re-encoded and refuses the merge (`Error::Write`, naming each caption and
  timecode) when a caption would not read back as a channel-1 caption. Test:
  `a_caption_that_would_read_back_on_channel_2_is_refused` in `tests/roundtrip.rs`.
- A literal `⟦xN⟧` typed as text is refused (the token characters are not 608 characters); LocOS treats it as the
  token, or drops it when the line has no such code. Inline codes travel as tags here.
- An empty target means "not translated" (the line is kept); LocOS refuses an empty translation as "does not fit".
- The unsupported-character scan stops at the eight characters the refusal shows (LocOS collects all of them in a
  list, which is quadratic in distinct characters). The text is the same.
- A unit whose stored original line holds a line break (a tampered document) is refused.

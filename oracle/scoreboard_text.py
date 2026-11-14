"""Hand-written section copied into SCOREBOARD.md by scoreboard.py."""

TEXT = """## What this means

Written by hand after reading the tables above (wave 12, 2026-11-05); `python run.py --scoreboard` copies it in
unchanged. The reconciliation with every LocOS dialect, connector shape and option, and the classification of each
`differs` row, is `docs/status/BILINGUA-SCOREBOARD-2026-11-05.md`.

- **Coverage is complete for the registry.** Every one of the 87 LocOS registry formats has a Bilingua filter; none is
  `missing`. The dialects LocOS picks by content that the fixtures show (Android strings and Java XML properties in
  `.xml`, Storyline tables in `.docx`, DITA, DocBook, multilingual XML) are `same`, except Mozilla `.properties`.
- **The `differs` rows are mostly the LocOS-profile work of Track D** (placeholders LocOS keeps as text, its extra
  units such as document properties, alt text and comments, one unit per line, entities kept as typed), listed in
  local-tms `docs/reports/bilingua-differences.md`. A few are deliberate Bilingua choices (PNG text chunks and UTF-16
  SVG read, URLs not extracted, SCC re-wrapped to 32 columns) and a few are small Bilingua bugs found by the pseudo
  round trip (Fluent re-indent, ICML hidden range, DXF trailing backslash).
- **Bilingual formats compare differently.** LocOS leaves finished segments out of its unit list; Bilingua lists every
  segment with a state, so finished segments count as locked on the Bilingua side.
- **Pseudo round trip.** Units are wrapped in letters (`Zq...qZ`) and tag markers are ignored; formats that write the
  translation beside the source (MO, desktop, catkeys, CSV and STF target columns, TTX, TXML, TXLF, XLZ, multilingual
  XML) pass when the source reads back unchanged. Letters typed right after a placeholder can still join it (Vue
  `@:key`, Mozilla `#1`), which shows as a round-trip failure that a real translation with a space would not have.
- **Fixture coverage is thin** for some rows, so a `same` is a floor, not proof. The LocOS side of PDF fails its OCR
  fallback in the throwaway data folder (no database), which shows as missing units.
"""

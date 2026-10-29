"""Hand-written section copied into SCOREBOARD.md by scoreboard.py."""

TEXT = """## What this means

Written by hand after reading the tables above; `python run.py --scoreboard` copies it in unchanged.

- **Coverage is the real gap, not quality.** Of 87 LocOS formats Bilingua opens 15 (10 source parsers plus the
  XLIFF family, SDLXLIFF, MQXLIFF, MXLIFF and the three package types, all now Bilingua crates). 73 have no parser. On the
  formats Bilingua does read, nothing fails: every file opens, an unchanged merge gives the same file, and a
  pseudo-translated merge reads back.
- **No format can switch yet**, but the switch list is short for the formats LocOS fixtures already cover: TXT, SDLXLIFF,
  MQXLIFF and MXLIFF are `same`. The rest of the `differs` rows are mostly the deliberate choices listed in
  `REPORT.md` (more attributes extracted, master placeholders and URLs left out, shared strings once). Before DOCX,
  HTML, JSON, PO, SRT, Markdown, PPTX or XLSX move over, each row needs one pass through its reasons to separate
  those choices from real gaps.
- **Known gaps seen again here** (features, not crashes): PPTX picture alt text, chart text and SmartArt-style shape text
  (the generated and Strict PPTX packages); XLSX text boxes in drawings and `t="str"` cells without a formula; the
  XLIFF 1.2 vendor extension of GlobalLink (`iws:status` finished or locked) that LocOS reads and Bilingua now reads too:
  a finished segment is translated, a locked one is locked.
- **Bilingual formats compare differently.** LocOS leaves finished segments (translated, confirmed) out of its unit
  list; Bilingua lists every segment with a state. The scoreboard marks finished segments as locked on the
  Bilingua side so the two lists line up. The pseudo-translated merge is checked on the target.
- **Unit size.** TXT and Markdown are paragraphs in Bilingua and lines in LocOS: the match column compares words and
  the unit counts are not compared.
- **Nothing here is a verdict on LocOS being right.** Where the two disagree the row says so; the notes in
  `REPORT.md` say which side looks wrong.
- **Fixture coverage is thin** for some rows (one file each for most software resource formats), so a `same` is a
  floor, not proof. Formats with no fixture show a dash.
- Sorting of the missing list is a guess at customer usage, not data.
"""

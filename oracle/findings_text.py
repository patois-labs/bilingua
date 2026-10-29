"""The hand-written part of REPORT.md. run.py copies it in front of the generated tables."""

FINDINGS = """## What the comparison found

Written by hand after reading the tables below; `run.py` copies this section into `REPORT.md`.

### Bilingua bugs, fixed (each has a regression test with a small made-up file)

| Format | Bug | Fix |
|---|---|---|
| XLSX | Cells and rows without an `r` attribute were skipped, so their text was never extracted. The attribute is optional in the file format and some writers leave it out. | A row without `r` follows the previous row, a cell without `r` follows the previous cell. Test: `tests_oracle.rs` in `bilingua-xlsx`. |
| HTML | `<ruby>`, `<rt>`, `<rp>` and `<rb>` were treated as block elements, so a sentence with a ruby annotation was cut into three units (text before, the annotation, text after). | They are inline now. Test: `tests/oracle.rs` in `bilingua-html`. |
| PPTX | The covered cells of a merged table range (`hMerge`, `vMerge`) were extracted although PowerPoint never shows them. | They are skipped. Paragraph numbers still count them, so the ids of other units do not move. Test: `tests_oracle.rs` in `bilingua-pptx`. |

### Deliberate differences (Bilingua's choice; not bugs)

- Strings with no letters ("2026", "$4.99", "42") are not units in any format. The tables leave them out on both
  sides so that this does not show as a difference.
- Unit size: Bilingua makes one unit per paragraph in TXT and Markdown, LocOS one per line. The tables compare the
  words instead of the units for these two formats. Markdown markup (bold marks, backticks, table pipes, HTML tags)
  is removed from the LocOS side first, because Bilingua turns it into tags and does not translate code spans.
- Inline codes: LocOS puts a token around every run of a DOCX, PPTX or XLSX paragraph, Bilingua only around runs
  whose formatting differs from the paragraph's. The `Codes B/L` column shows both counts; they are not meant to match.
- Placeholders and markup: Bilingua turns `%s`, `{name}`, `<b>` in PO, JSON, SRT and VTT, and `<br>`, into tags; LocOS
  leaves them in the text. Pairs with the same words are listed as notes ("markup shown differently").
- Bilingua extracts more, on purpose: DOCX title and subject, HTML `placeholder`, `aria-label`, button `value`,
  `iframe` and `abbr` titles, and attributes written without quotes; XLSX headers and footers.
- Bilingua extracts less, on purpose: JSON values that are URLs; PPTX layout and master placeholders (prompt text such
  as "Click to edit Master title style"); the result text of a field such as a table of contents; HTML `pre`, `svg`,
  `math`, `textarea`; text inside Markdown code spans.
- A shared string in XLSX is one unit even when many cells use it (LocOS lists every cell).
- HTML `button` and `label` stay separate units (UI strings), where LocOS keeps them inside the sentence.
- Entities: Bilingua decodes `&amp;` and `&nbsp;` in HTML and VTT text and writes them back; LocOS shows them raw.

### Gaps (LocOS extracts it, Bilingua does not; listed in the READMEs as unsupported)

These are features, not bugs, so they were not built tonight. Brent decides.

1. PPTX: picture alt text and titles (`descr`, `title` on `p:cNvPr`), and chart text. DOCX picture alt text is the same story.
2. XLSX: text boxes in drawings; a cell typed `t="str"` that has no formula (Excel only writes `str` for formula results).
3. Markdown: visible text inside raw HTML blocks (`<summary>`, `<div>`).
4. HTML: SVG `<title>` and `<text>`, and the default text of a `<textarea>`.

### Where LocOS looks wrong

- SRT: after two blank lines in a row, the next cue number and timestamp were glued onto the cue text.
- HTML: attribute values written without quotes are not extracted.
- Markdown: the units keep the Markdown markup in the text, so a translator sees it and must keep it.

### Round trips

Every file of the corpus: an unchanged merge gives the same file (byte for byte for text formats, every part
identical for DOCX, XLSX and PPTX), a pseudo-translated merge opens again with the same units, and LocOS's own
filters (plus python-docx and openpyxl) can read what Bilingua wrote. `cargo test -p bilingua-oracle` repeats the
first two checks on the committed corpus.
"""

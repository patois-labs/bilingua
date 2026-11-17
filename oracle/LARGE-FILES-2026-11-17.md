# Large files: Bilingua against Okapi and neokapi (11-17)

Bench: `~/bili/oracle3` on the M5 (BILI-113). Same generated files through Bilingua, Okapi Tikal 1.48.0
(Temurin 21 JRE container, `-Xmx16g`) and neokapi kapi (commit 89dcf6a5, v1.4.0-rc5, no plugins, container). Each
step is timed with `/usr/bin/time -v`; RSS is the peak of that step. "Identity" is an extract then a merge with no
targets. The files are made-up text from `genlarge.py` (seeded), sized after the files the lineage comparison
reported as refused.

Bilingua is the `bilingua` CLI. `merge` reads the XLIFF and the skeleton JSON from disk, so its RSS includes parsing
the whole Document from JSON; `pseudo` extracts and merges in one process.

## Before and after BILI-112

Bilingua main (7c16201c) refused 6 of the 7 files (only the pptx passed). Branch `feat/bili-large-files` (890ec255,
`--limits large`, the CLI default) completes all 7: every merge is byte-identical (text files) or keeps every zip
entry identical, every pseudo-translated file is well formed and reads back with the same unit count.

| file | units | extract | identity merge | pseudo (extract + merge) | max RSS |
|---|---|---|---|---|---|
| docx, 62,000 paragraphs (5.4 MB) | 62,000 | 2.0 s / 993 MB | 1.7 s / 1.5 GB | 2.4 s / 421 MB | 1.5 GB |
| docx, 12 MB mixed | 144,138 | 6.4 s / 2.9 GB | 5.3 s / 4.6 GB | 6.7 s / 1.6 GB | 4.6 GB |
| xlsx, 300,000 cells | 267,797 | 6.1 s / 3.4 GB | 4.2 s / 5.3 GB | 5.3 s / 1.4 GB | 5.3 GB |
| pptx, 1,500 slides | 9,774 | 0.4 s / 199 MB | 0.3 s / 302 MB | 0.3 s / 75 MB | 302 MB |
| Markdown, 19 MB | 132,472 | 3.0 s / 1.8 GB | 2.7 s / 3.0 GB | 3.2 s / 611 MB | 3.0 GB |
| HTML, 29 MB | 312,223 | 5.3 s / 2.7 GB | 3.9 s / 4.4 GB | 6.4 s / 1.3 GB | 4.4 GB |
| HTML, flat, 55 MB | 282,784 | 7.8 s / 4.2 GB | 5.6 s / 6.8 GB | 10.0 s / 2.3 GB | 6.8 GB |

The pseudo step on the 12 MB docx took 440 s before 890ec255: `set_state` scanned the document per call.

## The other engines on the same files (baseline run)

| file | Okapi: extract / max RSS | Okapi identity | neokapi: extract / max RSS | neokapi identity |
|---|---|---|---|---|
| docx 62k | 3.3 s / 1.9 GB | document.xml, styles.xml rewritten | 1.1 s / 720 MB | entries identical |
| docx 12 MB | 7.9 s / 5.0 GB | 2 entries rewritten | 4.3 s / 2.0 GB | entries identical |
| xlsx 300k | 3.5 s / 4.3 GB | 5 entries rewritten | 2.9 s / 2.9 GB | entries identical |
| pptx 1,500 | 1.6 s / 1.1 GB | 1,503 entries rewritten | 0.3 s / 192 MB | entries identical |
| md 19 MB | 4.3 s / 3.4 GB | identical | 1.0 s / 1.0 GB | identical |
| html 29 MB | 5.3 s / 1.5 GB | lang and charset rewritten | 1.5 s / 1.5 GB | lang rewritten |
| html 55 MB | 7.0 s / 1.5 GB | lang rewritten | 1.6 s / 2.1 GB | lang rewritten |

neokapi's peak is in merge and pseudo-translate, not extract (12 MB docx: extract 401 MB, merge 1.75 GB).

## What this says

- Bilingua now takes every file, and is the only engine of the three whose identity merge leaves every one of them
  unchanged (neokapi rewrites HTML `lang`; Okapi rewrites Office parts and HTML).
- Memory is the gap (BILI-114): on extract Bilingua uses 2 to 7 times neokapi, and the CLI merge path is the
  worst (4.6 GB on the 12 MB docx), because it parses the whole skeleton JSON. The in-process pseudo pass is
  close to neokapi's merge numbers.
- Speed on big files: behind neokapi (1.5 to 5x on extract), level with or ahead of Okapi. Small files stay
  fastest (about 10 ms, 11 to 14 MB against kapi 20 to 60 ms and Okapi 0.4 to 1.6 s).

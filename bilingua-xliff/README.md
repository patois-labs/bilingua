# bilingua-xliff

XLIFF 1.2 and 2 for Bilingua, with the vendor dialects: SDLXLIFF (Trados), MQXLIFF (memoQ), MXLIFF (Phrase) and
GlobalLink/WorldServer `iws:` metadata. Moved here from `core/catcore`, which now calls this crate.

- `Doc`: the open-file handle. Opening scans byte spans; segment content is parsed on first use; saving **patches
  the original bytes** (target text and state attributes only), so declaration, BOM, namespaces, quoting, line
  endings and vendor markup come back as they were. An unedited file is returned byte for byte.
- `filters()`: Bilingua `Filter`s `xliff` (`xlf`, `xliff`), `sdlxliff`, `mqxliff`, `mxliff`. `extract` gives a
  `Document` with the file's own segments (not sentence-split), targets, states, `origin` (Trados `origin` and
  `percent`, memoQ machine and pre-translation, Phrase `trans-origin` and `score`, XLIFF 1.2 `state-qualifier`),
  notes, lock reasons (`translate="no"`, `sdl:seg locked`, `mq:locked`, `m:locked`, `iws:status lock_status`) and
  the original markup of every code in `Unit::codes`. `merge` writes targets and states back into the original,
  which the skeleton holds whole.
- States: Trados `Translated`, `ApprovedTranslation` (confirmed), `ApprovedSignOff` (signed off),
  `RejectedTranslation` (rejected); memoQ `ManuallyConfirmed`, `Reviewer1Confirmed`, `Proofread` (reviewed),
  `Reviewer2Confirmed` (signed off; the three reviewer names come from memoQ documentation, not a sample file);
  XLIFF 1.2 `translated`, `final`, `signed-off`; XLIFF 2 `translated`, `reviewed`, `final`.
- GlobalLink: `iws:status translation_status="finished"` reads as translated, `lock_status="locked"` as locked; a
  segment accepted in Bilingua sets `translation_status="finished"`.
- Free codes: an `x` or `ph` placeholder is `free` by the Localization OS rule (no `START_`/`CLOSE_` ids, not a
  bold/italic/underline/link `ctype`, and the code it stands for is not markup or an unnumbered `%s`/`{}`).
- Limits: the file must be UTF-8 (a UTF-16 file is refused). A `.sdlxliff` saved before segmentation reads one
  segment per trans-unit. Nothing was opened in the vendor tools: round trips are tested on real files from open
  projects and on made-up Arabic and Japanese files.

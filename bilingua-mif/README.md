# bilingua-mif

Adobe FrameMaker MIF (`.mif`), version 8 and later (Unicode, UTF-8), for Bilingua. A port of the Localization OS
`mif_filter`, tested against it and against Okapi's `okf_mif` resources.

- **Scanner.** MIF is not XML: `<Name args... <Sub ...> >`, `` `strings' `` with `\> \q \Q \ \t \xNN \uXXXX`
  escapes, `#` comments, inset facet lines (`=` or `&` at the start of a line) skipped as opaque data. The scanner
  is iterative (no recursion), allocation-light, and refuses nesting deeper than `max_xml_depth` (256) and more than
  200 statements per allowed unit.
- **Units.** Every `Para` statement with text, in file order (text flows, table cells, footnotes, master pages);
  a paragraph wrapped over several `ParaLine` statements is one unit; the `PgfTag` is `context.style`. `String`
  statements and the printable `Char` names (`Tab`, `HardSpace`, `EmDash`, ...) are text; `Char HardReturn` is a
  newline.
- **Tags.** Variables, cross-references (`XRef` through `XRefEnd`, computed text included), markers, footnote and
  table anchors, conditional text, other `Char` names (`SoftHyphen`, ...) and any other statement are standalone
  tags (consecutive statements share one tag). A font change switched on and restored in the same paragraph (a
  `Font` with the earlier font or the paragraph format's own values) is an `Open`/`Close` pair that moves with its
  words. Tag ids are the LocOS token numbers (one counter), so the model's LocOS conversion gives `x1`, `g2`, `/g2`.
- **Merge.** Only paragraphs whose translation differs are rewritten (as one `ParaLine`, split after each
  `Char HardReturn`); everything else is the original bytes. A pair the translation writes broken or nests
  differently loses its font change and keeps its words. Missing point tags are appended so anchors survive.
  The result is re-scanned. With a target language and at least one translated unit, `PgfLanguage` and `FLanguage`
  are set to the target.
- **Refused:** MIF 7 and older (not Unicode: re-save from FrameMaker 8 or later), book files, non-UTF-8 bytes.
  `include_locked` adds nothing: LocOS lists no locked MIF units.

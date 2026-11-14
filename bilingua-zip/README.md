# bilingua-zip

The safe zip reader and writer that Bilingua's package formats share (DOCX, XLSX, PPTX, ODF, IDML, EPUB, Visio and
the translation packages). Use it through the `bilingua` crate; its own API may change in minor releases.

It refuses what a hostile archive uses: names that escape a folder, a name listed twice (exactly, by case, or by `\`
against `/`), encrypted entries and links, too many entries, too much unpacked data and too high an expansion ratio
(each a typed `LimitError`). The real size and checksum of every entry are checked against the directory. Nothing is
written to disk. When it rebuilds an archive it keeps each entry's compression method and time stamp, so the result
looks like the original.

Licensed under Apache 2.0.

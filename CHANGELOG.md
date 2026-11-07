# Changelog

All notable changes to Bilingua. Every crate shares one version number, so one entry covers the Rust crates, the
`bilingua` command and the Python package. The format follows [Keep a Changelog](https://keepachangelog.com/), and
versions follow [semantic versioning](https://semver.org/) (before 1.0, a minor release may break the API; each break
is listed here).

## [Unreleased]

### Fixed

- html: the preview declares the target language again once something is translated (`lang`, and `dir="rtl"` for a
  right-to-left target), as the merge does. The htmlcore rebuild had dropped it, so an Arabic preview showed left
  to right.
- pdf: U+FFFE is no longer in unit text. PDFium writes it for a hyphen that breaks a word at a line end (the next
  line follows right after it, so dropping it joins the word: `extraordinarily`) and for a glyph its font maps to
  U+0000 (dropped, no hyphen is invented). A hyphen PDFium does not mark stays `-`. C0 control characters (raw glyph
  codes of fonts with no Unicode map) are removed from units, except tab and line breaks, and a page counts only
  printable characters (not C0, U+FFFD or U+FFFE) toward the OCR threshold, so a page of garbage codes now goes to
  OCR. `page_texts` still returns PDFium's text unchanged (#808).
- adoc: a translated cell of a nested table escapes `|` as well as `!`, so it cannot split the outer cell; a cell that
  starts a line and a translation that opens with block syntax or a directive (`ifdef::`, `include::`, `//`) is
  written after `{empty}`, as paragraphs already were (security; LocOS tracker #788).
- rc: a translated character right after a short escape (`\x4` + `1`, `\1` + `2`) would join it into another
  character; the escape is now padded with leading zeros to the width rc.exe reads (`\x04`, `\001`), the same
  character. Octal escapes in `L"..."` strings are read up to 7 digits, as rc.exe does (was 3) (LocOS tracker #791).
- ftl: a multi-line value whose first line is on its own line no longer gains two spaces of indent per merge;
  continuation lines use the indent of the line the value starts on (LocOS tracker #811 item 1).
- scc: a translation that holds an inline code the caption does not have (an unknown tag id) is refused with a message
  naming the code. It used to be dropped without a warning (as LocOS does; LocOS gets the same fix).
- stl: a code table switch (a translation that needs another EBU table) re-encodes comment subtitles with the rest of
  the file, and is refused when a comment cannot move. A byte kept raw (a control code, a byte the old table has no
  character for) only moves when it means the same in the new table: 0xC0 (none in table 00, a letter in table 01)
  makes the switch refuse with a message naming the subtitles, where it used to be copied raw and read back as a
  letter of the new table. LocOS gets the same fix.
- stf: a literal backslash in a translation is written as `\\`, so `C:\new` no longer reads back as a newline.
  The bytes of such a build change (LocOS gets the same fix).
- xcstrings: `stringSet` localizations (App Shortcuts phrases) are read as one unit per string and written back as a
  `stringSet` for the target language, one string per source string in order; a set with a phrase that has no
  new translation is marked `needs_review`. Such catalogs used to open with no units.
- xcstrings: App Intents parameters (`${applicationName}`, `${place}`) are inline codes a translation may move. They
  used to be offered as text (LocOS does the same).
- xliff (LocOS profile): a comment or processing instruction inside a `<source>` is dropped (its tail text stays) and
  takes no code number. It used to become a code pair whose content was the comment text.
- dita, docbook: on a target-language merge, a nested element that holds translated text (a unit whose target
  differs from its source) and still declares the source language in `xml:lang` (DocBook 4.x: `lang`) is restamped
  with the target language; other languages (a foreign
  passage, another region such as `en-GB` in an `en-US` file) are left alone. Only the root used to be stamped.
- dita, docbook: no attribute unit (`navtitle`, `title`, `alt`) on an element under `translate="no"`, inside an element
  with a `conref` or `conkeyref`, or with the value `-dita-use-conref-target`. They used to be offered for
  translation. The conref element's own attributes stay translatable (DITA 1.3 conref attribute rules).
- wxl: the line breaks of a changed element text value take the file's line break (the first one it uses). A CRLF file
  used to get bare LF in a multi-line translation under the LocOS profile.
- xml, xmltree, xmlres, wxl: a start tag with tens of thousands of `xmlns:pN` declarations or attributes no longer
  makes the scan quadratic. xmltree refuses more than 1,024 namespace declarations in scope at one element, and a file
  whose namespace scopes would hold more than 4,194,304 entries in all.

- dxf: a lone `%` before a `%%d` style code keeps the code (it was written `%%%d`); a CRLF drawing gets the same group
  code budget as an LF one; DOS864 drawings read byte 0x25 as `%`; a code that ends at the value end is closed before
  white space too. See MIGRATION.md.
- htmlhelp: `gb2312`, `shift_jis`, `euc-kr`, `ks_c_5601-1987` and `big5` declared charsets are read as the Windows
  supersets (GBK, 932, 949, 950), so a localized CJK export can be read back.
- zip batch: a member path with a Windows device name (`CON/a.md`, `aux.md`, `COM1.md`, ...) refuses the archive.
  Package formats (DOCX, EPUB, ...) still accept such part names.
- HTML: an end tag with no open tag (`<p>Hello</b> big</p>`) is kept as a point code, so a translated build writes it
  back. It used to be blanked with the rest of its unit (also in XHTML, server pages, EPUB, e-mail and Flare).
- Text files that declare ISO-8859-3, -4, -6, -8, -10, -13, -14, -16, Windows-874, IBM866 or the Mac code pages are read
  in that code page and written back in it. Hebrew pages used to come out as Windows-1252 mojibake. A page labelled
  ISO-8859-8 that holds Windows-1255 vowel points is read as Windows-1255 (label kept); a file with a byte its declared
  rare code page leaves undefined is read the way it was before, not refused.
- Token look-alikes: text in a source that looks like a LocOS token (`⟦x2⟧`, `⟦1⟧`) stays text. Real codes in the
  LocOS text skip those numbers, and an unknown token in a translation is kept, not deleted. WebVTT and SubRip builds
  used to drop such text.
- docx, pptx, xlsx (LocOS profile): look-alike text (`⟦x2⟧`) that a source or a translation holds as plain text is
  written back as text. The writers used to read it as a token, so it was dropped, turned into a break or object, or
  moved. A token in a translation that names no code of the unit is now kept as text, not dropped (#814).
- xliff, sdlxliff, mqxliff, mxliff, xlz, mqxlz, sdlppx and sdlrpx (LocOS profile), mdx, srt and vtt (LocOS profile):
  the same for their writers. Look-alike text in a source or a translation is written back as text, even one that
  names a real code of the unit; it used to be dropped, become an empty code pair, or take the real code's place.
  The MDX emphasis repair still reads such text the way LocOS does; a repair that would change a token (the CJK
  emphasis repair can move a mark into one, `⟦*x1⟧`, which LocOS writes as text and so loses the code) is not
  applied to that line. The public `bilingua_xliff::locos` functions
  `target_text`, `written_texts` and `build_cat` keep their 0.9.0 behaviour (#814).
- LocOS profile: a Markdown file with a byte order mark reads its first heading as a heading (the mark is not part of
  the line) and keeps the mark; ARB `@@locale` and plural keys use the file's CRLF; resx writes a translation's line
  breaks as CRLF in a CRLF file; the CJK autolink repair leaves a URL alone when the source already had it glued to
  text (a link text such as `[https://x.example/a #hash](<a b.md>)`).
- XAML, LocOS profile: a translation's line breaks are written as CRLF in a CRLF file (they used to stay LF unless the
  string's own text held a CRLF).

## [0.9.0] - 2026-11-06

The first public release.

### Formats

92 filter ids in 67 format crates, all behind the `bilingua` facade (`bilingua::filters()`, `bilingua::detect`). The full
table, with extensions, is in the README.

- Office and publishing: Word, Excel, PowerPoint, Visio, OpenDocument (text, spreadsheet, presentation, drawing),
  PDF (text layer, extract only), InDesign IDML, InCopy ICML and WCML, FrameMaker MIF, QuarkXPress Tags, EPUB 2 and 3, AutoCAD DXF.
- Web, documentation and help: HTML and XHTML, Markdown, MDX, AsciiDoc, reStructuredText, LaTeX, MediaWiki and
  DokuWiki, XML with ITS 2.0 rules, XSLT, server pages (ASP.NET, JSP, PHP templates, SSI, ERB), HTML Help, MadCap
  Flare projects, email messages, SVG text and the text layer of raster images.
- Software strings: JSON, JSON5 and JSONC, YAML, TOML, INI, Java and Mozilla properties, gettext PO, POT and MO,
  .NET resx and resw, Android string resources, Java XML properties, XAML, Windows resjson and RC, WiX, Apple strings, stringsdict, property lists, String Catalogs
  and Xcode localization bundles, Flutter ARB, Mozilla Fluent and DTD, Qt Linguist TS, JavaScript and TypeScript
  locale modules, PHP language arrays, Google and Angular XMB and XTB, freedesktop desktop entries, Haiku catkeys,
  Salesforce STF, CSV and TSV, plain text.
- Subtitles and captions: SRT, WebVTT, ASS and SSA, TTML and DFXP, EBU STL, SCC, SBV, SubViewer and MicroDVD.
- Bilingual and package formats: XLIFF 1.2 and 2 (with SDLXLIFF, memoQ MQXLIFF and Phrase MXLIFF), Trados TTX,
  Wordfast TXML and TXLF, Trados project and return packages and project files, memoQ packages, WorldServer XLZ and
  WSXZ packages.

### Library

- One document model for every format (`bilingua-model`): units, segments, inline tags, notes, context and a
  skeleton that rebuilds the file. Documents serialize to JSON.
- Extract, set targets, merge: `extract`, `Document::set_target`, `merge` and `merge_with` (`MergeOptions`: output
  encoding, target language stamping).
- Sentence segmentation with SRX 2.0 rules that understand inline tags (`bilingua-seg`), with sentence, paragraph and
  document granularity, and `resegment`, `join` and `split` to change it later.
- Typed per-format options (`<format>.<name>` keys, listed by `format_option_keys()`), refused when misspelt.
- Profiles: `locos` reproduces Localization OS's output choices.
- ZIP batches (`bilingua::batch`): one uploaded `.zip` becomes one document per supported member, with every
  skipped member listed with its reason (Localization OS's ZIP batch upload, stricter about hostile archives).
- Content dialects as Localization OS picks them: a `.xml` file with a `<resources>` or `<properties>` root goes to
  the Android strings or Java XML properties filter, and a Word file holding an Articulate Storyline "Export to
  Word" table is translated in its Translation column only.
- Builders on every option and model type, `#[non_exhaustive]` on the public structs and enums, so later additions
  do not break callers.

### Safety

- Limits on input size, units, unit length, lines, XML depth and element count, zip entries, unpacked size and
  expansion ratio, and per-format caps; each refusal is a typed `LimitError`.
- XML with entity declarations is refused (no billion laughs, no XXE); DTDs are never read. Hostile zip archives
  (escaping or duplicate names, encryption, links, size or checksum mismatches) are refused.
- A declared encoding is binding. A translation the format cannot hold is refused with `Error::Unwritable` instead
  of being written wrongly.
- An untouched merge gives back the input byte for byte (text formats) or every entry as it was (zip formats).
- The parsing crates use no file system, network, threads, clock or processes, and build for
  `wasm32-unknown-unknown`.

### Command line (`bilingua-cli`)

- `bilingua extract`, `merge` (through XLIFF 2.1 and a sidecar skeleton), `segment`, `wordcount`, `pseudo` and
  `options`.

### Python (`pip install bilingua`)

- `extract`, `merge`, `detect`, `formats`, `profiles` and `version`, on plain dicts and lists in the Document's JSON
  shape. One abi3 wheel for Python 3.11 and later, with type stubs (`py.typed`). Errors raise `BilinguaError` with
  a `code`. The GIL is released while parsing and merging.

### Requirements

- Rust 1.88 or later. Python 3.11 or later for the wheel.

[0.9.0]: https://github.com/patois-labs/bilingua/releases/tag/v0.9.0

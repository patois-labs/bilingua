# Changelog

All notable changes to Bilingua. Every crate shares one version number, so one entry covers the Rust crates, the
`bilingua` command and the Python package. The format follows [Keep a Changelog](https://keepachangelog.com/), and
versions follow [semantic versioning](https://semver.org/) (before 1.0, a minor release may break the API; each break
is listed here).

## [Unreleased]

### Added

- Python: `DocumentHandle.unit_notes()` gives the `context.notes` of every unit that has any, in document order, as
  `{"unit", "segments", "notes"}`, without building the whole dict. An adapter that only wants the notes no longer
  calls `to_dict()`.

- Model: `enc::decode_source_legacy(data, source_lang)`, the last step of `decode_text` for a format with its own
  BOM and UTF-8 handling: the code page of the project's source language, with the same warnings. It returns `None`
  for a language with no code page of its own.
- Model: `enc::encode_text_unchanged(text, encoding)`, `encode_text` for a file nothing was changed in: a page
  labelled ISO-8859-8 that was read as Windows-1255 gets its own bytes back (no switch to UTF-8).

- LaTeX: the format option `tex.text_commands` (comma-separated inline text commands for one call, LocOS
  `LATEX_TEXT_COMMANDS`; adds to `Latex::with_inline_commands`). Model: `enc::DecodeOptions::mark_before_legacy`
  (LocOS BOM read version 2: a UTF-8 BOM before code-page bytes is a mark; the envelope tag gets `;bom8` and the
  mark is written back). W6 B side.

- Python (BILI-18, speed): `extract_handle(name, data, ...)` returns a `DocumentHandle`, a Document kept on the Rust
  side: `locos_units()`, `locos_set_targets(targets)` (a new handle and the problems), `merge(...)`, `to_dict()`,
  `DocumentHandle.from_dict(document)`, the read-only `format`, `parser`, `warnings`, `source_encoding`, `len()` and
  `unit_extras(names=None)`. Each gives what the dict function gives; the Document never crosses to Python unless
  asked. `extract(..., locos_units=True)` returns `(document, units)` in one call. Nothing existing changed.
- Rust: the HTML Help format option `htmlhelp.code_page` (`bilingua_htmlhelp::CODE_PAGE`), the code page of the
  project's `Language=` for a file that declares none (LocOS `hhp_code_page`); `bilingua_htmlcore::html::missing_alt_findings`.
- LocOS profile (W5): LocOS's image inventory (`locos.a11y_images`, `locos.a11y_missing_alt`, `locos.a11y_empty_alt`)
  on HTML, server-page and MDX units, for the missing-alt prompts.

- Rust: `profile::tokens::to_locos_text_coded(format, inlines, codes)` (IDML and ICML ranges as `r` pairs, the rule
  `profile::locos_units` now uses), `profile::A11Y_MISSING_ALT` (DOCX missing-alt findings of both kinds),
  `bilingua_zip::check_xml_parts_except`, `bilingua_pptx::locos::COMMENT_BASE`. Python: `to_locos_text(..., codes=)`.
- LocOS profile, PowerPoint: slide comments (classic and modern, with replies) are units with
  `pptx.translate_comments`, numbered and written as LocOS does (W4).
- Rust: `profile::tokens::repair_locos_text_for`, `profile::tokens::locos_repairs_tags` and `profile::tokens::TagRepair`
  (the CAT formats' tag repair, below); `bilingua_xliffpkg::MEMBER_SLOTS`. Python: `locos_set_targets` lists tag
  repairs as problems of kind `repeated`, `stray_close`, `unclosed` and `pair_as_point`.
- Python: `BilinguaError.detail`, the parser's own sentence (a `parse` error's message without its
  `not a <format> file:` prefix; the message itself for every other code).
- Rust: `profile::tokens::parse_locos_text_for(format, text, map)`: the token rules of a format's LocOS filter (TOML
  and JavaScript modules drop a wrong-kind token that names a code of the unit, LocOS #814.1); `locos_set_targets`
  in Python uses it. `Limits::from_model` in bilingua-arb, -jsmodule, -resjson and -yaml, `Limits::capped_by` in
  bilingua-json5.

- PHP arrays: the block form of the direct-access guard, `if (!defined('X') || !X) { die('...'); }` (SugarCRM, vtiger),
  is accepted like `defined('X') OR die(...)`. Anything else in the block is still refused.
- SVG: a DOCTYPE that declares plain internal entities (`<!ENTITY ns "http://...">`, as Illustrator and Inkscape
  write them; a literal of at most 1024 bytes with no `&`, `<` or `>`) is accepted; external, parameter and nested entities are still refused. A text that uses a declared
  entity is left alone.

- Python: `to_locos_text(format, inlines)`, `from_locos_text(text, map)`, `locos_units(document)` and
  `locos_set_targets(document, targets)` give the LocOS profile's token text, token numbering and target rules to a
  Python caller (the Localization OS adapter), so they exist once, in Rust. `extract` and `merge` take
  `profile_version` (or `profile="locos@1"`); a version the release cannot produce is code `parser_version`. The
  Document from `extract` carries a `parser` record (release, format crate, its version and skeleton schema,
  profile and its version); `merge` ignores it.
- Rust: `profile::resolve`, `profile::LOCOS_OLDEST_VERSION`, `Profile::oldest`, `Profile::reads`,
  `profile::locos_units` and `profile::LocosUnit`.
- bilingua-py: optional `generate-import-lib` feature (a Windows abi3 wheel built off Windows);
  `tools/build-wheel-m5.sh` builds and tests a manylinux_2_28 wheel on a Linux host.

### Changed

- New structure fuzz check S4 (AsciiDoc): the codes set in a translation are the codes read back, and the alphabet now
  has lone marks.

- LocOS profile, OpenDocument (odt, ods, odp, odg): a part that a merge rewrites (`content.xml`, `styles.xml`,
  `meta.xml`) is now written the way LocOS's lxml writes it: UTF-8, the declaration
  `<?xml version='1.0' encoding='UTF-8'?>` and a new line, `&apos;` in attributes as a plain `'` (`&quot;` stays),
  `<x/>` for an empty element, namespace declarations first, no white space after the root. A part with a CDATA
  section or a comment outside the root gets the declaration only. Parts a merge does not rewrite, and the native
  mode, are unchanged. W-D D3 (odf-x).
- LocOS profile, PowerPoint: the segment cap counts every walked paragraph, empty ones included, as LocOS does
  (`check_segment_cap(len(self.paras))`). A deck whose unit count is under the cap but whose paragraph count is over
  it is now refused (`LimitKind::Units`), as LocOS refuses it. Native mode still counts units. W-D D4 (sec-2).

- bilingua-xcstrings, `locos` profile: a changed catalog is written the way LocOS writes it: every object and array
  expanded over lines (a source's compact inline objects are expanded), `{}` for an empty object, LF line ends even
  for a CRLF source, and a final newline when the source had one. Separator, indent and escapes still follow the
  source. Native mode is unchanged (it keeps the source layout and its line ends).

- INI, YAML and JavaScript modules: a file that is not UTF-8 and has no byte order mark is read in the Windows code
  page of the project's source language (a Russian project reads Windows-1251), as text, PO, subtitles, CSV and HTML
  already did and as LocOS does (decision 22). Before, it was read as Windows-1252 whatever the language. The
  warnings (letters that look wrong in that code page) are on the document. Without a source language, or for a
  Western one, nothing changes. An untouched merge writes the same bytes; a changed YAML or JavaScript file is
  written as UTF-8, and an INI file stays in its code page when every character fits.
- HTML: a page labelled ISO-8859-8 that was read as Windows-1255 (it holds vowel points) is written as UTF-8 when a
  character is one that Windows-1255 has at a byte which means another sign under that label (curly quotes, dashes,
  the ellipsis, the euro sign, the new shekel sign). Before, Windows-1255 bytes were written under the
  ISO-8859-8 label, which a browser shows wrong (LocOS issue 810 gate). Such a page still opens, and an
  untouched merge gives its bytes back, as in LocOS.
- LocOS profile, HTML and other formats that declare a charset: a declaration past the first 4096 characters of a
  page that carries a charset label now takes the page's label when the page is written, or UTF-8 when the page is
  written as UTF-8, as LocOS does. Without the profile such a declaration is content and comes back as it was.
- LocOS profile, WiX (`.wxl`): a translation's carriage return is not written as `&#13;` any more; the line breaks
  of a translation take the file's own style, as in LocOS. Without the profile a carriage return that came from
  `&#13;` still comes back as `&#13;`.
- HTML Help (native mode, issue #12): a character that the file's code page cannot hold, in a sitemap `param`
  value of a `.hhc` or `.hhk`, is written as a numeric character reference (`&#272;` for U+0110) instead of refusing
  the merge. The LocOS profile keeps the refusal (`HtmlHelpEncodingError`), and so do project files, `Local` paths
  and every other place. `htmlhelp.utf8_fallback` works as before.

- xml: generic XML whose root has no `xml:lang` still restamps a nested element that declares the source language and
  holds a changed unit, on a target-language merge (TEI `<exemplum xml:lang="en">` becomes `xml:lang="de-DE"`, as LocOS
  writes it). The root is not given a language. A foreign passage and an element with no changed unit are left alone.
- xslt: under the `locos` profile, text written right against a whitespace-only node the stylesheet strips is written
  plain, as LocOS does, instead of inside `xsl:text`. Native mode keeps the `xsl:text` wrap (it keeps the stripped
  whitespace stripped and the units apart on re-read).
- icml: under the `locos` profile a rebuilt Story is written the way lxml writes it (attributes on one line, empty
  elements as `<a/>`), and every line end in a translation becomes InDesign's forced line break (U+2028), which reads
  back as a point, as LocOS does. Native mode keeps the source bytes of the Story and the line feeds of a code block.
- plist: a rewritten binary plist writes equal strings (keys included), integers, reals, dates, data, booleans and
  nulls once, as plistlib does, and uses the smallest reference size. The file no longer doubles in size; it reads
  back the same.

- LocOS profile (W5): `locos.p_index` for Android strings, Java XML properties, XMB, XTB and TTML is LocOS's number
  (skipped entries counted); an untouched XMB merges to the XTB; e-mail and TTML units have no style; html, xslt and
  the HTML Help files keep their encoding (legacy code page, UTF-16, ANSI code page with a refusal for what does not
  fit); `locos_set_target` repairs repeated, stray and unclosed tags for the XML formats and takes a repeated tag in
  MDX; an untouched PHP array keeps its bytes. See MIGRATION.md, "Wave 5 B-side".
- HTML Help: a project file (.hhp) is read in the code page of its own `Language=` (LocOS #797). MDX: a front matter
  translation with a tab is quoted.
- LocOS profile, wave 6 (W6 B side): units of ASS, LaTeX, plist, stringsdict and MO have LocOS's empty style; MO
  placeholders (`%d`, `{name}`) are unit text, as LocOS's PO reading of an MO gives them; rc, plist, CSV, TSV and
  XPress Tags keep their encoding on merge as LocOS does (UTF-16, a code page when the text fits, rc's `#pragma
  code_page(65001)` and UTF-8 when it does not); a UTF-16 ASS file is written as UTF-8 with a BOM; `stf.load_targets`
  defaults to false; MicroDVD and XPress Tags translations that repeat a style pair are written with the repeat's
  tags dropped (its text kept), as LocOS does, where they were refused; a pair token for a point code in rc is
  dropped (LocOS `rc_filter`); a DXF `p` token is not a code (LocOS reads only `x` and `g`) and is dropped by the
  DXF writer, each side of it escaped on its own; a localized SBV/SubViewer/MicroDVD export with nothing translated
  is written (as UTF-8), as LocOS does; a CSV/TSV read without `csv.target_locales` or `csv.columns` is not planned
  by the export's target language. Native and profile: a subtitle refusal names the cue in LocOS's sentence
  ("Cue 3 was not exported: ..."); an rST table cell that cannot fit is `unwritable`, not `write`.

- LocOS profile, Office and DTP formats (W4): DOCX units carry LocOS's image inventory for the missing-alt prompts
  and give the Storyline notice; XLSX text cells starting with `=` and values under a merged range are not units; ODF styles are `heading`, `comment`
  or none; Visio `p_index` is LocOS's slot index (with gaps) and comments are `1000000 + j`; IDML and ICML ranges
  are `r` pairs; ICML refuses a bare `<Story>` root and skips story-level tables, as LocOS does. Native and profile:
  a DOCX `customXml/` part is no longer depth-checked; a rebuilt ICML story keeps the file's CRLF line ends; a
  rebuilt IDML or ICML range keeps the text after its `<Properties>`. See MIGRATION.md, "Wave 4 B-side".
- Python, speed (same objects and errors): `locos_units` writes its result straight into Python objects, and the
  Document reader takes exact `str`, `dict` and `list` objects first.
- LocOS profile, CAT formats (W3): a translation that repeats, leaves open or wrongly closes a pair tag is repaired as
  LocOS's CAT writers do instead of refused (xliff, sdlxliff, mqxliff, mxliff, txlf, txml, ttx, xlz, mqxlz, sdlppx);
  confirmed TXML and TXLF segments are protected; TXML, TXLF and TTX keep LocOS's `p_index`, no style, and are written
  in their own encoding (UTF-16 stays UTF-16); plain XLIFF has LocOS's #571 inline copy budget; xlz and mqxlz list their
  members' slot counts. See MIGRATION.md, "Wave 3 B-side".
- Error codes: the json, docx, pptx and xlsx nesting caps are `limit` (`LimitKind::Depth`), not `parse`; an mqxlz
  refusal names `mqxlz`.

- Speed (no change in output): `detect` no longer sniffs every filter's bytes for a file whose extension already
  decides (the sniff result was thrown away), and `extract` and `resegment` compile the bundled SRX rules only for
  sentence segmentation (3 to 4 ms on the first call of a process). Rust: `bilingua_seg::Rules::empty()`.
- Python, speed (same objects, key order and errors): the Document is written straight into Python objects and read
  straight from them, with no `serde_json::Value` copy in between (about half the time of `extract`, `merge`,
  `locos_units` and `locos_set_targets` on small files). New dependency of the wheel: `pythonize` (MIT).
- Error codes (W2): every size, budget and nesting refusal is code `limit` (`Error::Limit` with a `LimitKind`) and
  every byte-level decode refusal code `encoding` (`Error::Encoding`), with LocOS's sentence where LocOS has one, in
  arb, dtd, ftl, ini, jsmodule, json5, properties, resjson, strings, toml and yaml. They were `unsupported` or
  `parse`. No code was added or removed.
- arb, jsmodule, resjson, toml, yaml: `Filter::extract` takes its limits from `ExtractOptions::limits` (LocOS's
  numbers by default), so the adapter's `MAX_DOCUMENT_SEGMENTS` reaches their budgets; json5 lowers its own limits to
  them when they are set. LocOS's pre-parse budgets added: arb (`check_json_budget`), toml (`_budget`), resx, wxl,
  Android strings, Java XML properties and Qt ts (`check_xml_budget`).
- LocOS profile: xaml and resjson keep their encoding (UTF-16 with its BOM, a declared legacy charset) as LocOS does;
  yaml values are plain text with no style (LocOS's yaml filter has no inline map); an xcloc bundle's members are read
  and written by the LocOS XLIFF port (trimmed text, `p_index` offset by the sources of the earlier members, LocOS's
  writer and `_mark_translated`).
- xcstrings (LocOS profile): a changed catalog writes empty objects as `{}`, as LocOS's `json.dumps` does.
- xaml: a DOCTYPE is refused with LocOS's sentence, "This XAML file declares a DOCTYPE / DTD; refused (XAML never
  needs one)."

- markdown: `markdown.fm_keys` is applied (front matter keys to translate, LocOS's `LOC_TMS_FRONT_MATTER_KEYS`), in
  native mode and under the LocOS profile; the profile's units carry LocOS's image inventory as unit extras
  (`profile::A11Y_IMAGES`, `profile::A11Y_EMPTY_ALT`).
- Encodings: a declared charset may use Python's codec names (`CP932`, `cp936`, `uhc`, ...) as LocOS reads them
  (`enc::label_encoding`, new); a PO declaring CP932 is read and written in Shift_JIS under its label.
- json, po: LocOS's pre-parse budgets (`guard::check_budget`, new): a quote, value or escape flood is a `limit`
  error, not an empty file.
- markdown: the LocOS-profile skeleton schema is 2 (the skeleton starts with the file's encoding tag, as po and
  srt do). A stored schema 1 Document (no tag line) still merges as before; its `source_encoding` stays as stored.
- Tests: the LocOS goldens were made again from Localization OS with its fixes for trackers 805, 810, 811, 813 and
  814; the cases kept as known differences (html orphan end tag, Markdown BOM, PPTX look-alike tokens, 15 DXF
  drawings) now match and their lists are gone.

### Fixed

- adoc: a lone mark character (`*`, `_`, backtick, `#`, `^`, `~`, `+`) in a translation no longer pairs with a code's mark,
  next to it or further along, or with a mark on the other side of a code (which would wrap and swallow the code). Where it
  could open or close formatting it is written as a character reference, which prints the same. The line is first written
  as typed, and the references are kept only when that line would not read back as the same codes and text, so a mark the
  source already had as plain text (`u_`, a backtick glued to a word) stays a plain character. Under the LocOS profile the
  bytes then differ from LocOS only for marks that would pair.

- LocOS profile, server pages (aspx, erb, shtml, jsp, phtml), EPUB and mail HTML parts: an inline element whose end
  tag is not in the same unit (it follows a code statement or a block child) is a pair token `⟦g1⟧` with no closing
  token, as LocOS writes it, not a point code `⟦x1⟧`. Model: `to_locos_text_for` treats these formats like `html`.
- LocOS profile, server pages: a unit that has a translation equal to its source is still rewritten, so an
  apostrophe in it is written `&#x27;` (and a bare `<` or `&` escaped) as LocOS does. Native mode leaves such a unit's
  bytes alone.
- LocOS profile, email: a white-space only (or empty) translation of a unit in an HTML part is written as it is
  (`<h1>  </h1>`), as LocOS does, instead of leaving the source text.
- MDX: a link reference title decodes each backslash escape once, so a title holding a backslash before a quote reads
  back as typed (LocOS `md_blocks_735` `test_titles`). Markdown already did.
- LocOS profile, HTML and server pages: a page read in a legacy code page whose only charset declaration sits past the
  first 4096 characters is written as UTF-8 with that declaration rewritten to UTF-8 (LocOS `encode_like` gate).
  New `enc::encode_declared_with`. Native mode keeps the code page and the declaration.

- idml, icml: under the `locos` profile `locos_set_target` repairs a target that repeats a pair token (the repeat loses
  its tags, its text stays), as LocOS writes it, instead of refusing it as `duplicated_tag`.

- LaTeX (native and profile): a token look-alike that names no code of the unit (`⟦x7⟧`) was deleted from the
  export; it is text and is kept (#814.1, W6 tex-1). One that names a code but was left as text (a closing mark of
  a standalone code) is still dropped, as LocOS does.
- SBV, SubViewer, MicroDVD and ASS (native and profile): a UTF-8 BOM before Windows-1252 bytes lost the first cue
  or the `[Script Info]` Title; the BOM is read as a mark and the rest in the code page (LocOS #799.4 / #800.4).
- Declared Windows-1252 / ISO-8859-1 XML (Qt Linguist and the other declared readers): a project with a CJK source
  language read it as that language's code page and refused it; the declaration decides (W6 ts-1).

- LocOS profile, TXLF: a translation equal to the target the file already had is written, and its `new` or
  `needs-*` state becomes `translated`, as LocOS writes it (it kept `needs-translation`). An open segment's Document
  under the profile no longer carries the file's target; native reading is unchanged.
- WiX (native and profile): a run of white space beside a string's comments (`&#160;`, `&#32;`, a blank CDATA
  section) no longer makes a skipped mixed string; the one run of text is the value, as LocOS G13 reads it.

- XML: a character reference outside XML 1.0's `Char` (`&#30;`, `&#0;`) is not well-formed in `bilingua-xmltree`
  (lxml's sentence); Bilingua's own XLIFF reader keeps it as the text it is instead of the character, so it is never
  written into a file.

- PDF: a Type 0 font with an Identity CMap, an Adobe CJK `CIDSystemInfo` (GB1, CNS1, Japan1, Korea1) and no
  ToUnicode now reads its CIDs through PDFium's CID to Unicode tables, as Localization OS does. Before, Latin text
  in such a font came out shifted (`Computer Science` read as `$PNQVUFS4DJFODF`) and CJK text as unrelated
  characters. The tables add about 75 KB to the crate (compressed, inflated on first use).

- HTML, XML-like and PO files with two charset declarations (a forum page that embeds another page): only the
  declarations in the first 4096 characters are the file's own, so the second no longer makes an untouched merge
  differ and the file is no longer refused as "cannot be written back unchanged". The file's own declarations get
  their label back wherever a translation moved them, and a legacy file whose only declaration comes after the first
  4096 characters is read by the default path with a `text_encoding` warning that names the later charset.
- HTML and server pages: a semicolon-less legacy reference the source wrote in text (`&nbsp&nbsp text`, `&copy`) is
  written back as it is in a translation, not as `&amp;nbsp` (which a browser shows as the letters). LocOS escapes it.
- docx (native mode): the text of a DrawingML shape that is not a Word text box (`a:p` in a lockedCanvas or a group
  shape) is a unit (`d1`, ...). Such a file opened with no units. Run formatting is kept as paired tags, and the VML
  `mc:Fallback` copy is written from the same translation when it has the same text. Paragraphs with line breaks or
  fields stay untouched.
- srt (native mode): word-timing marks such as `<00:00:04.480>` (YouTube-style captions) are standalone codes, not text.

- markdown (LocOS profile and native): a plain front matter value that goes on over the next indented line
  (`subtitle: A plain value that` and `  continues here`) is left alone. It was offered cut at the first line, and a
  translation would have replaced only that line. LocOS reads the whole block and skips such a key.
- po (LocOS profile): every target language LocOS knows through CLDR but babel's gettext table lacks now gets its
  `Language` header and its plural rule: `tl` (Tagalog), the three-letter ISO 639-2 and 639-3 forms (`tgl`, `ind`,
  `tur`, ...), the old codes (`in`, `iw`, `ji`, `jw`, `mo`) and the other aliases babel resolves (232 more table rows).
  `tl` had an empty `Language` and the source file's plural rule.
- markdown, po, srt (LocOS profile): `Document::source_encoding` reports the encoding the file was read from
  (`windows-1252`, `utf-16le`, ...), as it does for the other text formats. It was always `None`.

- html: the preview declares the target language again once something is translated (`lang`, and `dir="rtl"` for a
  right-to-left target), as the merge does. The htmlcore rebuild had dropped it, so an Arabic preview showed left
  to right.
- mdx: a translation's backtick no longer opens a code span that leaves `{x}` or `<X>` live: spans are found the
  CommonMark way (a whole backtick run, a backslash-escaped backtick is a literal, no span across a table cell `|`)
  (security; LocOS tracker #790).
- mdx, markdown: a translated line start that opens a code fence (```` ``` ````, `~~~`) is escaped; the fence
  swallowed the rest of the file (LocOS tracker #790). In native Markdown every block marker a translated unit
  would open at a block start (`# `, `- `, `1. `, `> `, a rule, a setext underline) is escaped too, a translated
  table cell escapes `|`, a bracket in a link's text is escaped and a heading's trailing ` #` is kept.
- mdx: a translated line that starts with `import ` or `export ` writes its first letter as a character reference,
  so it is not compiled as an ES module (LocOS tracker #790).
- mdx, markdown: a translated line start indented four columns or more (an indented code block) loses the blanks.
- mdx, markdown (LocOS profile): heading text gets the block-start escape too (`## 1. x` read back as `x`). MDX
  escapes a stray backtick in a changed line with codes and doubles a backslash right before a code, so a code is
  never read back as text.
- adoc: `x:: w` (also `:::`, `::::`, `;;`) in a translation is written with `{two-colons}` / `{two-semicolons}`, so
  a paragraph, admonition or term does not become a description list item (LocOS tracker #788). A term ending in
  `:` keeps its delimiter, a title ending in its own marker keeps it, and a translated mark next to a code's mark of
  the same character (or a `[...]` right before a mark) is written as a character reference.
- New structure fuzz (`bilingua/tests/structure_fuzz.rs`, proptest with a fixed seed): random translations from each
  format's special characters (MDX, Markdown native and LocOS profile, AsciiDoc, HTML, MediaWiki, JS modules, PHP,
  JSP, SVG) must keep the unit count and kinds, the skeleton, and read back as set.

- stl: the locos profile writes a base letter and its floating mark as a byte pair (as LocOS now does), and the pair
  takes one cell in row width. stf: an escape (backslash, line feed, carriage return, tab) the source row lacks is decoded in a loaded translation,
  so an edit and merge escapes it once. htmlhelp: `hhp_code_page` gives `None` over a size, `<` or line limit, as LocOS.
  Golden tests for stl, ass and subtext regenerated from LocOS build/issues-1107 (LocOS tracker #798, #805, #814, #797).
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
- rst, ARB and JSON (Chrome, go-i18n, FormatJS, ICU and the other JSON dialects): a token in a translation that names
  no code of the unit (`⟦x9⟧`) is kept as literal text, with the format's escaping, in both profiles. The rst writer
  used to delete it; the ARB and JSON writers (LocOS profile) wrote the source message instead of the translation
  (#814).
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

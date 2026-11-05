# Changelog

All notable changes to Bilingua. Every crate shares one version number, so one entry covers the Rust crates, the
`bilingua` command and the Python package. The format follows [Keep a Changelog](https://keepachangelog.com/), and
versions follow [semantic versioning](https://semver.org/) (before 1.0, a minor release may break the API; each break
is listed here).

## [0.9.0] - unreleased

The first public release.

### Formats

85 filter ids in 63 format crates, all behind the `bilingua` facade (`bilingua::filters()`, `bilingua::detect`). The full
table, with extensions, is in the README.

- Office and publishing: Word, Excel, PowerPoint, Visio, OpenDocument (text, spreadsheet, presentation, drawing),
  InDesign IDML, InCopy ICML and WCML, FrameMaker MIF, QuarkXPress Tags, EPUB 2 and 3, AutoCAD DXF.
- Web, documentation and help: HTML and XHTML, Markdown, MDX, AsciiDoc, reStructuredText, LaTeX, MediaWiki and
  DokuWiki, XML with ITS 2.0 rules, XSLT, server pages (ASP.NET, JSP, PHP templates, SSI, ERB), HTML Help, MadCap
  Flare projects, email messages.
- Software strings: JSON, JSON5 and JSONC, YAML, TOML, INI, Java and Mozilla properties, gettext PO, POT and MO,
  .NET resx and resw, XAML, Windows resjson and RC, WiX, Apple strings, stringsdict, property lists, String Catalogs
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

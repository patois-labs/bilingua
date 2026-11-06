# Bilingua

File parsers for translation, in Rust, with a command line tool and a Python package.

Bilingua opens a source file (a Word document, a PO catalog, a subtitle file, an XLIFF from a CAT tool, a Flutter
ARB file and many more), gives you its translatable text as units and sentences with the formatting kept as inline
tags, and writes the translated file back in its original format. Everything else in the file stays as it was.

It is bytes in, bytes out. The parsing crates use no file system, network, threads, clock or processes, so the same
code runs on a server, in a desktop app, from the command line, inside Python and (it compiles for WebAssembly) in
the browser. It does not depend on any editor, service or machine translation engine.

Status: 0.9, the first public release. The API is close to what 1.0 will be; see [Versions](#versions).
Licensed under Apache 2.0.

## Formats

Generated from `bilingua::filters()` by `tools/gen-format-table.sh`; do not edit the table by hand. "Bilingual"
formats carry source and target text (translation exchange formats); Bilingua reads the existing targets and writes
yours.

<!-- formats:start -->
| Format | Filter id | Extensions | Bilingual |
|---|---|---|---|
| Adobe FrameMaker MIF | `mif` | `.mif` |  |
| Adobe InCopy ICML and WoodWing WCML | `icml` | `.icml` `.wcml` |  |
| Adobe InDesign IDML | `idml` | `.idml` |  |
| Advanced SubStation Alpha and SSA subtitles | `ass` | `.ass` `.ssa` |  |
| Apple .strings | `strings` | `.strings` |  |
| Apple .stringsdict | `stringsdict` | `.stringsdict` |  |
| Apple property lists (XML and binary) | `plist` | `.plist` |  |
| AsciiDoc | `adoc` | `.adoc` `.asciidoc` `.asc` |  |
| ASP.NET pages and controls | `aspx` | `.aspx` `.ascx` |  |
| AutoCAD DXF (ASCII) | `dxf` | `.dxf` |  |
| CSV | `csv` | `.csv` |  |
| DITA topics, maps and bookmaps | `dita` | `.dita` `.ditamap` `.bookmap` |  |
| DocBook 4.x and 5 | `docbook` | `.dbk` `.docbook` |  |
| DokuWiki markup | `dokuwiki` | `.dokuwiki` |  |
| DTD entity files (Mozilla) | `dtd` | `.dtd` |  |
| EBU STL subtitles | `stl` | `.stl` |  |
| Email messages | `eml` | `.eml` |  |
| EPUB 2 and 3 | `epub` | `.epub` |  |
| ERB templates | `erb` | `.erb` |  |
| Flutter ARB | `arb` | `.arb` |  |
| freedesktop .desktop entries | `desktop` | `.desktop` |  |
| gettext MO (binary) | `mo` | `.mo` `.gmo` | yes |
| gettext PO and POT | `po` | `.po` `.pot` | yes |
| Google and Angular XMB | `xmb` | `.xmb` |  |
| Google and Angular XTB | `xtb` | `.xtb` |  |
| Haiku catkeys | `catkeys` | `.catkeys` |  |
| HTML and XHTML | `html` | `.html` `.htm` `.xhtml` |  |
| HTML Help index | `hhk` | `.hhk` |  |
| HTML Help project | `hhp` | `.hhp` |  |
| HTML Help table of contents | `hhc` | `.hhc` |  |
| INI and Joomla language files | `ini` | `.ini` |  |
| Java and Mozilla .properties | `properties` | `.properties` |  |
| JavaScript and TypeScript locale modules | `js` | `.js` `.cjs` `.mjs` `.ts` `.mts` `.cts` |  |
| JavaServer Pages | `jsp` | `.jsp` `.jspf` |  |
| JSON | `json` | `.json` |  |
| JSON5 and JSONC | `json5` | `.json5` `.jsonc` |  |
| LaTeX | `tex` | `.tex` `.latex` `.ltx` |  |
| MadCap Flare project | `flprjzip` | `.flprjzip` |  |
| Markdown (CommonMark, GitHub tables) | `markdown` | `.md` `.markdown` `.mdown` `.mkd` |  |
| MDX | `mdx` | `.mdx` |  |
| MediaWiki markup | `wiki` | `.wiki` `.mediawiki` `.wikitext` |  |
| memoQ MQXLIFF | `mqxliff` | `.mqxliff` | yes |
| memoQ package (MQXLZ) | `mqxlz` | `.mqxlz` | yes |
| Microsoft .NET resx and resw | `resx` | `.resx` `.resw` |  |
| Microsoft Excel | `xlsx` | `.xlsx` `.xlsm` `.xltx` `.xltm` |  |
| Microsoft PowerPoint | `pptx` | `.pptx` `.pptm` `.potx` `.potm` `.ppsx` `.ppsm` |  |
| Microsoft Visio | `vsdx` | `.vsdx` `.vsdm` |  |
| Microsoft Word | `docx` | `.docx` `.docm` `.dotx` `.dotm` |  |
| Mozilla Fluent | `ftl` | `.ftl` |  |
| OpenDocument drawing | `odg` | `.odg` `.otg` |  |
| OpenDocument presentation | `odp` | `.odp` `.otp` |  |
| OpenDocument spreadsheet | `ods` | `.ods` `.ots` |  |
| OpenDocument text | `odt` | `.odt` `.ott` |  |
| PDF (text layer, extract only) | `pdf` | `.pdf` |  |
| PHP array language files (Laravel, CodeIgniter) | `php` | `.php` |  |
| PHP templates | `phtml` | `.phtml` |  |
| Phrase MXLIFF | `mxliff` | `.mxliff` | yes |
| Plain text | `txt` | `.txt` `.text` |  |
| Qt Linguist TS | `ts` | `.ts` |  |
| QuarkXPress Tags | `xtg` | `.xtg` `.tag` |  |
| Raster images (text layer: PNG, JPEG, TIFF, BMP, GIF, WebP) | `image` | `.png` `.jpg` `.jpeg` `.tif` `.tiff` `.bmp` `.gif` `.webp` |  |
| reStructuredText | `rst` | `.rst` `.rest` |  |
| Salesforce Translation Workbench | `stf` | `.stf` |  |
| Scenarist Closed Captions | `scc` | `.scc` |  |
| Server-side includes (SSI) | `shtml` | `.shtml` `.shtm` `.stm` |  |
| SubRip subtitles | `srt` | `.srt` |  |
| SubViewer 2.0 and MicroDVD subtitles | `sub` | `.sub` |  |
| SVG text | `svg` | `.svg` |  |
| TOML | `toml` | `.toml` |  |
| Trados project and return packages | `sdlppx` | `.sdlppx` `.sdlrpx` | yes |
| Trados SDLXLIFF | `sdlxliff` | `.sdlxliff` | yes |
| Trados Studio project file | `sdlproj` | `.sdlproj` |  |
| Trados TagEditor TTX | `ttx` | `.ttx` | yes |
| TSV | `tsv` | `.tsv` `.tab` |  |
| TTML and DFXP subtitles | `ttml` | `.ttml` `.dfxp` |  |
| WebVTT subtitles | `vtt` | `.vtt` |  |
| Windows .resjson | `resjson` | `.resjson` |  |
| Windows resource scripts | `rc` | `.rc` |  |
| WiX localization | `wxl` | `.wxl` |  |
| Wordfast Pro TXLF | `txlf` | `.txlf` | yes |
| Wordfast Pro TXML | `txml` | `.txml` | yes |
| WorldServer and Idiom XLIFF packages | `xlz` | `.xlz` `.wsxz` | yes |
| XAML resources (WPF, UWP, WinUI, MAUI, Avalonia) | `xaml` | `.xaml` |  |
| Xcode localization bundle | `xcloc` | `.xcloc` | yes |
| Xcode String Catalog | `xcstrings` | `.xcstrings` |  |
| XLIFF 1.2 and 2 | `xliff` | `.xlf` `.xliff` | yes |
| XML with ITS 2.0 rules | `xml` | `.xml` |  |
| XSLT stylesheets | `xslt` | `.xsl` `.xslt` |  |
| YAML (generic, Rails i18n, Symfony) | `yaml` | `.yaml` `.yml` |  |
| YouTube SBV subtitles | `sbv` | `.sbv` |  |

90 formats.
<!-- formats:end -->

`bilingua::detect` picks the format by extension and then by looking at the bytes, so most files need no hint.
Each format lives in its own crate (`bilingua-po`, `bilingua-docx`, ...); the `bilingua` crate puts them all
behind one API, and that is the crate to use.

## Quick start

### Rust

```toml
[dependencies]
bilingua = "0.9"
```

```rust
use bilingua::{ExtractOptions, Granularity, Inline};

let bytes = std::fs::read("page.html")?;
let opts = ExtractOptions::new()
    .with_source_lang("en")
    .with_target_lang("es-ES")
    .with_granularity(Granularity::Sentence);
let mut doc = bilingua::extract("page.html", &bytes, &opts)?;

// Each unit is a paragraph, a cell, a string resource...; each segment a sentence of it.
for unit in &doc.units {
    for seg in &unit.segments {
        println!("{}\t{}", seg.id, bilingua::plain_text(&seg.source));
    }
}

let id = doc.units[0].segments[0].id.clone();
doc.set_target(&id, vec![Inline::text("Hola mundo.")])?;
let out: Vec<u8> = bilingua::merge(&doc)?; // the original format, with the translation in place
```

A `Document` serializes to JSON (serde), so you can store it between extract and merge. `MergeOptions` choose the
output encoding and target language stamping; `bilingua::format_option_keys()` lists the per-format options
(`csv.columns`, `odf.translate_comments`, ...). [MIGRATION.md](MIGRATION.md) shows the builder style every option and
model type uses.

### Command line

```
cargo install bilingua-cli

bilingua extract page.docx --target es-ES --mode sentence -o page.xlf   # XLIFF 2.1 and a sidecar skeleton
bilingua merge page.xlf -o page.es.docx                               # the translated Word file
bilingua segment notes.txt --source en
bilingua wordcount strings.json
bilingua pseudo page.md -o page.pseudo.md
bilingua options                                                     # every format option key
```

See [bilingua-cli/README.md](bilingua-cli/README.md).

### Python

```
pip install bilingua
```

```python
import bilingua

data = open("messages.po", "rb").read()
doc = bilingua.extract("messages.po", data, source_lang="en", target_lang="es", granularity="sentence")
for unit in doc["units"]:
    for seg in unit["segments"]:
        seg["target"] = [{"text": "..."}]   # a list of inlines: {"text": ...} or {"tag": {...}}
open("messages.es.po", "wb").write(bilingua.merge(doc))
```

The Document is plain dicts and lists, the same shape as its JSON. One wheel works for every Python from 3.11, type
stubs included. Errors raise `bilingua.BilinguaError` with a `code` (`parse`, `limit`, `locked`, ...). See
[bilingua-py/README.md](bilingua-py/README.md).

## Safety model

Bilingua is built to open files from people you do not know.

- **Limits.** Every input is checked against limits before and while it is parsed: file size (100 MB), units
  (50 000), characters in one unit (100 000), lines, XML depth (256) and element count, zip entry count (10 000),
  unpacked size (256 MB) and expansion ratio (200 times), and per-format caps such as spreadsheet cells and rows.
  Each one can be changed through `Limits` in `ExtractOptions` (or `limits=` in Python).
- **Refusals, not guesses.** A file over a limit, or one that uses a known attack, is refused with a typed error
  (`Error::Limit` with a `LimitKind` a program can branch on, and a short message safe to show to whoever uploaded
  the file). XML that declares an entity (internal or external, so the billion laughs and XXE families) is refused
  before any parser sees it; DTDs are never read or fetched. Zip archives with names that escape a folder, names
  listed twice, encrypted entries, links, or sizes and checksums that do not match their directory are refused.
  Text that is not valid in the encoding a file declares is refused rather than silently re-read in another one.
- **The writer never invents structure.** A translation that cannot be written in the format (it does not fit, a
  character has no code in the file's code page, it would read back as markup) is refused with
  `Error::Unwritable` and the unit id. A target that adds or repeats an inline tag is refused.
- **Untouched means untouched.** Merging a document with nothing translated gives back the input byte for byte for
  every text format. Zip-based formats (Office, OpenDocument, IDML, EPUB, packages) keep every untouched entry's
  compressed bytes, compression method and time stamp as they were; only the parts holding translated text are
  written again.
- **Round trips are tested.** Every format round-trips a full pseudo-translation (merge, reopen, compare). Format
  crates have hostile-input tests (truncated files, zip and XML bombs, deep nesting, bad encodings).

## Parity with Localization OS and Okapi

Bilingua grew out of Localization OS, the open source translation management system from Patois Labs, whose Python filters it
replaces format by format. Each format crate is tested against what the Localization OS filter extracts from the same
files (same units, same text, same notes and context) and against its round trip, using golden files generated from
that filter. Where Bilingua knowingly differs, the crate's README says so. `MergeOptions` and the `locos` profile
(`profile="locos"` in Python) reproduce Localization OS's exact output choices.

Where the Okapi Framework has a filter for the same format, several crates also run Okapi's own test files and check
that Bilingua finds the same text. Bilingua is not a port of Okapi and does not depend on it or on Java; those test
files stay in the source repository and are not part of the published packages.

## Versions

All crates share one version number and are released together. 0.9 is the first public release: expect a few
breaking changes before 1.0, each listed in [CHANGELOG.md](CHANGELOG.md). Public model types are
`#[non_exhaustive]` and built with constructors, so most additions after 1.0 will not break callers. Items marked
`#[doc(hidden)]` are not covered by semver. The minimum supported Rust version is 1.88.

## Building from source

```
cargo test --workspace --exclude bilingua-py
tools/check-portable.sh        # the parsing crates build for wasm32-unknown-unknown and use no OS APIs
```

`bilingua-py` builds with [maturin](https://www.maturin.rs/) (see its README). `oracle/` holds the comparison
harness; it is a test aid, never a dependency, and is not published.

## Licence

Apache License 2.0; see [LICENSE](LICENSE) and [NOTICE](NOTICE). Some generated tables come from Unicode data
(the Unicode Character Database and CLDR) and carry the Unicode License v3; the PDF crate holds data from
PDFium and Adobe's glyph list and font metrics (BSD 3-Clause and Adobe's AFM notice). NOTICE reproduces these.

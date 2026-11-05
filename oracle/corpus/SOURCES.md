# Corpus sources

Every file here is made up for Bilingua (Lumen Bakery is a fictional company; no real client text), except the Okapi test resources listed below.

| Files | How they were made | Licence |
|---|---|---|
| `docx/features`, `docx/languages`, `docx/runs`, `docx/edge`, `xlsx/menu`, `pptx/launch`, `html/*`, `md/*`, `json/*`, `po/*`, `srt/*`, `vtt/*`, `txt/*` | `gen_corpus.py` writes the XML and text by hand (stdlib only) | Apache-2.0, same as Bilingua |
| `docx/manual.docx`, `xlsx/products.xlsx` | `gen_office.py`, with python-docx 1.2 and openpyxl 3.1 (both MIT). The files contain those libraries' default package parts (styles, theme, settings) | Apache-2.0 for the invented text; the default parts come from MIT-licensed libraries |
| `idml/*.idml` (7 files: `03-hyperlink-and-table-content`, `07-paragraph-breaks`, `08-conditional-text-and-tracked-changes`, `09-footnotes`, `10-tables`, `11-xml-structures`, `1415-adjacent-codes`) | Okapi Framework test resources, copied unmodified from https://gitlab.com/okapiframework/Okapi (`okapi/filters/idml/src/test/resources`, tag `v1.48.0`, commit `ce9f9edc2956da5868fcb71ae10426c7c6ae4e0b`) | Apache-2.0 (Okapi Framework) |
| `odf/guide.odt`, `odf/sales.ods`, `odf/slides.odp`, `odf/flow.odg` | `bilingua-odf/tools/gen_corpus.py` writes the OpenDocument XML by hand (stdlib only) | Apache-2.0, same as Bilingua |
| `bilingua-odf/tests/data/okapi/*` (15 OpenDocument packages; test fixtures, not corpus) | Copied unchanged from the Okapi Framework, https://gitlab.com/okapiframework/Okapi.git, tag `v1.48.0`, commit `ce9f9edc2956da5868fcb71ae10426c7c6ae4e0b`, folder `okapi/filters/openoffice/src/test/resources`; see the `NOTICE` there | Apache-2.0 (Okapi Framework) |
| `icml/okapi_Test01.wcml` (and its copy `bilingua-icml/tests/data/okapi_Test01.wcml`) | Copied unchanged from Okapi: `okapi/filters/icml/src/test/resources/Test01.wcml` in https://gitlab.com/okapiframework/Okapi.git, tag `v1.48.0`, commit `ce9f9edc2956da5868fcb71ae10426c7c6ae4e0b` | Apache-2.0 (Okapi Framework) |
| `vsdx/fixture.vsdx`, `vsdx/fixture.vsdm`, `vsdx/languages.vsdx`, `vsdx/rtl-paragraph.vsdx`, `vsdx/shiftjis.vsdx`, `vsdx/utf16.vsdx` | Written by the `write_corpus` test of `bilingua-vsdx` (hand-written XML parts, zipped in Rust) | Apache-2.0, same as Bilingua |
| `vsdx/okapi-2-pages.vsdx` (also `bilingua-vsdx/tests/data/2-pages.vsdx`) | Okapi Framework test resource `2-pages.vsdx`, copied unmodified from https://gitlab.com/okapiframework/Okapi.git (`okapi/filters/openxml/src/test/resources`, tag `v1.48.0`, commit `ce9f9edc2956da5868fcb71ae10426c7c6ae4e0b`) | Apache-2.0 (Okapi Framework) |
| `mdx/handbook.mdx`, `mdx/edge.mdx` | Written by hand for `bilingua-mdx` (invented text, YAML and TOML front matter, JSX, expressions, ES modules) | Apache-2.0, same as Bilingua |
| `latex/*.tex` (21 files: `features*`, `lumen_*` in 11 languages plus Latin-1 and UTF-16, `chapter_fragment`, `deep_braces`, `unclosed_*`) | `bilingua-latex/tests/tools/gen_corpus.py` writes the LaTeX by hand (stdlib only) | Apache-2.0, same as Bilingua |
| `bilingua-latex/tests/data/okapi/Test01.tex`, `Test02.tex`, `Test03.tex` (test fixtures, not corpus) | Okapi Framework test resources, copied unmodified (repository line endings, LF) from https://gitlab.com/okapiframework/Okapi.git (`okapi/filters/tex/src/test/resources`, tag `v1.48.0`, commit `ce9f9edc2956da5868fcb71ae10426c7c6ae4e0b`) | Apache-2.0 (Okapi Framework) |
| `email/*.eml` (34 messages; copies of `bilingua-email/tests/data/*.eml` minus `hard_wrapped.eml`) | `bilingua-email/tests/tools/gen_fixtures.py` writes them (stdlib only; invented people and text, `example.com` addresses) | Apache-2.0, same as Bilingua |
| `serverpages/*` (31 pages: `.phtml`, `.jsp`, `.jspf`, `.aspx`, `.ascx`, `.erb`, `.shtml`, `.shtm`, `.stm`; 11 languages, BOM, CRLF, CR, Windows-1252, Windows-1251, Shift_JIS, UTF-16) | `bilingua-serverpages/tests/tools/gen_corpus.py` writes them by hand (stdlib only; invented Lumen Bakery text) | Apache-2.0, same as Bilingua |
| `wordfast/lang_*`, `wordfast/edge_*` | hand-written TXML and TXLF in the 11 test languages and the four encodings (UTF-8, UTF-8 BOM with CRLF, UTF-16 LE, UTF-16 BE), plus edge cases (`seg-source`, nested inline codes, unpaired `ut`) | Apache-2.0, same as Bilingua |
| `wordfast/locos_sample.txml`, `wordfast/locos_sample.txlf` | The Localization OS Wordfast suite fixtures (`tests/fixtures/formats/wordfast`) | Apache-2.0 (Localization OS) |
| `ttml/lang_*`, `ttml/edge.ttml`, `ttml/dfxp_2006_04.dfxp`, `ttml/dfxp_2006_10.dfxp` (copies of `bilingua-ttml/tests/data`) | hand-written TTML and DFXP captions in the 11 test languages and the legacy encodings LocOS reads (Latin-1, Windows-1251/1254/1256, KOI8-R, Shift_JIS, EUC-JP, EUC-KR, GBK, Big5, UTF-16 LE/BE, UTF-8 BOM with CRLF, no declaration), plus edge cases (prefixed `tt:` elements, `xml:space`, nested spans, foreign elements, comments, PIs, CDATA); invented text | Apache-2.0, same as Bilingua |
| `ttml/locos_suite_tt.ttml` | The Localization OS TTML suite document (`TT` in `tests/verify_ttml_filter.py`), written out byte for byte | Apache-2.0 (Localization OS) |
| `wordfast/Test01.docx.txml`, `wordfast/Test02.html.txml`, `wordfast/Test03.mif.txml` | Okapi Framework test resources, copied unmodified from https://gitlab.com/okapiframework/Okapi.git (`okapi/filters/txml/src/test/resources`, tag `v1.48.0`, commit `ce9f9edc2956da5868fcb71ae10426c7c6ae4e0b`) | Apache-2.0 (Okapi Framework) |

Files from the Localization OS tree (its `sample-docs` folder and the hand-written PPTX and XLSX generators in its
test fixtures) are used by `python run.py --external` and read in place. Localization OS is open source under
Apache-2.0, so format crates may port its test cases and fixture files into their own `tests/data` folders.

Apart from the Okapi test resources listed above (Apache-2.0, unmodified, attributed per file), nothing was copied from Okapi or any other third-party test set. The LocOS parity corpus mentioned in the brief is
not present in the LocOS tree on this machine.

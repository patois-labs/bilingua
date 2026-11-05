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

Files from the Localization OS tree (its `sample-docs` folder and the hand-written PPTX and XLSX generators in its
test fixtures) are used by `python run.py --external` and read in place. Localization OS is open source under
Apache-2.0, so format crates may port its test cases and fixture files into their own `tests/data` folders.

Apart from the Okapi test resources listed above (Apache-2.0, unmodified, attributed per file), nothing was copied from Okapi or any other third-party test set. The LocOS parity corpus mentioned in the brief is
not present in the LocOS tree on this machine.

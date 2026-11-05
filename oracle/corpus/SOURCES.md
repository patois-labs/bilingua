# Corpus sources

Every file here is made up for Bilingua, except the third-party files named in the table (Lumen Bakery is a fictional company; no real client text).

| Files | How they were made | Licence |
|---|---|---|
| `docx/features`, `docx/languages`, `docx/runs`, `docx/edge`, `xlsx/menu`, `pptx/launch`, `html/*`, `md/*`, `json/*`, `po/*`, `srt/*`, `vtt/*`, `txt/*` | `gen_corpus.py` writes the XML and text by hand (stdlib only) | Apache-2.0, same as Bilingua |
| `docx/manual.docx`, `xlsx/products.xlsx` | `gen_office.py`, with python-docx 1.2 and openpyxl 3.1 (both MIT). The files contain those libraries' default package parts (styles, theme, settings) | Apache-2.0 for the invented text; the default parts come from MIT-licensed libraries |
| `icml/okapi_Test01.wcml` (and its copy `bilingua-icml/tests/data/okapi_Test01.wcml`) | Copied unchanged from Okapi: `okapi/filters/icml/src/test/resources/Test01.wcml` in https://gitlab.com/okapiframework/Okapi.git, tag `v1.48.0`, commit `ce9f9edc2956da5868fcb71ae10426c7c6ae4e0b` | Apache-2.0 (Okapi Framework) |

Files from the Localization OS tree (its `sample-docs` folder and the hand-written PPTX and XLSX generators in its
test fixtures) and a few internal Markdown notes are used by `python run.py --external`. They are **read in place and
never copied**; LocOS is proprietary, so nothing from it is committed. The report shows only counts for those files.

Apart from the Okapi test resource listed in the table (Apache-2.0, kept unchanged), nothing was copied from Okapi or any other third-party test set. The LocOS parity corpus mentioned in the brief is
not present in the LocOS tree on this machine.

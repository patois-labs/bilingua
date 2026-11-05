# bilingua-csv

CSV and TSV filter for Bilingua (`csv`, `tsv`, `tab`). A port of the Localization OS tabular filter, which is the
baseline; Okapi's `okf_table_csv` and `okf_table_tsv` are the second reference.

- The file is read record by record (RFC 4180, quote doubling, the way Python's `csv.reader` reads it) and the raw
  text of every record is kept. Records nothing is written into stay byte for byte; in a changed record only the
  written cell changes and every other field keeps its exact quoting. Delimiter (`,` `;` TAB `|`, detected as
  LocOS does), quote style and each record's own line ending are kept.
- Which cells are units: the column mapping (`source=English, target.de=German, key=id, context=notes,
  maxlen=limit, header=yes|no|auto, delimiter=...`, names or 1-based numbers) or, without one, the header row
  (`source`/`text` or the source locale, `target`/`translation` or a target locale, key and context columns become
  notes). With neither, every text cell is translated where it stands. A cell is a unit when it has a letter and is
  not a formula; written cells starting with `= + - @`, TAB, CR or an apostrophe get an apostrophe in front.
- A target column that already holds text keeps it when the unit has no translation of its own.
- UTF-8 (BOM kept), UTF-16 with BOM, and the legacy code pages `bilingua-model` reads; a legacy file goes back in its
  page when everything fits, else UTF-8 with a BOM.
- Limits: 1000 columns, 131072 characters per field (Python's csv limit), the model's unit, unit-length, line and
  input limits.

Tests (`cargo test -p bilingua-csv`):

| File | What it checks |
|---|---|
| `tests/locos_suite.rs` | the LocOS suite `verify_csv_filter.py`, case by case (13 Rust tests for its 14 test functions; the XLSX column mode, the HTTP API and CLI, and the glossary and TM importers belong to other layers) |
| `tests/locos_golden.rs` | 811 recorded LocOS runs (random files in all encodings and delimiters, every kind of column mapping, hostile bytes, the Shopify fixture): same units, same notes, same bytes for five translation maps, same refusals (`tests/tools/gen_golden.py`) |
| `tests/okapi_table.rs` | Okapi 1.48.0 on its own table test resources (`tests/data/okapi/`, `tests/tools/okapi_compare.py`): Bilingua equals LocOS on all of them, the Okapi comparison is printed |
| `tests/roundtrip.rs` | the corpus (36 files) byte exact and pseudo-translated, 11 languages, 10 legacy code pages, BOM and line endings, paragraph and sentence mode, locked units, serde, parser stamp, detection |
| `tests/hostile.rs` | proptest over random bytes, random text, CSV-shaped lines, mapping specs and mutated corpus files; deep, wide and huge inputs |
| `tests/speed.rs` | a 5 MB file, `cargo test -p bilingua-csv --release --test speed -- --nocapture`, then `tests/tools/speed_locos.py` |

Known differences from LocOS: Python's `str.isalpha` is letters only; Rust has no general-category tables in `std`,
so a cell whose only "letters" are combining marks or letter numbers can differ. A GBK file is written with
GB18030, so a character GBK lacks stays in the file's code page (LocOS switches to UTF-8 with a BOM).

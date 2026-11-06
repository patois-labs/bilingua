# bilingua-oracle

Checks Bilingua's parsers against the Localization OS Python filters (decision D16: LocOS is an oracle, never a
dependency) and against Bilingua's own round trip.

| File | What it is |
|---|---|
| `src/main.rs` | `bl-dump FILE [MERGED_OUT]`: prints Bilingua's units as JSON, checks that an unchanged merge gives the same file, pseudo-translates every unit, merges, reopens, and optionally keeps the merged file |
| `locos_dump.py` | Run with the LocOS venv Python. Imports `app.filters` read-only and prints its units; also opens DOCX and XLSX with python-docx and openpyxl |
| `run.py` | Runs both sides over `corpus/`, normalises, compares, and writes `REPORT.md` |
| `gen_corpus.py`, `gen_office.py` | Build the made-up corpus (see `corpus/SOURCES.md`) |
| `scoreboard.py`, `scoreboard_text.py` | `python run.py --scoreboard` writes `SCOREBOARD.md`: every LocOS format against Bilingua, one row each (files, units on both sides, text match, round trip, status), LocOS fixtures read in place |
| `findings_text.py` | The hand-written conclusions copied into `REPORT.md` |
| `tests/corpus.rs` | `cargo test -p bilingua-oracle`: round trip on every corpus file, no LocOS needed |

```
cargo build -p bilingua-oracle
python run.py                 # corpus only
python run.py --scoreboard    # regenerate SCOREBOARD.md (needs core/target/debug/catcore-cli built for the bilingual formats)
python run.py --external      # also files that live only in the LocOS tree (read in place, counts only in the report)
```

Split scoreboard run, for a PC with LocOS but no cargo and a build machine (the M5) with cargo but no LocOS:

```
PC:  python run.py --scoreboard --locos-side DIR       # LocOS inventory, dialects and units; LocOS-tree files copied to DIR/ext
     (copy DIR to the M5; the M5 checkout must be at the same commit, corpus files are read from it)
M5:  python3 run.py --scoreboard --build --bilingua-side DIR   # bl-dump --formats and bl-dump on every listed file
     (copy DIR/bl.json back)
PC:  python run.py --scoreboard --from DIR             # renders SCOREBOARD.md from DIR/locos.json + DIR/bl.json
```

`DIR` is scratch, never inside the repository.

`run.py` needs Python 3.9+ and a Localization OS checkout, by default a `local-tms` folder next to this repository (set `LOCOS_DIR` to change it).
LocOS is never edited, started or written to; its data directory is a throwaway.

Not done: the comparison does not judge segmentation (LocOS splits sentences in another layer), and it cannot tell
a formatting code from a placeholder, so code counts are shown but not compared.

# bilingua-htmlhelp

Microsoft HTML Help 1.x project files for Bilingua: contents (`.hhc`), index (`.hhk`) and project (`.hhp`).

- **hhc / hhk**: the `value` of every `<param name="Name">` and `<param name="Keyword">` inside an
  `<OBJECT type="text/sitemap">`. Local, URL, ImageNumber, Merge, Comment, the `text/site properties` object and all
  markup stay as they are. A `See Also` value follows the translation of the keyword it names.
- **hhp**: `Title=` in `[OPTIONS]` and the caption of each `[WINDOWS]` definition; `Language=` becomes the target
  language's LCID on a target-language merge.
- **Encodings**: HTML Help 1.x files are in the Windows ANSI code page of the help language. Read as UTF-8/UTF-16 with a
  byte-order mark, valid non-ASCII UTF-8, a declared meta charset, the project's `Language=` code page (a hint you pass
  in), else Windows-1252. Written back in the same encoding; a target-language merge writes the target language's code
  page and rewrites the meta charset and the site-properties `Font` charset. A character the code page cannot hold
  refuses the merge unless `MergeOptions::utf8_fallback` is set.
- Python's codec behaviour (what Localization OS writes) is reproduced where encoding_rs differs: see
  `tests/tools/gen_encdiff.py` and `gen_tables.py`, which make the generated `src/encdiff.rs` and `src/data.rs`.

Tests: the Localization OS suite ported one to one (`tests/locos_suite.rs`), a differential test against 580 files
LocOS produced goldens for (`tests/locos_golden.rs`), round trips over `oracle/corpus/htmlhelp`, hostile input and
property tests, and a speed test. See `WIRING.md` for the facade lines.

```
cargo run -p bilingua-htmlhelp --example htmlhelp_dump -- FILE [TARGET] [OUT]
```

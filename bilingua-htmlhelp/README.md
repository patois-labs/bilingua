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
  refuses the merge unless the format option `htmlhelp.utf8_fallback` was set at extract. `bilingua_model::MergeOptions`:
  `stamp_target_always` localizes with nothing translated (LocOS `build_localized`), `output_encoding: Utf8` writes
  UTF-8 with the meta charset rewritten (LocOS writes the ANSI code page instead).
- **Empty files**: a valid `.hhc` / `.hhk` with no entry names, or a `.hhp` with no `Title=` and no window caption, opens
  with zero units and a `no_translatable_text` warning, and merges back to the original bytes (LocOS refuses it as an
  empty document). A file with no HTML Help structure (zero bytes, whitespace, plain text: no `<UL>` or `<OBJECT>` in a
  sitemap, no `[section]` in a project) and malformed files are still refused.
- **Browser check** (LocOS `html_guard`, #676): a translated value is written opaque where a browser would not read
  it as that attribute value, and a merge whose sitemap a browser would read into different elements is refused.
- Python's codecs (what Localization OS reads and writes) are reproduced where encoding_rs differs, both ways, for all
  27 codecs a file may be read with (cp932 NEC/IBM rows, Python's strict `gb2312` / `shift_jis` / `big5` / `euc_kr`,
  EUC-KR make-up sequences, EUC-JP JIS X 0212, real ISO-8859-9, ...): `tests/tools/gen_encdiff.py`,
  `gen_decdiff.py` and `gen_tables.py` make the generated `src/encdiff.rs`, `src/decdiff.rs` and `src/data.rs`;
  `check_decode.py` and `check_encode.py` (with the `htmlhelp_decmap` / `htmlhelp_encchk` examples) compare the crate
  end to end with LocOS. A decode that is not one to one (cp932 duplicates) keeps the original bytes in the skeleton,
  so an untouched merge is byte-exact.

Tests: the Localization OS suite ported one to one (`tests/locos_suite.rs`), a differential test against 580 files
LocOS produced goldens for (`tests/locos_golden.rs`), round trips over `tests/fixtures/corpus`, hostile input and
property tests, and a speed test. See `WIRING.md` for the facade lines.

```
cargo run -p bilingua-htmlhelp --example htmlhelp_dump -- FILE [TARGET] [OUT]
```

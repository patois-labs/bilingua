# Wiring for the lead

- Workspace: add `"bilingua-dita"` to `members` and `bilingua-dita = { path = "bilingua-dita" }` to
  `[workspace.dependencies]` (the last commit of this branch does it for building; replace at merge). Dependencies:
  `bilingua-model`, `bilingua-xml` (workspace); dev: `proptest = "1"`, `serde_json`.
- Facade `Cargo.toml`: `bilingua-dita = { workspace = true }`. Filter list: `all.extend(bilingua_dita::filters())`
  (or `Box::new(bilingua_dita::Dita::new())`, `Box::new(bilingua_dita::DocBook::new())`) **just before
  `bilingua_xml::XmlFilter::new()`**, after the other XML vocabularies (xslt, ttml, sdlproj, ...).
- Format ids and extensions: `dita` = `dita`, `ditamap`, `bookmap`; `docbook` = `dbk`, `docbook` (LocOS registers
  the same, `docbook` being the format name it also accepts as an extension). A `.xml` DITA or DocBook file stays with
  the generic `xml` filter by extension, which recognises both and gives the same units, as in LocOS.
- **Sniff is strict**: DITA = well-formed XML whose DOCTYPE contains `DITA` or whose root has
  `ditaarch:DITAArchVersion`; DocBook = DOCTYPE contains `DocBook` or the root is in `http://docbook.org/ns/docbook`.
  A cheap check of the first 8 KB comes first (UTF-16 files skip it). A bare `<topic>` root without either is left to
  the generic filter, which reads it with the DITA rules anyway. The two sniffers never both accept a file in
  practice (one DOCTYPE), DITA is listed first.
- Contract test `crate_of`: `"dita" | "docbook" => "bilingua-xml"`. The skeleton is bilingua-xml's (its parser
  record and `SKELETON_SCHEMA`), since that crate writes and reads it; this crate only delegates. Documents carry
  `format = "dita"` / `"docbook"`, so `filter_for` routes the merge here.
- Options: none (`dita.*` / `docbook.*` keys are refused as unknown). `Dita::with_its_rules` /
  `DocBook::with_its_rules` take a user's `<its:rules>` file (LocOS `DitaFilter(rules=...)`); the facade does not
  need it. Limits: `ExtractOptions::limits` as in bilingua-xml.
- LocOS adapter: tags map one to one (`Open`/`Close` = `g`, `Standalone` = `x`), no free tags (LocOS keeps `{0}`,
  `%s` as text in XML), so no `placeholder` labels. Code ids starting with `~` are hidden codes to skip. Under the
  LocOS profile, `stamp_target_always` gives LocOS's stamp on an untouched merge and `OutputEncoding::Utf8` its UTF-8.
- Oracle: `oracle/corpus/dita/` holds eight made-up files (`tests/tools/gen_corpus.py`); add `dita`, `ditamap`,
  `bookmap`, `dbk`, `docbook` to `FORMATS` in `run.py` (LocOS side: `get_filter(ext)`).
- Portable check: add `dita` to `CRATES` in `tools/check-portable.sh` and `.ps1` (the last commit does it).
- Python binding: nothing new beyond the facade's list.

# Wiring bilingua-idml (the lead replaces the wire commit at merge)

The wire commit (`wire: idml (lead replaces at merge)`) already makes these edits; they are listed so the lead can
redo them by hand.

- Format id: `idml`. Extension: `idml`. Crate: `bilingua-idml`, type `bilingua_idml::Idml` (unit struct).
- `filters()` in `bilingua/bilingua/src/lib.rs`: `Box::new(bilingua_idml::Idml)`.
  **Sniff is strict**: a ZIP that holds the IDML mimetype text, or `designmap.xml` together with `Stories/`.
  It can sit anywhere among the other packages (before docx is fine); it does not claim other zips.
- Facade `Cargo.toml`: `bilingua-idml = { workspace = true }`. Workspace `members` and `[workspace.dependencies]`.
- `bilingua/tests/contract.rs` `crate_of`: `"idml" => "bilingua-idml"`.
- `bilingua/tools/check-portable.sh` and `.ps1`: `idml` added to `CRATES`.
- Options: none of its own. `target_lang` drives the language stamp and the right-to-left marks on merge.
- Corpus: `bilingua/oracle/corpus/idml/` holds 7 small packages that are **Okapi test resources** (Okapi Framework
  v1.48.0, Apache 2.0, `okapi/filters/idml/src/test/resources`): `03-hyperlink-and-table-content`, `07-paragraph-breaks`,
  `08-conditional-text-and-tracked-changes`, `09-footnotes`, `10-tables`, `11-xml-structures`, `1415-adjacent-codes`.
  `oracle/corpus/SOURCES.md` says nothing was copied from Okapi; it needs one row for these (Apache 2.0, unmodified).
  Drop the folder if Brent would rather not carry third-party files.

## LocOS adapter notes

- Unit ids are `p<N>` = LocOS `p_index`. Tag ids are the LocOS token numbers. Letter by `CodeData.label`: `run` is
  `r`, any other paired tag is `g`, every standalone tag is `x`. No tag is a `p` (free) token; no placeholder is
  flattened to text (tabs and line breaks are real `x` tokens in LocOS too).
- LocOS `build()` (no target) maps to `merge` with `target_lang: None`; `build_localized()` to `target_lang: Some(..)`.
- LocOS stamps `AppliedLanguage` and right-to-left marks only on paragraphs it rebuilds; so does this crate. The
  "stamp even when nothing is translated" option does not apply to IDML (nothing is rebuilt then), and an
  untranslated merge returns the original file in both.
- A unit counts as translated when any of its segments has a target; `Unit::output()` is what is written.

# Wiring bilingua-ftl (the lead replaces the throwaway wire commit at merge)

- Format id: `ftl`. Extension: `ftl`. Filter type: `bilingua_ftl::FtlFilter` (unit struct, `Filter` impl).
- Facade `filters()`: add `Box::new(bilingua_ftl::FtlFilter)` after `Properties` and before HTML and text.
  **Loose sniffer**: Fluent looks like `key = value`, so `sniff` demands a Fluent-only marker (`{ $var }`, `{ -term }`,
  `*[other]`, a `-term =` entry, an indented `.attribute =`, a `##` group comment) plus one parsed entry. The
  extension `.ftl` is the normal route; the sniffer only matters for extension-less input. Keep it last of the
  key-value formats.
- Facade `Cargo.toml`: `bilingua-ftl = { workspace = true }`. Workspace: member `bilingua-ftl` and
  `bilingua-ftl = { path = "bilingua-ftl" }`. Dev-dependency `proptest = "1"` is crate-local (same as ini).
- `bilingua/tests/contract.rs` `crate_of`: `"ftl" => "bilingua-ftl"`.
- `tools/check-portable.sh` and `.ps1`: add `ftl` to `CRATES`.
- Options: none in `ExtractOptions`. `include_locked` is ignored (Fluent has no locked text). The one LocOS option,
  `FTL_TRANSLATE_TERMS`, is `Limits::translate_terms` on `FtlFilter::extract_with(bytes, opts, &Limits)`; the crate
  never reads the environment, so the LocOS adapter passes it. `Filter::extract` takes its numbers from
  `ExtractOptions::limits` (B6 `bilingua_model::Limits`: 100 MB, 50000 units, 100000 characters per unit, the line
  budget) and refuses with `Error::Limit`; the 40-level nesting cap and the placeable budget (20 per allowed unit)
  are the crate's own and refuse with `Error::Unsupported`, as LocOS.
- Target locale: `FtlFilter::merge_localized(doc, locale)` (locale defaults to `doc.target_lang`) also fills plural selects with the target
  language's missing CLDR categories (LocOS `build_localized`); plain `merge` never adds units (the oracle and the facade contract reopen a merged file and count units). The tables are in `src/plural_data.rs`, made from
  babel by `tests/tools/gen_plurals.py`.
- Inline tags: every placeable is a free standalone tag, `label` = `placeholder`, `display` = the source text, so the
  LocOS adapter flattens them to plain text as LocOS has them. Brace literals (`{ "{" }`) are plain text.
- Extra public API: `bilingua_ftl::structure(bytes)` (counts of messages, terms, attributes, Junk).
- Oracle corpus: `oracle/corpus/ftl/` (15 files, made by `tests/tools/gen_corpus.py`).
- Python wheel: nothing format-specific; it goes through the facade.

# Wiring for the lead

Done in the wire commit (replace as you merge): workspace `members` and `[workspace.dependencies]`, the facade
dependency, the four `filters()` entries, the four `crate_of` rows in `bilingua/tests/contract.rs`, `Cargo.lock`, the
`CRATES` lists of `tools/check-portable.sh` and `.ps1`, and the corpus folder `oracle/corpus/richtext/` (4 files).

- Filters (all in `bilingua_richtext`, unit structs, `Default`): `Contentful` (id `contentful`, extension
  `contentful`), `Sanity` (`sanity`, `portabletext`), `Adf` (`adf`, `adf`), `Notion` (`notion`, `notion`).
- `sniff`: strict. Each parses the whole JSON (UTF-8 only) and needs its exact shape (`nodeType: document`; an array with
  a `_type: block` having `children`; `type: doc` with a `content` array; an array or `results` of Notion blocks).
  They can sit anywhere; put them before `JsonFilter` so a no-extension file of these shapes is not taken as plain JSON
  (JSON's own sniff accepts any object or array). Plain `.json` stays with `bilingua-json`; the adapter picks these by
  format id or by the connector's field type.
- Options: `adf.units` (text, `inline` or `leaf`, default `inline`), declared by `Adf::format_option_keys()` (also
  `bilingua_richtext::ADF_OPTIONS`). `ExtractOptions::profile = locos`: `context.extra["locos.p_index"]` is the unit's
  position (the LocOS walker index); nothing else changes.
- Skeleton: `Skeleton::Text` with parts `[Raw(mode), Raw(original text)]`, encoding tag from `enc::decode_text`.
- Inline codes: paired `g` and standalone `x` only, none `free`, none a flattenable placeholder; labels are the marks
  (`bold`, `strong link`, `entry hyperlink`, `code`, `mention`, ...).
- Hidden/extra: `context.kind` = node type (`paragraph`, `heading-2`, `list-item`, `caption`, `cell`), `context.key` =
  JSON pointer, `extra["block_id"]` (Notion), `extra["_key"]` (Sanity).
- `corpus/SOURCES.md`: row `richtext/*` made up by hand for Bilingua (Apache-2.0); add `FORMATS` entries in `oracle/run.py`
  for `contentful portabletext adf notion` if wanted.

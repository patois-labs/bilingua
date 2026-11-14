# Wiring for the lead

Done in the wire commit (replace as you merge): workspace `members` and `[workspace.dependencies]`, the facade
dependency, the four `filters()` entries, the four `crate_of` rows in `bilingua/tests/contract.rs`, `Cargo.lock`, the
`CRATES` lists of `tools/check-portable.sh` and `.ps1`, and the corpus folder `oracle/corpus/richtext/` (4 files).

- Filters (all in `bilingua_richtext`, unit structs, `Default`): `Contentful` (id `contentful`, extension
  `contentful`), `Sanity` (`sanity`, `portabletext`), `Adf` (`adf`, `adf`), `Notion` (`notion`, `notion`).
- `sniff`: strict. Each parses the whole JSON (UTF-8 only) and needs its exact shape (`nodeType: document`; an array with
  a `_type: block` having `children`; `type: doc` with a `content` array; an array or `results` of Notion blocks).
  Every file they accept is also plain JSON, which opened as `json` before this crate existed, so the four filters go
  **last** in `filters()` (gate decision 11-06): detection of every existing file is unchanged (checked on the M5 over
  every json and extensionless file of the corpus, goldens, test data and the fuzz sets), and these formats are chosen
  by their extensions or by format id (the adapter picks them by the connector's field type). `extract` is looser than
  `sniff`: it takes what the LocOS walker takes (any array for Sanity, any block list or `results` object for Notion,
  an object typed `doc` for ADF, a `document` for Contentful).
- Options: `adf.units` (text, `inline` or `leaf`, default `inline`), declared by `Adf::format_option_keys()` (also
  `bilingua_richtext::ADF_OPTIONS`). `ExtractOptions::profile = locos`: `context.extra["locos.p_index"]` is the unit's
  position (the LocOS walker index); nothing else changes.
- Skeleton: `Skeleton::Text` with parts `[Raw(mode), Raw(original text)]`, encoding tag from `enc::decode_text`.
- Inline codes: paired `g` and standalone `x` only, none `free`, none a flattenable placeholder; labels are the marks
  (`bold`, `strong link`, `entry hyperlink`, `code`, `mention`, ...).
- Hidden/extra: `context.kind` = node type (`paragraph`, `heading-2`, `list-item`, `caption`, `cell`), `context.key` =
  JSON pointer, `extra["block_id"]` (Notion), `extra["_key"]` (Sanity).
- `corpus/SOURCES.md`: row `richtext/*` added. `oracle/run.py`: one `FORMATS` block for `contentful portabletext adf
  notion`; their names (`Contentful`, `Portable Text`, `Jira ADF`, `Notion`) still need adding to `ORDER` for the report
  to show them (left out to keep the run.py change to one block). LocOS has no file filter for them, so the oracle shows
  a LocOS error row; the walker differential is in the gate report.

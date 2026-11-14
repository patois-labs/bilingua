# Wiring for the lead

- Workspace: add `"bilingua-vsdx"` to `members` and `bilingua-vsdx = { path = "bilingua-vsdx" }` to
  `[workspace.dependencies]`. The crate needs `bilingua-model` and `bilingua-zip`, `serde`, `serde_json`;
  dev-dependency `proptest = "1"` (already in use).
- Facade: dependency `bilingua-vsdx = { workspace = true }`; filter `Box::new(bilingua_vsdx::Vsdx)` (a unit struct).
  Comments are a separate call, `Vsdx.extract_with_comments(..)` (LocOS: the project's "translate comments" option;
  the model has no field for it).
- Format id: `vsdx`. Extensions: `vsdx`, `vsdm` (LocOS aliases only these two).
- `sniff`: loose-ish, a zip (`PK\3\4`) whose bytes contain `[Content_Types].xml` and `visio/`. docx, pptx and xlsx
  need `word/`, `ppt/`, `xl/`, so there is no overlap; it can sit next to those.
- Options used: `target_lang` (LangID and paragraph direction, only for translated shapes), `source_lang` (only for
  the legacy decode path), `limits`. `include_locked` is not used.
- Skeleton: `Skeleton::Zip`, parser stamp schema 1. Zip metadata and the encoding of non-UTF-8 parts are in the
  `\0bilingua-vsdx-meta` entry. Hidden unit codes start with `_` (`_orig`, `_pre`, `_suf`, `_k`, `_q`, `_crlf`,
  `_shape`).
- Inline tags: standalone, labels `character properties`, `paragraph properties`, `tab properties`, `field`
  (never `placeholder`: LocOS tokenises them all as `x`).
- Oracle: `oracle/corpus/vsdx/` holds 7 files (one is Okapi's `2-pages.vsdx`, Apache 2.0); add `vsdx`/`vsdm` to
  `FORMATS` in `run.py`.
- Model needs: an `ExtractOptions` flag for "translate comments"; the "stamp the language even when nothing is
  translated" merge option.

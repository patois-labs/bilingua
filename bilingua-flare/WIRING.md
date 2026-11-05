# Wiring for the lead

One filter: `bilingua_flare::Flare` (id `flprjzip`, extension `flprjzip`; a plain `.zip` is deliberately not claimed,
as in LocOS).

- Workspace: `"bilingua-flare"` in `members`, `bilingua-flare = { path = "bilingua-flare" }` in
  `[workspace.dependencies]` (done in the wire commit, replace at merge). Normal dependencies: `bilingua-model`,
  `bilingua-zip`, `bilingua-html` (used as a library, not edited), `quick-xml`, `regex`, `serde`, `serde_json`.
  Dev-dependencies: `proptest = "1"`, `zip`, `quick-xml`, `bilingua-seg`.
- Facade `Cargo.toml`: `bilingua-flare = { workspace = true }`.
- Facade `filters()`: `Box::new(bilingua_flare::Flare)` next to `Epub`, before `Docx`. **Sniff is strict**: `PK\x03\x04`
  and a central-directory name ending `.flprj`, `.flvar`, `.fltoc`, `.flglo` or `.flsnp`. Office, EPUB, ODF and the
  other packages never have those names, so its place does not matter; detection by extension is `flprjzip`.
- Facade `own_segmentation`: nothing to add (the units are paragraphs; sentence segments are written back in order,
  tested with `bilingua-seg`).
- `bilingua/tests/contract.rs`: `"flprjzip" => "bilingua-flare"` in `crate_of`.
- `tools/check-portable.sh` and `.ps1`: `flare` in `CRATES`.
- Skeleton: `Skeleton::Zip` with one raw entry `flprjzip` holding the whole original file and one `flprjzip-options`
  entry (include_locked and the limits it was read with); merge opens the project again. Schema 1.
- Options: none of its own. `target_lang` on the `Document` picks the language a merge declares (only when at least
  one unit is translated, or with `MergeOptions::stamp_target_always`). `include_locked` also lists
  `translate="no"` text as locked units. Output encoding: Flare is UTF-8 only, so `OutputEncoding` has no effect
  (a UTF-8 byte-order mark is kept).
- Unit ids: `m<member>-u<k>` (topic text and accessibility attributes, ids of the HTML filter), `m<member>-k<n>`
  (index keywords), `m<member>-x<n>` (variable sets, TOCs, glossaries); `<member>` is the zip member number.
  `context.style` is `flare:index-keyword`, `flare:variable`, `flare:toc-title`, `flare:glossary-term`,
  `flare:glossary-definition`; an accessibility attribute has `context.extra["locos-style"]` (`a11y:img-alt`, ...).
- Inline codes: paired MadCap elements (`MadCap:xref`, `conditionalText`, `glossaryTerm`, ...) are `Open`/`Close`
  with the element name as label and display; `MadCap:variable` (with its cached content), `snippetText`, `keyword`
  and `concept` are `Standalone`; `[%=System.LinkedTitle%]` in a TOC title is `Standalone` (label `variable`, display
  the exact text). Every one maps one to one onto the LocOS `g`/`x` tokens. None needs the `placeholder` label.
- Oracle: `oracle/corpus/flprjzip/` (three made-up projects, `tools/gen_corpus.py`); add `flprjzip` to `FORMATS` in
  `oracle/run.py` if it is not there. `.flprjzip` files are binary zips: the facade finds them by extension.
- Needs from `bilingua-model` / `bilingua-zip`: see README ("What other crates would need").

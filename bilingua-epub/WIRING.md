# Wiring for the lead

One filter: `bilingua_epub::Epub` (id `epub`, extension `epub`).

- Workspace: `"bilingua-epub"` in `members`, `bilingua-epub = { path = "bilingua-epub" }` in `[workspace.dependencies]`
  (done in the wire commit, replace at merge). Normal dependencies: `bilingua-model`, `bilingua-zip`,
  `bilingua-html` (used as a library, not edited), `regex`. Dev-dependencies: `proptest = "1"`, `zip`, `quick-xml`,
  `bilingua-seg`, `serde_json`.
- Facade `Cargo.toml`: `bilingua-epub = { workspace = true }`.
- Facade `filters()`: `Box::new(bilingua_epub::Epub)` next to `Xcloc`, before `Docx` (any place before the loose
  sniffers works). **Sniff is strict**: a `PK\x03\x04` zip that shows `application/epub+zip` in the clear (the normal
  stored `mimetype`), or one that names `META-INF/container.xml` and an `.opf`. Docx needs `[Content_Types].xml` and
  `word/`, so the two never claim the same file.
- Facade `own_segmentation`: nothing to add. The units are plain paragraphs and sentence segments are written back in
  order (tested with `bilingua-seg`).
- `bilingua/tests/contract.rs`: `"epub" => "bilingua-epub"` in `crate_of`.
- `tools/check-portable.sh` and `.ps1`: `epub` in `CRATES`.
- Skeleton: `Skeleton::Zip` with one raw entry named `epub` that holds the whole original file; merge opens it again
  and writes only what changed (the same shape as `xcloc`). Schema 1.
- Options: none. `target_lang` on the `Document` picks the language a merge declares (only when at least one unit is
  translated, lead decision 11-04). `include_locked` also lists `translate="no"` text as locked units.
- Inline codes: the HTML filter's own (`Open`/`Close` for `b`, `a`, `span`...; standalone for `br`, `img` and for
  protected `code`/`kbd`/`samp`/`translate="no"` elements). None of them needs the `placeholder` label: `{count}` and
  `%1$s` are plain text in LocOS and here.
- Unit ids: `opf-<n>` (`dc:title`, `dc:description`), `h<chapter>-u<k>` (XHTML, the nav document is the last
  chapter), `ncx-<n>`. A unit's `context.extra["locos-style"]` is the LocOS style of an accessibility attribute
  (`a11y:img-alt`, `a11y:link-title`, ...); `context.style` is `epub:title`, `epub:description`, `epub:toc-label` for the
  OPF and NCX units, and `context.extra["epub-version"]` is the package version on the OPF units.
- Oracle: `oracle/corpus/epub/` (four made-up books, `tools/gen_corpus.py`); add `epub` to `FORMATS` in
  `oracle/run.py` and the LocOS name in `scoreboard.py` (not touched here). `.epub` files are binary zips: the
  facade finds them by extension.
- Needs from `bilingua-model`: none required. A merge option "stamp the language even when nothing is translated"
  (LocOS behaviour, for the adapter) and "always write UTF-8" would apply here too (the OPF/NCX/XHTML language
  stamps and the per-member encodings).

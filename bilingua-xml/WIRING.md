# Wiring for the lead

- Workspace: add `"bilingua-xml"` to `members` and `bilingua-xml = { path = "bilingua-xml" }` to
  `[workspace.dependencies]` (the last commit of this branch does it for building; replace at merge). The crate uses
  `quick-xml` (workspace); dev-dependencies are `proptest = "1"` and `serde_json`.
- Facade filter list: `Box::new(bilingua_xml::XmlFilter::new())`. Facade `Cargo.toml`: `bilingua-xml = { workspace = true }`.
  Contract test `crate_of`: `"xml" => "bilingua-xml"`.
- Format id `xml`. Extensions: `xml`.
- **Sniff is loose**: true for any well-formed XML document without a DTD internal subset (it parses the whole file,
  UTF-8 or a UTF-16 BOM). Put it **after every filter that owns a specific XML vocabulary** (XLIFF family, SDL packages,
  resx, resw, xaml, wxl, Qt ts, plist, xliff-like) and before the HTML and plain text filters. Detection by extension is
  exact.
- Later XML formats do not need a new parser: `bilingua_xml::XmlFilter::with_id("dita", &["dita", "ditamap", "bookmap"]).dialect(bilingua_xml::Dialect::dita())`
  and `with_id("docbook", &["dbk"]).dialect(Dialect::docbook())` are what the LocOS DITA and DocBook filters are. A
  `.dita` or `.dbk` file also works through the generic `xml` filter, which recognises both.
- Options: none needed. `ExtractOptions::limits` applies (units, unit chars, XML depth and elements, input bytes).
  `target_lang` on the document decides the root language written on merge (only when something is translated).
  Multilingual layout: `XmlFilter::new().multilingual(MlConfig::from_json(..)?)`, or the in-file
  `<?loc-multilingual ...?>`; `XmlFilter::inspect(bytes)` gives the dialect and layout (the LocOS skeleton's
  `dialect` / `multilingual` keys).
- Skeleton: `Skeleton::Text`, encoding tag from `bilingua_model::enc` (`utf-8`, `utf-8-bom`, `utf-16le;label=UTF-16`,
  `windows-1252;label=windows-1252`, ...). Raw parts and unit codes contain NUL-delimited markers (README, "Skeleton");
  code ids starting with `~` are hidden codes the LocOS adapter must skip. Skeleton schema 1.
- LocOS adapter notes: tags map one to one (`Open`/`Close` = `g`, `Standalone` = `x`; no `free` tags, LocOS keeps
  `{0}`, `%s` and the like as plain text in XML); a point tag's display is the element's raw markup or `<name>...</name>`.
- Oracle: `oracle/corpus/xml/` holds the made-up corpus (15 files, `.xml` only so the facade picks this filter); add
  `xml` to `FORMATS` in `run.py`. The LocOS side of the oracle (`get_filter("xml")`) is the dispatcher that sends
  Android `strings.xml` and Java XML properties to their own filters; none of the corpus files has that root.
- Portable check: add `xml` to `CRATES` in `tools/check-portable.sh` and `.ps1` (the last commit does it).

# Wiring for the lead

- Workspace: `"bilingua-rst"` in `members` and `bilingua-rst = { path = "bilingua-rst" }` in `[workspace.dependencies]`
  (done in the wire commit). The crate needs only `bilingua-model` (no `regex`: the inline scanner is hand written).
  Dev-dependencies: `proptest = "1"`, `serde_json` (both already in use).
- Facade: dependency `bilingua-rst = { workspace = true }`; filter `Box::new(bilingua_rst::Rst)` (a unit struct).
- Format id: `rst`. Extensions: `rst`, `rest` (LocOS: `rst` with the alias `rest`).
- `sniff`: LOOSE. reStructuredText has no signature: it accepts text with a `.. name::` directive line, a role
  (`:ref:`, `:doc:`, `:class:`), or a title line with an underline of at least its width (3+ punctuation characters) in
  the first 8 KB. A Markdown setext heading or a plain-text file with an underlined line also passes. It belongs
  late in `filters()`: after every strict sniffer and the generic XML filter, right before HTML and plain text (the wire
  commit puts it there). Files with an extension are routed by extension, so only extensionless files depend on it.
- Options used: `source_lang` (picks the code page of an undeclared non-UTF-8 file), `limits`. `target_lang` and
  `include_locked` change nothing (no language declaration in the format; LocOS has no locked units for rst).
- Skeleton `encoding`: the shared `bilingua_model::enc` tags (`utf-8`, `utf-8-bom`, `utf-16le`, `utf-16be`, an
  `encoding_rs` name, `;nl=crlf|cr`).
- Hidden codes on a unit: `~raw` (the unit's region as written: a paragraph, a title with its adornment lines, a whole
  table on its first cell unit), `~kind` (`para`, `span`, `title`, `grid`, `simple`, `cell`), `~title`, `~cell`,
  `~line`, `~n`. A table is rewritten as a whole from its first cell unit (`~kind` `grid` or `simple`); the other cell
  units (`~kind` `cell`) have skeleton parts that write nothing. The unit order is LocOS's (body in document order,
  then the `.. meta::` units).
- Inline codes: pairs for emphasis, strong and embedded-URI references; everything else a standalone tag, order kept.
  Labels in the README; none is `placeholder`, because LocOS tokenises every one of them.
- Warnings: `rst_skipped_table` (part `line N`) for each table kept verbatim; LocOS keeps these in
  `skeleton["rst_skipped_tables"]`.
- Merge refusal: a table cell that does not fit is `Error::Write("reStructuredText table at line N: ...")`. LocOS
  raises `TableFitError`, an `InputLimitError`; the adapter should map this message to it (a `LimitKind` for it would be
  cleaner, see the report).
- Oracle: `oracle/corpus/rst/` holds the made-up corpus (8 files); add `rst` and `rest` to `FORMATS` in `run.py`.
- Merge options still wanted from the model: none for rst (LocOS neither stamps a language nor re-encodes: `build`
  keeps the BOM and writes UTF-8 only through `encode_text_lossless`, which the shared envelope covers).

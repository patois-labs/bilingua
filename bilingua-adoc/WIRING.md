# Wiring for the lead

- Workspace: add `"bilingua-adoc"` to `members` and `bilingua-adoc = { path = "bilingua-adoc" }` to
  `[workspace.dependencies]`. Needs `bilingua-model` and `regex`; dev-dependencies `proptest` and `serde_json`.
- Facade: dependency `bilingua-adoc = { workspace = true }`; filter `Box::new(bilingua_adoc::Adoc)` (a unit struct).
- Format id: `adoc` (LocOS `fmt`). Extensions: `adoc`, `asciidoc`, `asc`. `crate_of` row: `"adoc" => "bilingua-adoc"`.
- `sniff`: never (always false), like Markdown: the extension decides. Position in `filters()` does not matter; the
  wire commit puts it right after Markdown. An `.asc` file that is an OpenPGP key or signature is refused with a
  parse error (LocOS: HTTP 422).
- Options used: `limits`, `source_lang` (code page of an undeclared legacy file), `target_lang` (`:lang:` header entry
  on merge, only when a unit is translated), `include_locked` (spans with no letter become locked units `k1`, ...).
- Skeleton `encoding`: whatever `enc::decode_text` gives (`utf-8`, `utf-8-bom`, `utf-16le/be`, code pages, `;nl=` parts).
- Hidden codes on a unit: `~raw` (source text of the span), `~sep` (table cell separator), `~ls` (starts a line),
  `~meta` (header description / keywords). Inline tag ids are LocOS token numbers; labels are `bold`, `italic`, `mark`,
  `code`, `link`, `xref`, `footnote`, `attribute`, `passthrough`, `break`, `anchor`, `image`, `ui`, `math`, `macro`,
  `superscript`, `subscript`, `placeholder` (nesting deeper than 8). No tag keeps plain text, so none is `placeholder`
  in the adapter's sense: LocOS tokenises all of them.
- Oracle: `oracle/corpus/adoc/` holds the made-up corpus (13 files); add `adoc`, `asciidoc` to `FORMATS` in `run.py`.
- Merge options still wanted from the model: "stamp the target language even when nothing is translated" (LocOS does)
  and "always write UTF-8" (LocOS converts a legacy file to UTF-8 on build).

# Wiring for the lead

- Workspace: `"bilingua-wiki"` in `members` and `bilingua-wiki = { path = "bilingua-wiki" }` in `[workspace.dependencies]`
  (done in the wire commit). Needs `bilingua-model` and `regex`; dev-dependencies `proptest` and `serde_json`.
- Facade: dependency `bilingua-wiki = { workspace = true }` and two filters:
  `Box::new(bilingua_wiki::MediaWiki::default())` (id `wiki`, extensions `wiki`, `mediawiki`, `wikitext`) and
  `Box::new(bilingua_wiki::DokuWiki)` (id `dokuwiki`, extension `dokuwiki`).
- `sniff`: both return `false` (plain text; only the extension says so). A `.txt` file is never guessed to be DokuWiki.
  They can sit anywhere in `filters()`.
- Options used: `target_lang` and `source_lang` (the latter only picks the legacy code page of an undeclared file),
  `include_locked` (a span with no letter becomes a locked unit `k1`, `k2`, ...), `limits`.
- The one setting: LocOS `WIKI_TEMPLATE_PARAMS` becomes `MediaWiki::with_template_params(spec)`; the adapter builds the
  filter per install with the setting (the default filter protects every template parameter).
- Skeleton `encoding` values are `bilingua_model::enc`'s (`utf-8`, `utf-8-bom`, `utf-16le`, `windows-1252`, ... with the
  `;nl=` suffix).
- Inline codes: every code is a LocOS token (paired `g`, standalone `x`); none is a flattenable placeholder, none is `free`.
  Labels: `bold`, `italic`, `bold italic`, `underline`, `link`, `footnote`, `media caption`, `template`, `parameter`,
  `comment`, `url`, `entity`, `html`, `signature`, `magic word`, `line break`, `monospace`, `code`.
- Hidden codes on a unit: `~raw` (the source span) and `~cell` (`tpl`, `mw` or `dw`: how a translation is escaped).
  `context.kind` is `heading` (with `extra.level`), `paragraph`, `list-item`, `term`, `definition`, `cell`, `header-cell`,
  `caption`, `quote` or `template parameter`.
- Merge options still wanted from the model: "stamp the target language even when nothing is translated" is not needed (no
  language in either format); "always write UTF-8" (LocOS writes UTF-8 for a Windows-1252 file).
- Oracle: `oracle/corpus/wiki/` (34 made-up files); add `wiki` (`wiki mediawiki wikitext`) and `dokuwiki` to `FORMATS` in `run.py`.

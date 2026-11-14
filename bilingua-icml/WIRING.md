# Wiring for the lead

- Workspace: `"bilingua-icml"` in `members`, `bilingua-icml = { path = "bilingua-icml" }` in
  `[workspace.dependencies]` (done in the wire commit). The crate uses `bilingua_model::guard` and the model types only.
- Facade filter list: `Box::new(bilingua_icml::Icml)`. Format id `icml`, extensions `icml`, `wcml` (both map to the
  one filter, as in LocOS where `wcml` is an alias).
- `sniff`: tight. Text starting with `<` whose first 8 KB contains `InCopyInterchange` (the `<?aid SnippetType?>`
  processing instruction) or `urn:SmartConnection`. It can go anywhere before generic XML (`xml`) and before `Html`.
- Options: none.
- Skeleton: `Skeleton::Text` with one `Raw` part holding the whole original file (BOM removed), `encoding`
  `utf-8` or `utf-8-bom`. Merge parses it again, so the skeleton is the file.
- Hidden code `~p` on each unit holds the paragraph number (needed by merge). Unit ids are `u<paragraph number>`.
- LocOS adapter: ids are the LocOS token numbers. Letter by label: `Open`/`Close` with label `run` is `r`, other pairs
  are `g`, `Standalone` is `x`; LocOS `inline_map` kinds are `run`, `wrap`, `point` accordingly. Nothing is a
  `placeholder`.
- Merge options still to come in the model: "stamp even when nothing is translated" (not used here: this format only
  stamps rebuilt ranges, as LocOS `build_localized` does) and "always write UTF-8" (always true here).
- Oracle: `oracle/corpus/icml/` holds the LocOS suite documents (`suite_*.icml`, `suite_*.wcml`) and Okapi's
  `Test01.wcml`; add `icml` and `wcml` to `FORMATS` in `oracle/run.py`. Okapi's file is Apache 2.0 (Okapi Framework,
  tag v1.48.0, `okapi/filters/icml/src/test/resources`).

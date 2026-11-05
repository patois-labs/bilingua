# Wiring for the lead

- Workspace: `"bilingua-xmb"` in `members`, `bilingua-xmb = { path = "bilingua-xmb" }` in `[workspace.dependencies]`
  (done in the last commit, replace at merge). Dev-dependencies: `proptest = "1"`, `serde_json`, `bilingua-xliff`
  (workspace; used by one test of the Angular XLIFF fixtures).
- Facade (`bilingua/Cargo.toml` and `filters()`): TWO filters, `Box::new(bilingua_xmb::Xmb)` (id `xmb`, extension `xmb`)
  and `Box::new(bilingua_xmb::Xtb)` (id `xtb`, extension `xtb`). Both read either kind of file, as LocOS does, and set
  `Document::format` by the root element. `sniff` is tight (first start tag is `<messagebundle` or
  `<translationbundle`), so they can go anywhere before the loose sniffers; I put them right after Resjson.
- `bilingua/tests/contract.rs` `crate_of`: `"xmb" | "xtb" => "bilingua-xmb"`.
- `tools/check-portable.sh` and `.ps1`: `xmb` in `CRATES`.
- Skeleton `encoding` values: `utf-8`, `utf-8-bom`, `utf-16le`, `utf-16be` (with BOM) or an `encoding_rs` name, with
  `;nl=crlf`, `;label=..` as the engine defines.
- Merge rule the adapter must know: an XMB document merges to the **original XMB** when no segment has a target, and to
  an **XTB** (what LocOS exports) when any has one; `Xmb.to_xtb` forces the XTB. The XTB `lang` is written from
  `Document::target_lang` when a translation changed (`Xtb.merge_localized` forces it). Export name: `<stem>.<locale>.xtb`.
- Oracle: `oracle/corpus/xmb/` (three `.xmb`, two `.xtb`); add `xmb` and `xtb` to `FORMATS` in `oracle/run.py` and the
  LocOS names in `scoreboard.py` (not touched here).
- Options: none.

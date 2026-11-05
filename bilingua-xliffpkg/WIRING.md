# Wiring bilingua-xliffpkg (the lead replaces the wire commit at merge)

The wire commit (`wire: xliffpkg (lead replaces at merge)`) already makes these edits; they are listed so the lead can
redo them by hand.

- Format id: `xlz`. Extensions: `xlz`, `wsxz` (LocOS resolves `wsxz` to its `xlz` filter, so does this crate).
  Crate `bilingua-xliffpkg`, `bilingua_xliffpkg::filters()` returns the one filter.
- `filters()` in `bilingua/bilingua/src/lib.rs`: `all.extend(bilingua_xliffpkg::filters());` next to
  `bilingua_xliff::filters()` (before the loose sniffers).
  **Sniff is always false**: a zip is recognised by extension only (as LocOS), so it can sit anywhere.
- `own_segmentation()` in the facade: add `"xlz"` (the XLIFF members carry their own segments).
- Facade `Cargo.toml`: `bilingua-xliffpkg = { workspace = true }`. Workspace `members` and `[workspace.dependencies]`.
- `bilingua/tests/contract.rs` `crate_of`: `"xlz" => "bilingua-xliffpkg"`.
- `bilingua/tools/check-portable.sh` and `.ps1`: `xliffpkg` added to `CRATES`.
- Options: none of its own. `target_lang` stamps `target-language` on any inner `<file>` that has none, only when at
  least one XLIFF member was changed by the merge (nothing translated: original bytes back, no stamp).
- `bilingua_xliffpkg::mqxlz()` is the LocOS `mqxlz` filter built on the same code (for tests and comparison). It is
  **not** in `filters()`: the facade keeps `bilingua-sdl`'s `mqxlz`. Difference: this one keeps member dates,
  compression, order and the archive comment, and returns the original bytes when nothing is translated; the sdl one
  rewrites the zip.
- Corpus: `bilingua/oracle/corpus/xliffpkg/` holds 4 made-up packages (`locos_content.xlz`
  is the LocOS test package rebuilt from its fixtures (the multi-segment `locos_multi.wsxz` is built inside the tests, not kept as a corpus file, because the facade contract wants one segment per unit in paragraph mode); `lumen_languages.wsxz` (ar, ja, zh, ko), `lumen_utf16.xlz`
  (UTF-16 member), `lumen_bom.xlz`). `oracle/corpus/SOURCES.md`: made up for Bilingua except the XLIFF files inside
  `locos_*`, which are ported from the LocOS test fixtures (Apache-2.0).

## LocOS adapter notes

- Segment ids are `<member path>|<segment id>`; `Unit::context.location` is the member path. LocOS keys are
  `offset + position` in the member's walk: the adapter derives them by counting segments (protected ones included)
  in package order.
- LocOS protects locked, approved, final and confirmed units (not parsed, never written). Here a finished or
  confirmed segment is extracted with its target and state (Brent 11-05), only real locks set `translate=false`. The
  adapter selects `translate && state in {New, Draft}` to get the LocOS unit list (tested in `locos_oracle_golden`).
- Tokens: same as `bilingua-xliff`. `<bpt>`/`<ept>` pairs are `g` pairs here, `x` `x` in LocOS (see report).
- LocOS `build()` maps to `merge` with `target_lang: None`; `build_localized()` to `merge` with `target_lang`.
  "Stamp even when nothing is translated" (LocOS) is the coming model option; not implemented here.
- `tests/data/okapi_test1.wsxz` is Okapi's own WorldServer package test resource (Okapi Framework 1.48.0,
  `okapi/filters/wsxzpackage/src/test/resources/test1.wsxz`, commit `ce9f9edc2956da5868fcb71ae10426c7c6ae4e0b`,
  Apache-2.0, unmodified). It is a test fixture, not in the corpus folder.

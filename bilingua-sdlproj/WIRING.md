# Wiring for the lead

Already done in the wire commit (replace at merge):
- workspace `members` and `[workspace.dependencies]`, facade `bilingua/bilingua/Cargo.toml`, the `filters()` entry in
  `bilingua/bilingua/src/lib.rs`, the `crate_of` row in `bilingua/bilingua/tests/contract.rs`, `Cargo.lock`, `sdlproj`
  in `CRATES` of `tools/check-portable.sh` and `.ps1`, and the corpus folder `oracle/corpus/sdlproj/` (28 files).

What the facade needs:
- Format id `sdlproj`, extension `sdlproj`. Filter value: `bilingua_sdlproj::Sdlproj` (unit struct, no options).
- `sniff`: tight. The first 4 KB (UTF-8 or UTF-16 with a BOM) hold `<PackageProject`, or `<Project ` with `Guid=` and
  `GeneralProjectInfo` or `LanguageDirection`. It sits after `Xaml` and before generic XML (`XmlFilter`), with the
  other XML vocabularies; an MSBuild `<Project Sdk=...>` is not claimed.
- `bilingual()` is left false: the file carries no translations.
- Detection by extension is the normal way in; `own_segmentation` needs no change (no units).
- Oracle: add `sdlproj` to `FORMATS` in `run.py`. LocOS has no filter for it (`get_filter("sdlproj")` is `None`), so
  the scoreboard row will say LocOS reads 0 units; the status should be an accepted difference ("LocOS: manifest of a
  package only, no text"), not a missing parser.
- LocOS adapter: nothing to adapt for a loose file. For package manifests, `bilingua_sdlproj::flip_package_type`
  and `project_info` are the two manifest functions of `sdl_package.py`.

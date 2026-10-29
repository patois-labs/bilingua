# bilingua-sdl

Translation packages for Bilingua: Trados project and return packages (`.sdlppx`, `.sdlrpx`) and memoQ packages
(`.mqxlz`). Moved here from `core/catcore`.

- `SdlPackage<D>` and `MqPackage<D>` read the zip under `ZipLimits` (entry count and size, expansion ratio, no
  encryption, links, `..`, absolute or drive paths, duplicate names), open the bilingual files inside, and write
  the package back with edited files replaced and everything else as it was. `SdlPackage::to_return_package`
  builds the Trados return package (project file changes mirror the real project and return packages of an open
  test project). Nothing is written to disk by the library.
- `D` is any type that implements `PackageDoc`; `bilingua_xliff::Doc` does, and CAT OS uses its editor type.
- `filters()`: Bilingua `Filter`s `sdlppx` (`sdlppx`, `sdlrpx`) and `mqxlz`. One document holds the segments of
  every target-language file; segment ids are `<path in the package>|<segment id>`. Source-language copies and
  reports are kept untouched. `merge` writes a zip of the same entries; use `to_return_package` for Trados.
- Not verified: no package written here was opened in Trados Studio or memoQ.

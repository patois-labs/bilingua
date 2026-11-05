# bilingua-sdlproj

Trados Studio project files (`.sdlproj`) for Bilingua. It matches what the Localization OS filter does with one.

```rust
use bilingua_sdlproj::{project_info, flip_package_type, Sdlproj};
use bilingua_model::*;

let info = project_info(&bytes)?;            // source, targets, GUID, package type, name
let doc = Sdlproj.extract(&bytes, &ExtractOptions::default())?;
assert!(doc.units.is_empty());
assert_eq!(Sdlproj.merge(&doc)?, bytes);     // the original bytes
let manifest = flip_package_type(&bytes);    // the return package's manifest
```

## What LocOS does with an sdlproj (checked 11-05)

- LocOS has **no filter** for a loose `.sdlproj`: `get_filter("sdlproj")` is `None`, which is why the oracle
  scoreboard showed 0 units and 1 read error.
- `app/filters/sdl_package.py` reads the `.sdlproj` only as the **manifest of a package** (`.sdlppx`/`.sdlrpx`). It
  takes the language pairs, the GUID, the package type and the project name, and extracts **no text** from it. Its
  only write is `flip_package_type`: `PackageType="ProjectPackage"` becomes `"ReturnPackage"` on the root element,
  every other byte unchanged.

So the units of an sdlproj are none, and this crate says so: `extract` gives a document with **zero units**, the
file's languages (`source_lang`, `target_lang` = the first target) and the project name (`Name` of the root, when
present). `merge` returns the original bytes in the file's own encoding. `project_info` returns all target
languages, and `flip_package_type` is the one manifest edit LocOS makes. The translatable work of a Trados package is
in its SDLXLIFF files (`bilingua-sdl`, `bilingua-xliff`).

## Reading rules (as LocOS)

- Root element `PackageProject` or `Project` (any prefix); anything else is refused ("not a Trados project file").
- Every `LanguageDirection` element anywhere gives a pair from `SourceLanguageCode` / `TargetLanguageCode` (both
  non-empty after trimming). No pair is refused; more than one source language (compared without case) is refused.
  Targets are listed once each (without case), first spelling kept.
- `package_guid` is `PackageGuid`, else `Guid` (64 characters at most); `package_type` 40; `name` 200.
- Hardened XML: any DOCTYPE is refused, only the five predefined entities and valid character references are read,
  attributes must be unique, prefixes declared, XML characters only, one root. Over 16 MB is refused
  (`MAX_SDLPROJ_BYTES`). Depth, element count and size limits come from `bilingua_model::Limits`.
- Encodings: UTF-8 (BOM kept), UTF-16 with a BOM, or a legacy charset the XML declaration names. A non-UTF-8 file with
  no declaration is refused (LocOS and the XML specification do). A legacy file whose bytes would not come back
  unchanged is refused rather than altered.

## Differences from LocOS

- LocOS has no loose-file filter; here it is a filter with zero units (the adapter keeps using LocOS for packages).
- `Name` is read from the root element only. Studio's own non-package projects keep the name on
  `GeneralProjectInfo`; LocOS does not read it and neither do we (`name` stays empty).

# Wiring for the lead

Facade (`bilingua/src`): format id `strings`, filter `bilingua_strings::Strings` (unit struct, `Filter` impl).

- Extensions: `["strings"]` (the filter returns them; LocOS has none registered, the extension is the format name).
- Detection: `Strings.sniff(bytes)` reads the first 8 KB (UTF-8 or UTF-16, BOM or not) and wants at least one
  `key = value;` entry; it is false for PO, JSON, plain text and binary plists. Put it after the extension check.
- Options: none needed. `Strings.extract_with(bytes, opts, &Limits)` takes custom limits; the default matches LocOS.
- `bilingual()` is false. `bilingua_strings::records(bytes)` is the key/value/comment view (LocOS `records()`).
- Workspace: `members` gets `bilingua-strings`; `[workspace.dependencies]` gets
  `bilingua-strings = { path = "bilingua-strings" }` (the wiring commit on this branch already does both).
- Python wheel: nothing format specific.

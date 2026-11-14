# Wiring (done in wave 12 by the lead)

- Workspace `members` and `[workspace.dependencies]` (`version = "=0.9.0"`); facade dependency.
- Facade `filters()`: `bilingua_xmlres::AndroidStrings` and `bilingua_xmlres::JavaXmlProperties`, after the XML
  vocabularies with their own sniff and before DITA, DocBook and generic XML. Both sniff the root element only
  (`<resources>`, `<properties>`, any prefix), so they are strict.
- Facade `detect`: a `.xml` file whose root is `<resources>` or `<properties>` goes to these filters; any other
  `.xml` stays with `bilingua-xml` (LocOS `XmlResourceDispatch`). Neither filter claims an extension.
- Format ids `android`, `javaxml`; contract test `crate_of`: `"android" | "javaxml" => "bilingua-xmlres"`.
- Oracle: `oracle/corpus/xmlres/` (made up). The LocOS side is `get_filter("xml")`, which picks the same filter
  by root; `locos_dump.py` reports the dialect (`xml:android`, `xml:java-xml-properties`).
- Options: none. Merge honours `output_encoding` and `stamp_target_always` (plural fill with nothing translated).
- LocOS adapter: tags map one to one (Open/Close = `g`, Standalone = `x`); placeholders are text as in LocOS. LocOS's
  `p_index` counts skipped slots; Bilingua ids count units only (`u1`...). `records(bytes)` (and `AndroidStrings::records`, `JavaXmlProperties::records`)
  is LocOS's `records()` key view (`Record`: key, value, comment, plural, index, translatable; every entry in file
  order); `tests/records.rs` checks it against LocOS on `tests/data/locos_records.json` (`tests/tools/gen_records.py`).

# bilingua-xmlres

Android string resources (`res/values*/strings.xml`) and Java XML properties (`java.util.Properties.storeToXML`) for
Bilingua. Both arrive as plain `.xml`; the `bilingua` facade hands a `.xml` file to this crate when its root element
is `<resources>` or `<properties>`, as the Localization OS `.xml` dispatcher does (`app/filters/xml_resource.py`).
Use it through the `bilingua` crate; its own API may change in minor releases.

```rust
use bilingua_xmlres::AndroidStrings;
use bilingua_model::*;

let mut doc = AndroidStrings.extract(&bytes, &ExtractOptions::default())?;
doc.set_target("u1", vec![Inline::text("Bienvenue")])?;
let out = AndroidStrings.merge(&doc)?;
```

## Android (`android`)

- One unit per `<string>`, per `<item>` of a `<plurals>` (`context.extra["plural"]` = the quantity) and per `<item>`
  of a `<string-array>` (`context.extra["index"]`). `context.key` is the resource name, `context.notes` the comment
  right before it (for a plural or array item, the comment before its group). Ids are `u1`, `u2`, ... in file order.
- Not translated: `translatable="false"` (on the string, the plurals or the array), a value that is a resource or
  theme reference (`@string/x`, `?attr/x`), values with no letter or digit, `<color>`, `<dimen>` and other resource
  types.
- The translator sees `\'` `\"` `\@` `\?` unescaped; `\n`, `\t`, `\uXXXX` and `\\` stay as typed. A value wholly in
  double quotes is shown without them and written back in them. Styling children (`<b>`, `<i>`, `<a>`, `<br/>`) are
  paired tags; `<xliff:g>` (with its content), comments and processing instructions are standalone tags kept byte
  for byte. CDATA is read as text (its HTML stays text, as in LocOS) and written as CDATA again.
- Merge writes `'` and `"` as `\'` and `\"` (aapt2 refuses them bare), escapes a leading `@` or `?`, and writes a raw
  line break as `\n` when the source wrote its breaks as `\n` and had no raw one (LocOS #706).
- Plurals: on a merge with a target language (and something translated, or `stamp_target_always`), every `<plurals>`
  gets the CLDR categories the language uses, filled with the `other` text, and loses the ones it never uses
  (`other` is kept). The locale itself lives in the folder name (`values-fr/`), so nothing else is stamped.

## Java XML properties (`javaxml`)

- One unit per `<entry key>`; `context.key` is the key. `<comment>` and the DOCTYPE are kept byte for byte (the DTD
  URI is never fetched). An entry with child elements, a comment or a processing instruction inside is left alone
  (LocOS: lxml counts them as children).
- A value with MessageFormat arguments (`{0}`, `{1,date}`) shows a doubled `''` as one `'` and doubles every `'` on
  merge; other values keep a single apostrophe.

## Both

- Placeholders (`%1$s`, `%d`, `{0}`) stay text, as LocOS shows them.
- An untouched merge returns the original bytes. Only translated values change; everything else (declaration,
  comments, indentation, attribute quoting, CRLF, BOM) is kept. Translated text is escaped (`& < >`); characters
  XML 1.0 forbids are dropped. The markup of a translation is repaired as LocOS repairs it, so a merge never writes
  broken XML: an unknown tag or a second use of one is dropped, a close with no matching open is dropped, and a
  pair left open is closed at the end.
- Encodings (`bilingua_model::enc`): UTF-8 (BOM kept), UTF-16 with a BOM, and the charset the declaration names. The
  file goes back in its own encoding (LocOS writes UTF-8; use `MergeOptions` `output_encoding` UTF-8 for that).
- Safety: a DTD internal subset is refused (no entity expansion, no XXE), undefined entities, invalid character
  references and characters XML forbids are errors; element depth and count, unit count and input size follow
  `ExtractOptions::limits`.

Licensed under Apache 2.0.

# bilingua-resx

.NET `.resx` and UWP / WinUI `.resw` filter for Bilingua. It matches the Localization OS filter
(`app/filters/xml_resource.py`, resx part) unit for unit and byte for byte.

```rust
use bilingua_resx::Resx;
use bilingua_model::*;

let mut doc = Resx.extract(&bytes, &ExtractOptions::default())?;
doc.set_target("u1", vec![Inline::text("Willkommen")])?;
let out = Resx.merge(&doc)?;
```

## What it does

- One unit per `<data>` that has a `<value>` with a letter or digit. A `<data>` with a `type` or `mimetype` (images,
  `ResXFileRef`, serialized objects, `System.Int32`), a name starting with `>>` (WinForms designer metadata) and a
  `<value>` that holds child elements, comments or processing instructions are never translated. `<resheader>`, the
  schema, `<assembly>` and `<metadata>` are never touched.
- `<comment>` is the unit's note (`context.notes`); the `name` is `context.key`; unit ids are `u1`, `u2`, ... in file
  order; `context.location` is `line N`. A repeated name gives two units.
- The translator sees the decoded text, cut of its leading and trailing white space (put back on merge, as LocOS does,
  whatever `xml:space` says). Entities, character references and CDATA are resolved; line ends read as LF.
- .NET composite-format placeholders (`{0}`, `{1:N2}`, `{0,10}`, `{dbName}`) are standalone tags with label
  `placeholder` whose display is the exact text, so the LocOS adapter flattens them to the plain text LocOS shows.
  Named and numbered ones are free (may move). `{{` and `}}` are text. Escaped HTML in a value (`&lt;b&gt;`) is text.
- Merge changes only the translated values. An unchanged value keeps its original spelling (entities, CDATA, line
  breaks); everything else (declaration, comments, schema, attribute quoting, `<value />`, indentation, CRLF, BOM) is
  byte for byte. A changed value is escaped (`& < >`, a real CR as `&#13;`), written as CDATA again when the original
  was CDATA, and written with CRLF line ends when the whole file uses them. Characters XML 1.0 forbids are dropped.
- Encodings (B6 `enc`): UTF-8 (BOM kept), UTF-16 with a BOM, and the charset the XML declaration names (ISO-8859-1,
  Windows-125x, Shift_JIS, GBK, Big5, EUC-KR), else Windows-1252. The file goes back in the encoding it came in, with its
  own declaration; LocOS rewrites everything to UTF-8, this crate does not. A translation the code page cannot hold makes
  the file UTF-8 with a truthful declaration. A file whose letters look garbled gets a `Document::warnings` entry.
- The culture of a resx lives in the file name (`Strings.de-DE.resx`), so a target-locale merge stamps nothing.
- `Resx.records(bytes)` gives the key-based view (key, value, comment, unit index, translatable), as LocOS `records()`.
  `include_locked` lists plain-text values left alone (`---`, `>>` metadata) as locked units.
- Safety: a DTD internal subset or any `<!ENTITY` is refused (no expansion, no XXE), undefined entities, invalid
  character references and characters XML forbids are errors, element depth and count, unit count, value length and
  input size are limited (`bilingua_model::Limits`, `ExtractOptions::limits`); everything returns a typed `Error`.

## Differences from LocOS and Okapi

See the report in the commit history; short list: LocOS is the default everywhere. Okapi (`okf_xml-resx`) tags `{n}`
placeholders too (same as here) and also the escaped HTML tags inside a value (not done: LocOS keeps them as text).
Okapi skips fewer designer entries. The letter-or-digit test is Python's `isalnum` exactly (`bilingua_model::py_isalnum`).

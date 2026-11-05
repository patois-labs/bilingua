# bilingua-xaml

XAML string resources for Bilingua (WPF, UWP, WinUI, .NET MAUI, Avalonia). It matches the Localization OS XAML
filter unit for unit and byte for byte.

```rust
use bilingua_xaml::Xaml;
use bilingua_model::*;

let mut doc = Xaml::default().extract(&bytes, &ExtractOptions::default())?;
doc.set_target("u1", vec![Inline::text("Willkommen")])?;
let out = Xaml::default().merge(&doc)?;
```

## What it does

- One unit per `<sys:String>` (`xmlns:sys="clr-namespace:System;..."` or `using:System`) and per XAML 2009
  `<x:String>`, with the element's text as the source. A string that holds a comment, CDATA or a child element, and a
  string with no letter or digit, is left alone. `x:Key` is `context.key`; a comment right before the element is its
  note. Unit ids are `u1`, `u2`, ... resources first, then attributes.
- `Xaml { view_attributes: true }` (off by default, like `XAML_VIEW_ATTRIBUTES` in LocOS) also takes the `Content`,
  `Text`, `ToolTip`, `Header` and `Title` attributes of views, after every resource string. Property elements
  (`Button.Content`) and attributes with no letter are skipped. `Name`, `x:Name` or `x:Uid` is the key.
- Markup extensions (`{Binding ...}`, `{StaticResource ...}`, `{x:Static ...}`, `{DynamicResource ...}`) are never
  translated, in content or attributes. An attribute escaped with the `{}` prefix is translated after the prefix,
  and the prefix stays. A translation that starts with a brace gets the `{}` prefix, so it can never become a
  markup extension.
- `{0}`, `{0:N2}`, `{name}`, `%s`, `%1$s` inside a string are tags (label `placeholder`, display = the exact text;
  LocOS keeps them as plain text). Numbered ones may move in the target.
- `include_locked` lists what is left alone (markup extensions, number-less values) as locked units `k1`, `k2`, ...
- Merge splices only the translated spans. Declaration, comments, processing instructions, CDATA, namespaces,
  attribute order and quoting, indentation, line ends, the BOM and the final newline come back byte for byte.
  Text is escaped (`& < >`), attribute values per their quote style plus `&#9; &#10; &#13;`; XML-illegal characters
  are dropped; a newline in a translation follows the source's CRLF convention.
- Encodings (`bilingua_model::enc`, declared charsets honoured): UTF-8 (BOM kept), UTF-16 with a BOM, or a legacy
  charset the XML declaration names (Windows-125x, ISO-8859-x, KOI8, Shift_JIS, EUC-JP, GBK, GB18030, Big5,
  EUC-KR). The file is written in the same encoding with its declaration; characters the legacy charset cannot hold
  become `&#NNNN;` (LocOS behaviour; the engine default would switch the file to UTF-8). A legacy file whose bytes
  would not come back unchanged (double-mapped Shift_JIS bytes) is refused, never altered. In a GBK / GB2312 file
  a translated euro sign is always `&#8364;` (the engine's GB18030 writer spells it A2 E3, which GBK readers reject).
- Safety (`bilingua_model::guard`): input size, `<!ENTITY` declarations (entity bombs, external entities), depth 256
  and element count come from `check_input` / `check_xml`; units and unit length from `check_unit_count` and
  `finish`. On top of that any DOCTYPE is refused, only the five predefined entities and valid character references
  are read, a prefix must be declared, and only XML characters are allowed. Limits are `ExtractOptions::limits`
  (`extract_with` takes explicit ones); refusals are `Error::Limit`.

## Differences from LocOS

- An empty result (nothing to translate) is an empty document, not an error.
- `encoding="us-ascii"` is read as UTF-8 (LocOS refuses it); translated text outside ASCII is written as `&#NNNN;`,
  so the file stays ASCII under its declaration.
- A file with no declaration and bytes that are not UTF-8 is read as Windows-1252 (or the source language's code
  page), as every Bilingua text format does (a warning only when the letters look garbled); LocOS refuses it.
- Placeholders are tags (see above).
- Python's `isalnum`/`isalpha` and Rust's differ for combining marks.

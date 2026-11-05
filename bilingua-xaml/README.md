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
- Encodings: UTF-8 (BOM kept), UTF-16 with a BOM, or a legacy charset the XML declaration names (Windows-125x,
  ISO-8859-x, KOI8, Shift_JIS, EUC-JP, GBK, GB18030, Big5, EUC-KR). The file is written in the same encoding;
  characters it cannot hold become `&#NNNN;`. Undeclared non-UTF-8 bytes are refused, never guessed.
- Safety: a DOCTYPE/DTD of any kind is refused (no entity expansion, no external entities); only the five predefined
  entities and valid character references are read; a prefix must be declared; XML characters only. Limits
  (`Limits`, `extract_with`): input 100 MB, 50,000 units, 100,000 characters per unit, depth 256, 4,000,000
  elements, and 40 `<` per allowed unit. They return `Error::Unsupported("input limit: ...")`.

## Differences from LocOS

- An empty result (nothing to translate) is an empty document, not an error.
- `encoding="us-ascii"` is read as UTF-8 (LocOS refuses it).
- Placeholders are tags (see above).
- Python's `isalnum`/`isalpha` and Rust's differ for combining marks.
- Legacy charsets use `encoding_rs`, so `iso-8859-1` is Windows-1252 and a file whose bytes would not come back
  unchanged (rare double-mapped CJK bytes) is refused.

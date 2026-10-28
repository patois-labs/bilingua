# bilingua-markdown

Markdown filter for Bilingua (`.md`, `.markdown`). CommonMark plus GitHub tables, strikethrough, task lists
and footnotes, parsed with `pulldown-cmark`.

## What becomes a unit

| Source | Unit `context.kind` | Notes |
|---|---|---|
| paragraph | `paragraph` | |
| heading | `heading` | level in `style` (`h1` to `h6`); later units carry it in `heading` |
| list item | `list-item` | tight and loose lists, nested lists |
| table cell | `cell` | one unit per cell |
| image alt text | `alt` | written back inside `![...](url)` |
| link title `[a](url "title")` | `title` | |
| front matter `title`, `description` | `title`, `description` | one-line plain, 'single' or "double" quoted values |

Inline: bold, italic and strikethrough are paired tags (`<b>`, `<i>`, `<s>`); links are paired tags (`<a>`);
code spans, images, autolinks, inline HTML, footnote references and hard breaks are standalone tags. The URL
and all markup characters stay in the tag's code data, never in the text.

Not translated: fenced and indented code, HTML blocks, link reference definitions, other front matter keys,
code spans, URLs. A paragraph with no letters (only code, numbers, an image) is left in the skeleton.
An image-only paragraph whose image has alt text becomes a locked carrier unit (`translate: false`) so the alt unit
has a place to be written.

## Byte-for-byte

Markdown is never re-rendered. Each unit covers the byte range from its first to its last inline event; text
inside is the raw source slice (backslash escapes and entities untouched), so `\*` and `&amp;` come back as
written. Block markers, indentation, `> ` prefixes, table pipes, heading `#` and setext underlines stay in the
skeleton. A soft line break is plain text; a line break followed by a prefix such as `> ` is a tag that holds the
prefix. Every unit is checked at extraction: if its source would not write back to the exact original bytes it is
dropped to the skeleton, and `extract` as a whole fails loudly rather than return a file that does not round-trip.
A fuzz test (3000 random fragment mixes) runs through this.

Front matter: a plain value that needs quotes after translation (a colon, `#`, a leading indicator) is written in
double quotes. Values with block scalars, lists, or escapes other than `\"` and `\\` are left alone.

## Preview

`preview(&doc)` renders the translated document to HTML, each segment in `<span data-bl-seg="ID">`. Raw HTML in
the source is shown as text (no passthrough, no scripts). Links and images with a scheme other than http, https,
mailto, tel (or a relative URL) are replaced by `#`. Front matter is not shown. `render_segment(&doc, id)` gives the
inner HTML of one span for live patches.

```rust
use bilingua_model::{ExtractOptions, Filter};
let doc = bilingua_markdown::MarkdownFilter.extract(bytes, &ExtractOptions::default())?;
let out = bilingua_markdown::MarkdownFilter.merge(&doc)?;
```

## Not done

TOML front matter; reference-style link titles (they sit in the definition, which is skeleton); image titles;
definition lists; math; Markdown extensions such as admonitions or MDX; wrapping long translated lines; escaping
characters a translator types that would be read as Markdown (for example a new `*`). Alt and title text typed by
a translator has quotes (titles) or brackets (alt) escaped.

# bilingua-html

HTML filter for Bilingua. Extracts translatable text and attributes, writes translations back, and builds a
live-preview page.

```rust
use bilingua_html::Html;
use bilingua_model::*;

let mut doc = Html.extract(&bytes, &ExtractOptions { target_lang: Some("es".into()), ..Default::default() })?;
doc.set_target("u3", vec![Inline::text("Hola.")])?;
let out = Html.merge(&doc)?;          // bytes
let page = Html.preview(&doc);        // HTML with data-bl-seg spans, scripts removed
let inner = Html.render_segment(&doc, "u3"); // inner HTML for one span (live patch)
```

## What it does

- Never re-serialises the page. A small tolerant tokenizer finds byte ranges; only the text ranges that were
  extracted are rewritten. With nothing translated, `merge` returns the input byte for byte (comments, scripts,
  odd spacing, broken markup and all).
- Units: text in blocks (`p`, `h1`-`h6`, `li`, `td`, `th`, `dt`, `dd`, `blockquote`, `figcaption`, `caption`,
  `button`, `label`, `option`, `summary`, `title`, plain `div` text, block-level links). Attributes: `alt`,
  `title`, `placeholder`, `aria-label`, `aria-description`, `value` of button inputs, and `content` of
  `meta` description, keywords, `og:` and `twitter:` title and description.
- Context: `kind` (heading, paragraph, button, link, alt, title, cell, list-item, label, option, caption,
  summary, meta), `style` (element name), `key` (attribute name, for attributes), `location` (CSS-like path),
  `heading` (nearest heading above).
- Inline tags (`b`, `strong`, `i`, `em`, `a`, `span`, `sup`, `sub`, `small`, `br`, `img`, `abbr`, `mark`, `u`,
  `s` and more) become paired or standalone tags. The full original markup is kept in `Unit::codes`.
  Unpaired or crossed tags become standalone tags so tag ids stay unique.
- Not translated: `script`, `style`, `template`, `pre`, `svg`, `math`, `textarea` content, `code`/`kbd`/`samp`
  (kept as a standalone tag), `translate="no"` and `class="notranslate"` subtrees. With
  `include_locked`, locked block text is extracted with `translate = false`.
- Entities are decoded for the editor and encoded again on merge (`&`, `<`, `>`, and `"` in attributes).
  Unknown entities such as `&foo;` stay as they are. UTF-8 (with or without BOM), ISO-8859-1 and
  Windows-1252 are read and written; characters a legacy charset cannot hold become numeric references.
  Other charsets return `Error::Unsupported`.
- On merge, when the target language is set and at least one segment is translated, `lang` (and `xml:lang`)
  on `<html>` are set to it if present, and `dir="rtl"` is set for right-to-left targets.

## Preview

`preview()` returns the translated document (target where present, else source). Each segment is wrapped in
`<span data-bl-seg="SEGMENT_ID">` inside its element, inline tags inside the span. An attribute unit (or the
text of `<title>` and `<option>`, which cannot hold spans) puts `data-bl-attr-seg="ID"` and `data-bl-attr="alt"`
(or `#text`) on the element. When an element has several, both lists are space separated, and the segment ids
of one unit are comma separated. `render_segment()` returns the inner HTML of a span, or the escaped attribute
or text value for attribute units (for those it returns the whole unit's value, even if the unit was split).

The preview removes `<script>` elements, `on*` attributes, `srcdoc`, `javascript:` URLs, `<base>` and meta
refresh, and adds `<style>[data-bl-seg]{outline:none}...</style>` (with a `.bl-active` highlight hook).
`sanitize_html` is exported for other callers.

## Hidden codes

`Unit::codes` carries three reserved ids that start with `~`: `~raw` (the original bytes of the unit, used
when nothing is translated), `~v` (marks an attribute or plain-text unit) and `~attr` (links an attribute
inside an inline tag to its paragraph). They are never referenced by a segment, so editors can ignore them.
Attribute units inside a paragraph's inline tags are in `Document::units` but not in the skeleton; merge
writes them through their paragraph.

## Not done

- Conditional comments, `<noscript>` and `iframe srcdoc` are treated as ordinary markup.
- A comment in the middle of a paragraph splits it into two units.
- No sentence splitting (that is `bilingua-seg`), no CSS `content:` strings, no inline SVG text.
- Stronger preview sandboxing (CSP, iframe `sandbox`) is up to the caller.

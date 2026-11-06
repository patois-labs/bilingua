# bilingua-html

HTML filter for Bilingua: the Localization OS HTML filter (`app/filters/html_filter.py`, `HtmlFilter`) unit for
unit and byte for byte, on the engine both share with the server-page filters (`bilingua-htmlcore`), plus a live
preview.

```rust
use bilingua_html::{Html, HtmlFilter};
use bilingua_model::*;

let mut doc = Html.extract(&bytes, &ExtractOptions::new().with_target_lang("es"))?;
doc.set_target("u3", vec![Inline::text("Hola.")])?;
let out = Html.merge(&doc)?;          // bytes
let page = Html.preview(&doc);        // HTML with data-bl-seg spans, scripts removed
let inner = Html.render_segment(&doc, "u3"); // inner HTML for one span (live patch)

// EPUB chapters, e-mail parts: no page metadata units (LocOS `HtmlFilter(page_meta=False)`)
let chapter = HtmlFilter::new().with_page_meta(false);
// template languages (LocOS `HtmlFilter(template_pattern=...)`): spans that stay in their text
let hubl = HtmlFilter::new().with_template_pattern(regex::Regex::new(r"(?s)\{\{.*?\}\}|\{%.*?%\}")?);
```

## What it does (LocOS's rules)

- Never re-serialises the page. With nothing translated, `merge` returns the input byte for byte.
- Units, in this order: the block units (text of `p`, `li`, `h1`, `td`, `div`, `title`, `pre`, `option`, ... with
  inline tags kept inside as codes), then the accessibility attributes (quoted `alt`, `title`, `aria-label` of
  `img` and `area`; `title`, `aria-label` of `a`; the value needs a letter), then, with `page_meta` (default on),
  the page metadata (`<meta>` description, keywords, `og:` and `twitter:` title and description).
- Inline tags (`a b i em strong span code kbd samp label q small sub sup u s ruby rt ...`, `br img wbr` as points)
  are codes; an orphan end tag is dropped from the unit (as LocOS), an unclosed start tag is an open code with no
  close. Comments, unknown and block tags end a unit. `script` and `style` bodies are never text.
- `translate="no"` and `class="notranslate"` (`translate="yes"` re-enables a subtree): a block holding only such
  content is no unit; a run inside a block is one point code written back verbatim. `include_locked` has no
  effect (LocOS has no locked units).
- Unit text is the page's text with character references as typed (`&amp;`, `&eacute;`). A translation is
  written with a bare `&`, `<` and `>` escaped (and `"` `'` too where the page's markup is one a browser could
  read differently, or with a template pattern); attribute values are escaped in full, and made opaque to every
  HTML5 tag state where a browser may read the value's surroundings differently.
- Every build is re-parsed the way a browser does (scripting on and off, legacy and new `<select>` parsing) and,
  when the source and the build are both well-formed XML, as XML; a build that would show the reader an element,
  attribute or script/style body the source does not have is refused (`Error::Write`, LocOS's message).
- Unit context: `kind` (heading, paragraph, cell, list-item, title, option, caption, summary, button, label, and
  alt / title / label / meta for attributes), `style` (LocOS's: `a11y:img-alt`, `meta:description`, ...; none for
  block units), `key` (the attribute), `extra["element"]` (the attribute's element), `location` (CSS-like path),
  `heading` (nearest heading above). Attribute units carry the hidden marker `v` = `attr` ([`VALUE`]).
- Encodings: UTF-8 (BOM kept), UTF-16, a declared legacy charset, else Windows-1252, as LocOS reads them. Merge
  keeps the file's encoding when every character fits, else writes UTF-8 and rewrites the charset declaration;
  `OutputEncoding::Utf8` always writes UTF-8. A page read in another code page than the one it declares is written
  as UTF-8 when translated (LocOS).
- With a target language, a merge with a translation (or `stamp_target_always`) sets `lang` on `<html>` (added if
  missing), `xml:lang` where present, `dir="rtl"` for a right-to-left target (an existing `dir` becomes `ltr` for
  a left-to-right one) and `<meta http-equiv="content-language">`.
- A plain `.html` page with closed PHP (`<?php ... ?>`, `<?= ... ?>`) or `<% ... %>` islands is read through
  `bilingua-serverpages` (LocOS `server_page_for_html`); the document says `html` and merges back through the same
  dialect. A page that mixes PHP and `<% %>` is refused. Not with a template pattern.
- Limits: ten `<` per allowed unit and a tag-scan budget (LocOS #571), plus Bilingua's own nesting cap
  (`Limits::max_xml_depth`, 256 by default).

## Differences from LocOS (kept on purpose)

- A UTF-8 byte order mark is the envelope (kept on merge); LocOS reads it as a text character.
- Nesting deeper than `max_xml_depth` is refused (LocOS has no cap).
- A legacy-encoded page whose bytes would not be written back the same is refused (an untouched merge must give
  the input back).
- The encoding of the file is kept (B6) where LocOS writes UTF-8 for a page it read with its Windows-1252
  fallback; the LocOS profile (`OutputEncoding::Utf8`) writes what LocOS writes.
- JSP pages routed from `.html` get the server-page difference (a backslash before `${` written as `&#92;`).
- LocOS's missing-alt findings (`a11y_images`) are not reported.

The differential test `tests/locos_golden.rs` holds 636 LocOS cases (`tests/tools/gen_golden.py`).

## Preview

`preview()` returns the translated document (target where present, else source). Each segment of a block unit is
wrapped in `<span data-bl-seg="SEGMENT_ID">`, inline tags inside the span. An attribute unit, and the text of a
`<title>`, `<option>` or `<textarea>` (which cannot hold spans), puts `data-bl-attr-seg="ID"` and
`data-bl-attr="alt"` (or `#text`) on its element; several are space separated, and the segment ids of one unit
comma separated. `render_segment()` returns the inner HTML of a span, or the escaped value for attribute units.

The preview removes `<script>` elements, `on*` attributes, `srcdoc`, `javascript:` URLs, `<base>` and meta
refresh, and adds `<style>[data-bl-seg]{outline:none}...</style>` (with a `.bl-active` highlight hook).
`sanitize_html` is exported for other callers. A page read through server-page routing has no preview.

# bilingua-xslt

XSLT stylesheets (`.xsl`, `.xslt`; XSLT 1.0, 2.0 and 3.0) for Bilingua. It matches the Localization OS XSLT filter
unit for unit and, for a pseudo-translated merge, byte for byte.

```rust
use bilingua_xslt::Xslt;
use bilingua_model::*;

let mut doc = Xslt.extract(&bytes, &ExtractOptions::default())?;
doc.set_target("u1", vec![Inline::text("Willkommen")])?;
let out = Xslt.merge(&doc)?;
```

## What it does

- Translatable: text nodes in literal result elements (`<p>Hello</p>`), in `xsl:text`, and in the instructions whose
  text is output (`xsl:template`, `xsl:if`, `xsl:when`, `xsl:otherwise`, `xsl:for-each`, `xsl:element`, ...). Text
  directly inside `xsl:variable`, `xsl:param`, `xsl:with-param`, `xsl:function` and `xsl:sequence` is never touched
  (it is often a code value); literal result elements inside them are.
- Inline structure stays in one unit. HTML phrasing elements (`b`, `a`, `span`, ...) and `xsl:text` are `Open` and
  `Close` tags; `xsl:value-of`, `xsl:copy-of`, `xsl:apply-templates`, `xsl:call-template`, `xsl:number`, comments,
  CDATA, processing instructions, entity references and XSLT 3.0 text value templates (`{$n}` under
  `expand-text="yes"`) are standalone tags. Tag ids are the LocOS token numbers (`1`, `2`, ...; a pair shares one),
  so `bilingua_model::profile::tokens::to_locos_text` gives the LocOS text exactly.
- Translatable attributes of literal result elements: `alt`, `title`, `placeholder`, `aria-label`, and the content
  of `<xsl:attribute name="title">` (same names). They are units after every text unit (`context.style` =
  `attr:title`). Attribute value template expressions (`{@name}`) are tags; `{{` and `}}` are shown as one brace
  and written back doubled.
- Never translated: any attribute of an `xsl:*` instruction (`select`, `test`, `match`, ...), XPath, `xsl:message`,
  `xsl:comment`, `xsl:key`, `xsl:sort` and the other declarations, anything marked `translate="no"` or
  `its:translate="no"` (with its descendants), `xsl:text disable-output-escaping="yes"`, and literal result
  `<script>` / `<style>` (in no namespace, XHTML or SVG).
- Any prefix bound to the XSLT namespace works; a stylesheet whose root is a literal result element (simplified
  stylesheet) works.
- Whitespace is collapsed for display only (`xml:space="preserve"` and `xsl:text` keep it). Merge splices only the
  units whose translation differs from the source, so prolog, DOCTYPE, namespaces, attribute quoting, comments,
  indentation, line ends and the BOM come back byte for byte. A newline in a translation follows the unit's CRLF
  convention; XML-illegal characters are dropped.
- An attribute that sits inside the opening tag of an inline element (`<a href=".." title="..">`) is its own unit; the
  text unit writes it in, so both can be translated at once. Merge repairs a damaged tag stream the way LocOS does:
  an unknown or repeated tag is dropped, an unclosed pair is closed, a dropped standalone tag is appended.
- With `target_lang` set and at least one unit translated, the literal `lang` / `xml:lang` of `<html>` literal
  result elements becomes the target tag (`es_ES` gives `es-ES`). Nothing translated gives the original bytes.
- Encodings (`bilingua_model::enc`, declared charsets honoured): UTF-8 (BOM kept), UTF-16 with a BOM, or a legacy
  charset the XML declaration names. A character the legacy charset cannot hold is written as `&#NNNN;` (LocOS
  behaviour), and the file keeps its encoding. In a GBK file the euro sign is always `&#8364;`. A legacy file whose
  bytes would not come back unchanged (double-mapped Shift_JIS bytes) is refused. A declaration the engine cannot
  read (`ebcdic-cp-us`, `UTF-16` on UTF-8 bytes) and a file that is not UTF-8 and declares nothing are refused
  (`Error::Encoding`), as LocOS and the XML specification do.
- Safety: `guard::check_input`, `guard::check_xml` (depth 256, element count; run on the text with the vetted DOCTYPE
  taken out), unit and unit-length limits through `guard::finish`, and a pre-parse budget of 40 `<` or `{` per
  allowed unit. The scanner is strict (balanced, names, declared prefixes, defined entities, XML characters only, no
  `]]>` in text, `--` in comments) and reads at most 200 levels. A DOCTYPE is accepted only when its internal subset
  declares simple character entities (`<!ENTITY nbsp "&#160;">`: one character reference or up to 16 plain
  characters); anything else (entity bombs, external entities, parameter entities, `<!ELEMENT>`) is refused.
  External subsets are never loaded.

## Differences from LocOS

- A stylesheet with nothing to translate is an empty document, not an error.
- Merge stamps `lang` only when a unit is translated (engine rule); LocOS stamps whenever a locale is given.
- A CR in a translation is turned into a newline before the unit's line-end convention is applied (LocOS would
  produce `CR CR LF`).
- Python's `isalnum` and Rust's `is_alphanumeric` differ for combining marks (affects only an attribute whose only
  "letters" are combining marks).
- Limits and messages are Bilingua's (`Error::Limit` with a `LimitKind`), not the LocOS `MAX_*` names.
- `{0}`, `%s` and the like in plain text stay text, as in LocOS (they are not tags).

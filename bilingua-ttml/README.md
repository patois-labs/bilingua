# bilingua-ttml

TTML and DFXP subtitles for Bilingua (`.ttml`, `.dfxp`; IMSC1 text profile documents are TTML). It matches the
Localization OS TTML filter unit for unit and byte for byte.

```rust
use bilingua_ttml::Ttml;
use bilingua_model::*;

let mut doc = Ttml.extract(&bytes, &ExtractOptions { target_lang: Some("es".into()), ..Default::default() })?;
doc.set_target("p1", vec![Inline::text("Hola")])?;
let out = Ttml.merge(&doc)?;
```

## What it does

- One unit per caption `<p>` (under `<body>` and TTML `<div>` elements; a `<p>` in `<metadata>`, a foreign element or
  another `<p>` is not a caption). Unit ids are `p1`, `p2`, ... by caption position, blank captions counted, so a
  caption keeps its id when an empty one gets text. Context: kind `subtitle`, `xml:id` as key, `style`, and
  `begin` / `end` / `dur` / `region` in `context.extra` (display only; timing is skeleton).
- Inline codes: `<br/>` is a standalone code, `<span>` a paired code, any other child element (`<set>`, `<metadata>`,
  animation, a foreign element), a comment or a processing instruction one standalone code that carries the whole
  element. LocOS tokens: `g` for spans, `x` for the rest. `{0}` and `%1$s` stay plain text, as in LocOS.
- Text rules as LocOS: entities are decoded, CDATA is text, `xml:space` is inherited (`default` collapses a run of
  white space that holds a line break to one space, and drops it next to a `<br/>`), the unit text is trimmed, a
  caption with no text (only codes) is skipped.
- Merge splices only the content of the rebuilt `<p>`. Timing, styling, layout, `<head>`, comments, the BOM, line
  ends and every untouched caption come back byte for byte. Text is escaped (`& < >`); a typed newline becomes
  `<br/>` (with the caption's own prefix); XML-illegal characters are dropped; an unknown or repeated code and a stray
  close are dropped, an unclosed span is closed. A translation cannot make a new caption, attribute or element.
- With a target language and something translated (or `stamp_target_always`): `xml:lang` on `<tt>` becomes the target
  (added when absent), and `xml:lang` on other elements that repeat the source language becomes the target too
  (elements in another language and `<head>` metadata stay). For a right-to-left target every rebuilt caption gets
  `tts:direction="rtl"`, using the styling prefix the document binds (TTML `#styling`, DFXP `#style` or `#styling`),
  flipping an explicit `ltr`, or an unused prefix bound on `<tt>`; untouched captions keep their level.
- Encodings (`bilingua_model::enc`): UTF-8 (BOM kept), UTF-16 with a BOM, or a legacy charset the XML declaration names. Legacy bytes with no declaration, or under a false UTF-8 or UTF-16 one, are refused (B6), never guessed.
  The same encoding is written back; a character the charset cannot hold becomes `&#NNNN;`. A file in the engine
  default `MergeOptions` keeps its encoding; `MergeOptions::locos()` writes UTF-8 and rewrites the declaration.
- Safety: any DOCTYPE is refused, only the five predefined entities and valid character references are read, names and
  attribute syntax are checked, prefixes must be declared, only XML characters. `bilingua_model::guard` limits (input
  size, entity declarations, depth 256, elements, units) plus a pre-parse budget of 40 `<` per allowed unit and at most
  4096 namespace declarations in scope. The scanner is one pass in this crate (no second parser to disagree with). Namespace lookups are constant time (an index per prefix) and the duplicate-attribute check uses a set, so namespace-heavy files stay linear.

## Differences from LocOS

- A comment containing `<!DOCTYPE` is accepted (LocOS refuses it by a text search); a real DOCTYPE is refused.
- `xml:lang` is found by parsing the attributes of each start tag (LocOS searches the tag text, which would also hit
  ` xml:lang='..'` written inside another attribute's value).
- A `<br>` written with a separate end tag consumes its balanced subtree (LocOS stops at the first end tag).
- A few well-formedness corners are checked by us (reserved prefixes, an empty prefix binding, a duplicate expanded
  attribute name) and a file with more than 4096 namespace declarations in scope is refused as a limit.
- A literal `⟦x1⟧` typed in the source text is plain text here; LocOS would read it as a token.
- A carriage return typed in a translation is written as is (LocOS does the same); XML readers read it as a newline.
- An undeclared legacy file (Latin-1 bytes and no `encoding=` in the declaration, or a declaration that says UTF-8) is refused here as the B6 rule says; LocOS reads it through its code-page fallback.

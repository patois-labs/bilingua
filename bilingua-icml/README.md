# bilingua-icml

Adobe InCopy ICML (`.icml`) and WoodWing Smart Connection WCML (`.wcml`) for Bilingua. It matches the Localization
OS ICML filter unit for unit, and Okapi `okf_icml` on Okapi's own test resource.

```rust
use bilingua_icml::Icml;
use bilingua_model::*;

let mut doc = Icml.extract(&bytes, &ExtractOptions::new().with_target_lang("de-DE"))?;
doc.set_target("u3", vec![Inline::text("Willkommen")])?;
let out = Icml.merge(&doc)?;
```

## What it does

- An ICML file is one XML file: an InDesign snippet `<Document>` holding the styles and one `<Story>`
  (`Story > ParagraphStyleRange > CharacterStyleRange > Content | Br | ...`). A WCML article holds several stories,
  each in an `<ea:Story>` wrapper (`urn:SmartConnection_v3`); every plain `<Story>` inside a snippet `<Document>` is
  read in document order, each honouring its own document's hidden conditions.
- One unit per visible paragraph (`u<n>`, n = paragraph number, stable for the same file): body paragraphs first,
  then table cells, footnotes and endnotes. A paragraph styled across several ranges is one unit.
- Inline codes, one tag each (ids are the LocOS token numbers, so `u.codes[id].label` tells the kind):
  - `Open`/`Close`, label `run`: a character style range inside a multi-range paragraph (LocOS `r`);
  - `Open`/`Close`, label `link`, `xml-tag`, `tracked-change`, `range`: hyperlink source, XML element, tracked
    insertion, a range nested in one of those (LocOS `g`);
  - `Standalone` (LocOS `x`), labels `tab`, `line-break`, `paragraph-separator`, `special-char` (`<?ACE n?>`),
    `break` (`<Br/>` inside a wrapper), `index-marker`, `footnote`, `table`, `note`, `hidden-text`, `text-variable`,
    `anchored-frame`, `point`.
- Not offered: text under a condition declared `Visible="false"` in the same document, notes, hidden text, tracked
  deletions, `StoryInfo` metadata. They stay where they are on merge.
- Merge: nothing translated gives the original bytes. Otherwise only the `<Story>` elements that hold a translated
  paragraph are rewritten, spliced into the original bytes at their exact spans; inside such a story, every node the
  rebuild did not touch is copied from its source bytes too. Text can grow, shrink and reorder; a point or wrapper
  the translation dropped is kept at the end of its paragraph, never lost. A newline in a translation becomes a
  space (a break would add a paragraph), except in a paragraph whose source already has line feeds in its `Content` (a
  code block), where they stay, XML-illegal characters are dropped.
- Target language (`Document::target_lang`): every rebuilt range gets `AppliedLanguage` set to the InDesign language
  the file itself declares for the target (`<Language Name="$ID/...">`); an undeclared language is never invented.
  A right-to-left target also sets `ParagraphDirection="RightToLeftDirection"` and the Adobe World-Ready composer
  on every translated paragraph, and turns an explicit left-to-right `CharacterDirection` into `DefaultDirection`.
  Nothing is stamped when nothing is translated.
- UTF-8 only (one BOM allowed and kept). A declared encoding other than UTF-8, UTF-16, or bytes that are not UTF-8
  are refused with `Error::Encoding`, as LocOS does.
- Safety (`bilingua_model::guard`): input size, entity declarations, depth and element count come from `check_input`
  and `check_xml`; any DOCTYPE is refused; only the five predefined entities and valid character references are
  read; namespace prefixes must be declared; only XML characters are allowed; markup is capped at forty `<` per
  allowed unit; the reader is iterative with a hard depth cap of 1000. Refusals are typed errors.
- Portable: no file system, threads, clock or network (`cargo check --target wasm32-unknown-unknown`).

## Differences from LocOS

- A story is spliced by exact byte span (the XML reader knows every span), so LocOS's fallback of re-serializing the
  whole document when a comment mentions a `<Story>` tag never happens.
- Inside a rewritten story, untouched elements keep their original bytes (LocOS re-serializes the whole story
  through lxml: single quotes, `<Br />`, CRLF and entity spellings change there).
- Tokens are structural tags: literal text that looks like `⟦x1⟧` stays text.
- A unit that has a target counts as translated even when the target equals the source (its ranges are rebuilt and
  stamped).
- Python's `isalnum`/`isalpha` and Rust's differ for combining marks (not used here: the only test is
  "non-white-space").

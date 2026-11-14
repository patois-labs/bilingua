# bilingua-richtext

Rich text JSON filters for Bilingua, as Localization OS reads them (`app/filters/contentful_rich_text.py`,
`sanity_portable_text.py`, `adf.py`, `app/services/connectors/notion_blocks.py`). A file holds the value of one rich
text field.

| Format id | Extension | File holds | Units |
|---|---|---|---|
| `contentful` | `.contentful` | a Rich Text `{"nodeType": "document", ...}` | one per paragraph or heading (list items and table cells hold paragraphs) |
| `sanity` | `.portabletext` | a Portable Text array of blocks | one per text block |
| `adf` | `.adf` | a Jira ADF `{"type": "doc", ...}` | one per inline container (`adf.units=leaf`: one per text leaf) |
| `notion` | `.notion` | a Notion block array, or the blocks under `results`, `children` or `blocks` (a page's `title` property first), or one block | one per text run (rich_text, caption, table cell, page title) |

- Marks and links are paired inline tags (`bold`, `link`, `strong link`), embedded entries, mentions, emoji, inline
  objects and code spans are standalone tags; the tag ids are the LocOS token numbers (`g1`, `x2`). A translation may
  reorder tags, but must keep every tag of its source once; otherwise the merge returns `Error::Unwritable`.
- Everything that is not translated is copied from the source tree: unknown block types, embedded entries and assets,
  `data` payloads, `markDefs`, annotations, code block text (Notion: also its children), ids and key order.
- Text holding the placeholder brackets `⟦` `⟧` is never a unit (Contentful and Sanity, as LocOS).
- An untouched merge, or one whose targets equal their sources, is the original bytes. A real merge splices only the
  values that changed into the original text, written in the layout of the bytes they replace (indent, one line or
  pretty, CRLF, `\uXXXX` escapes); every other byte, number spelling and duplicate key stays.
- Notion writes a run over 2000 characters as several runs with the same annotations and link.
- Option `adf.units` (`inline` default, `leaf`), remembered by the document.
- Limits: nesting 256, the shared `Limits` for bytes and units. Malformed or hostile input is `Error::Parse`.

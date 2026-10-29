# bilingua-model

The document model every Bilingua parser fills and every writer reads back.

- `Filter`: the trait a format crate implements (detect, extract to a `Document`, merge back).
- `Document`: the file's translatable `Unit`s plus a `Skeleton` holding everything else byte for byte.
- `Unit`: one paragraph, cell or string, with one or more `Segment`s (sentences after `bilingua-seg`), its context
  (where it is in the file) and the original markup of its inline codes.
- `Inline`: text or a `Tag` (open, close or standalone). The JSON shape matches the CAT OS editor's own model, so one
  editor shows bilingual files and source files the same way.
- `ExtractOptions`, `Granularity` (sentence, paragraph, document) and `Error`.

No parsing happens here; this crate is types and small helpers only.

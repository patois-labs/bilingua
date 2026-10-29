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

## Bilingual units, tag kinds, parser stamp (D27)

- A `Segment` may carry a target, a `state` (new, draft, translated, confirmed, reviewed, signed-off, rejected,
  locked) and an `origin` (kind tm, mt, ai, human or other, a name and a match percentage). A `Unit` may carry
  `notes` and a `lock_reason`. All are left out of the JSON when empty.
- A standalone tag has a `free` flag: true when it names its argument (`%1$s`, `{name}`) so a translation may move
  it. `locos::to_locos_text` and `from_locos_text` turn tags into the `g`, `x` and `p` text tokens Localization OS
  uses, one token per tag, and back.
- Every skeleton records the producing crate and version (`parser`). `merge` refuses a skeleton from another
  crate or an incompatible version (`Error::ParserVersion`); a skeleton with no record is accepted.
- Raw zip bytes in a skeleton are a base64 string in JSON; the old array form still reads.

Not done: the LocOS adapter itself, and `free` for XLIFF codes read from files (that belongs to the XLIFF crate).

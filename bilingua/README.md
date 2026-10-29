# bilingua

The Bilingua facade: one API over the format crates.

```rust
use bilingua::*;

let opts = ExtractOptions { source_lang: Some("en".into()), target_lang: Some("es-ES".into()),
                            granularity: Granularity::Sentence, ..Default::default() };
let mut doc = extract("page.md", &bytes, &opts)?;      // detect, parse, split into sentences
doc.set_target("u3#1", vec![Inline::text("Primera frase.")])?;
let out = merge(&doc)?;                                // original format, targets in place
let html = preview(&doc);                              // Markdown only: spans with data-bl-seg
```

- `filters()`: text, JSON, Markdown, HTML, PO, SRT and VTT, DOCX, XLSX, PPTX (one crate each).
- `detect(name, bytes)`: by extension, then by sniffing the bytes.
- `extract` applies sentence segmentation (`bilingua-seg`) for `opts.granularity`, using the source language
  (default `en`). `resegment`, `join` and `split` change segmentation later.
- `merge` is byte-identical to the input when nothing is translated.
- `preview` and `render_segment` exist for Markdown; other formats return `None`.

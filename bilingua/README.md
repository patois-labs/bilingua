# bilingua

The Bilingua facade: one API over the format crates.

```rust
use bilingua::*;

let opts = ExtractOptions::new().with_source_lang("en").with_target_lang("es-ES").with_granularity(Granularity::Sentence);
let mut doc = extract("page.md", &bytes, &opts)?;      // detect, parse, split into sentences
doc.set_target("u3#1", vec![Inline::text("Primera frase.")])?;
let out = merge(&doc)?;                                // original format, targets in place
let html = preview(&doc);                              // Markdown only: spans with data-bl-seg
```

- `filters()`: every format Bilingua reads, from office files to subtitles, software strings,
  XLIFF and translation packages (one crate per format family). The workspace README has the full table.
- `detect(name, bytes)`: by extension, then by sniffing the bytes.
- `extract` applies sentence segmentation (`bilingua-seg`) for `opts.granularity`, using the source language
  (default `en`). `resegment`, `join` and `split` change segmentation later.
- `merge` is byte-identical to the input when nothing is translated (zip formats keep every untouched entry as it was).
- `preview` and `render_segment` exist for Markdown; other formats return `None`.

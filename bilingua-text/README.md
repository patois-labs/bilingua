# bilingua-text

Plain text filter for Bilingua (`.txt`, `.text`).

- One unit per paragraph. A paragraph is a run of lines; a blank line (or a line with only spaces) ends it.
- Blank lines, the indentation of a paragraph's first line, and the spaces and line ending after its last line stay
  in the skeleton and come back byte for byte.
- A line break inside a paragraph is a standalone tag (shown as the return arrow). Its code holds the original
  ending, `\n` or `\r\n`, so mixed endings are kept and a translator cannot lose a break by accident.
- UTF-8 with or without a byte order mark. The BOM is kept (`encoding: "utf-8-bom"`), and none is added.
- Not valid UTF-8 is an error. UTF-16 files are refused with a clear message (save as UTF-8).

```rust
use bilingua_model::{ExtractOptions, Filter};
let doc = bilingua_text::TextFilter.extract(bytes, &ExtractOptions::default())?;
let out = bilingua_text::TextFilter.merge(&doc)?;   // identical bytes when nothing is translated
```

Not done: other encodings (Latin-1, Shift-JIS), a lone `\r` as a line ending, hard-wrapped text re-flowing.

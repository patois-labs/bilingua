# bilingua-subtitle

SRT and WebVTT filters for Bilingua: `Srt` and `Vtt`.

```rust
use bilingua_subtitle::{Srt, Vtt};
use bilingua_model::*;

let mut doc = Srt.extract(&bytes, &ExtractOptions::default())?;
doc.set_target("c1", vec![Inline::text("Hola")])?;
let out = Srt.merge(&doc)?;
```

## What it does

- One unit per cue, id `c1`, `c2`, ... in file order. `context.kind = "subtitle"`. `context.key` is the cue
  number (SRT) or the cue identifier (VTT; the cue's position when it has none).
- Timing is in `context.extra["start"]` and `["end"]`, exactly as written in the file. VTT cue settings
  (`align:start`) and SRT coordinates (`X1:100`) are in `extra["settings"]`.
- `extra["max_cpl"] = "42"` is the default maximum characters per line (`DEFAULT_MAX_CPL`). It is a hint for QA
  and the editor, not enforced here. Change it per project by overwriting the value.
- Formatting: `<i>`, `<b>`, `<u>`, `<s>`, `<font ...>` (SRT); `<i>`, `<b>`, `<u>`, `<c.class>`, `<v Name>`,
  `<lang xx>`, `<ruby>`, `<rt>` (VTT) become paired tags. VTT timestamp tags (`<00:00:05.000>`) and SRT style
  overrides (`{\an8}`) are standalone tags. Unclosed or stray tags become standalone tags. Other `<` characters
  in SRT text stay text.
- A line break inside a cue is a standalone tag with display `↵`. It is written with the file's own line
  ending. A translator can also type a newline in the text; it is written the same way.
- VTT text is entity-decoded (`&amp; &lt; &gt; &nbsp; &lrm; &rlm;`) and encoded again on merge (`&` and `<`).
  Other entities such as `&eacute;` are left as written. SRT text is not entity-decoded.
- VTT `WEBVTT` header, `NOTE`, `STYLE` and `REGION` blocks, blank lines, numbering, timing lines, BOM and CRLF
  are skeleton. A cue nobody translated is written back byte for byte.

## Not done

- No line or duration QA (that is for the QA layer, using `max_cpl`), no re-timing, no splitting of long cues.
- No SSA/ASS, TTML or other subtitle formats.
- UTF-8 only; other encodings return a parse error.
- The model forbids more copies of a tag than the source has, so a translation can drop a line-break tag but add
  breaks only by typing a newline.

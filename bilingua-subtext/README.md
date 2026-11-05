# bilingua-subtext

YouTube SBV (`Sbv`) and SubViewer 2.0 / MicroDVD `.sub` (`Sub`) filters for Bilingua. Port of the Localization OS
`subtitle_text_filter`.

```rust
use bilingua_subtext::{Sbv, Sub};
use bilingua_model::*;

let mut doc = Sub.extract(&bytes, &ExtractOptions::default())?;
doc.set_target("c1", vec![Inline::text("Bonjour")])?;
let out = Sub.merge(&doc)?;
```

## What it does

- One unit per cue, id `c1`, `c2`, ... in file order, `context.kind = "subtitle"`, `context.key` the cue number,
  `extra["start"]` and `["end"]` the timing as written (timecodes for SBV and SubViewer, frame numbers for MicroDVD),
  `extra["max_cpl"] = "42"`.
- Cue text only is a unit. Timecodes, headers (`[INFORMATION]`, `[COLF]`), blank lines, `{DEFAULT}` lines, the
  `{1}{1}23.976` frame-rate line, newlines, BOM and encoding are skeleton and come back byte for byte.
- A line break inside a cue is a `\n` in the unit text (as LocOS) and is written back as the cue's own: a newline
  (SBV, the file's first newline when the cue was one line), `[br]` (SubViewer, in the case it had) or `|` (MicroDVD).
  Lines are trimmed; spaces around `|` are layout.
- `.sub` is told apart by content: MicroDVD cues or SubViewer timecode lines. A file that is both, SubViewer 1.0,
  VobSub (binary) or neither is refused with LocOS's plain message (`Error::Parse`).
- MicroDVD control codes (`{y:i}`, `{c:$BBGGRR}`, `{f:Font}`, `{s:12}`, `{P:x,y}`, `{Y:b}`...) are tags, in LocOS's
  shape: a line-scoped lowercase code with words after it on its line is a pair (`Open` + `Close`) around those words
  (#680), so a reordered translation moves the style with its words; every other code is a standalone tag whose order
  matters. Codes that open the cue and apply to it as a whole stay in the skeleton. `{0}` and `%s` stay plain text.
- Hidden codes on a unit: `~raw` (the cue text as written), `~kind` (`sbv`, `sv`, `mdvd`), `~joiner`, `~heads`
  (pairs whose code opens its line), `~fps`.
- An untouched merge, or one where every unit equals its source, returns the original bytes in the original
  encoding (LocOS's byte guard). Otherwise the file's encoding is kept, or UTF-8 with
  `MergeOptions::output_encoding = Utf8` (LocOS). Neither format names a language, so `target_lang` and
  `stamp_target_always` change nothing.

## A translation cannot change the file's structure

LocOS writes a translation as text; Bilingua refuses (`Error::Write`, naming the cue) what would change how the file
reads back:

- a line that reads as a timecode (SBV, SubViewer written with real newlines);
- a line that reads as a MicroDVD cue in a SubViewer file (the file would become ambiguous);
- text that reads as a MicroDVD control code (`{y:i}` typed by a translator, or pieces joined once a tag is dropped);
- a MicroDVD cue `{0}{1}` or `{1}{0}` whose text reads as a frame-rate declaration;
- a translation with no text (only spaces, `|`, `[br]`, codes or placeholders): the cue would disappear.

Lines of only Unicode spaces (U+3000...) are dropped where a line break is a real newline (LocOS writes them as a
blank line that ends the cue). A new line, a blank line or a pipe never makes a new cue: line breaks collapse to the
cue's own break.

## Differences from LocOS

Same units, inline maps and written bytes on 2,400 generated files and 26,000 builds (see `tests/locos_golden.rs`),
except the refusals above (LocOS would write a file that no longer reads) and a file with a UTF-8 byte-order mark
followed by bytes that are not UTF-8 (LocOS reads it as UTF-8 with replacement characters; the shared model reader
falls back to the code page).

## Limits

`bilingua_model::Limits` (units, characters per unit, lines, input size) are honoured while reading.

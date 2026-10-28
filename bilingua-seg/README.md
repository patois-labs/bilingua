# bilingua-seg

Sentence segmentation for Bilingua documents, with SRX 2.0 rules.

## Use

```rust
use bilingua_seg::{segment, join, split, Rules};

segment(&mut doc, "en", Granularity::Sentence, Rules::bundled());
let id = join(&mut doc, "p1#1", "p1#2")?;      // editor: join two neighbours
let (a, b) = split(&mut doc, "p1", 12)?;       // editor: split after 12 characters
```

- `Sentence` splits each translatable unit into sentences. `Paragraph` and `Document` keep one segment per unit.
- Segment ids are `<unit id>#<n>` (1-based) when a unit has more than one segment, else the unit id.
- The text between sentences goes into `Unit::separators`, so `Unit::output()` rebuilds the original exactly.
  Spaces at the very start or end of a unit stay in the first or last segment.
- A paired tag that spans a sentence boundary keeps the whole pair in one segment. A closing tag stays with the
  sentence it ends; an opening tag goes with the next one; a standalone tag stays with the sentence before it
  when it sits right after the full stop.
- `segment` called again first rejoins each unit; targets survive only if every segment of the unit had one.
- `join` merges two neighbours (the target is kept when both have one). `split` cuts after N characters of the
  source text (tags do not count), clears both targets, and refuses to cut inside a tag pair. Both renumber ids.

## Rules

`rules/default.srx` is the bundled rule set (SRX 2.0, Rust `regex` flavour, cascade on). It covers English,
Spanish, French, Italian, German, Dutch, Portuguese, Japanese, Chinese, Korean and Arabic:

- abbreviations per language (Dr., Sr., M., Sig., z. B., Dhr., p. ex., S.A., ...);
- no break before a lower case letter (approx. five), none inside numbers (1.9) or after a list number at the start;
- quotes and brackets after the stop stay with the sentence, including French spacing before a closing guillemet;
- Spanish inverted marks, Arabic question mark U+061F (the Arabic comma U+060C does not end a sentence),
  Japanese and Chinese stops U+3002 U+FF01 U+FF1F with no space needed.

A language with no map (for example Polish) falls back to ICU sentence breaking (`icu_segmenter`).
Custom rules: `Rules::from_srx(xml)`.

Provenance: the rules were written for Bilingua. They follow the layout of Okapi's default SRX (Apache 2.0) but
no text was copied from it; see the note at the top of the file.

## Not done

- No look-behind in rules (the `regex` crate has none), so a few Okapi-style rules need another form.
- The `srx` crate was not used: our own reader is about 150 lines and keeps the cascade and overlap behaviour in
  our hands.
- No segmentation of right-to-left text beyond the rules above; Thai, Khmer and Burmese rely on the ICU fallback.

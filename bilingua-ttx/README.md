# bilingua-ttx

Trados TagEditor TTX (`TRADOStag`) for Bilingua: the bilingual format of Trados 2007 and earlier. It matches the
Localization OS TTX filter unit for unit and token for token, and also reads finished translations as targets.

```rust
use bilingua_ttx::Ttx;
use bilingua_model::*;

let mut doc = Ttx::default().extract(&bytes, &ExtractOptions::default())?;
doc.set_target("u1", vec![Inline::text("Willkommen")])?;
let out = Ttx::default().merge(&doc)?;
```

## What it does

- One unit per outermost `<Tu>` (a segment Trados already made): id `u<n>`, `context.key` the LocOS `p_index`
  (the position among all `<Tu>`, 0 based). Only a pre-segmented file is translatable. A file with no `<Tu>` gives no
  units and a `ttx_not_segmented` warning (LocOS refuses it with a message); text outside the segments is counted in
  a `ttx_unsegmented_text` warning and, with `include_locked`, listed as locked units (`k<n>`), never changed.
- Source and target `<Tuv>` are picked as LocOS does: `UserSettings` `SourceLanguage` / `TargetLanguage`, else the
  first `<Tuv>` is the source and the next one the target. `Document.source_lang` / `target_lang` come from the header;
  the options fill them only when the header is empty.
- Inline codes are tags that map one to one onto the LocOS tokens: a `<ut Type="start">` / `<ut Type="end">` pair among
  the same siblings is `Open`/`Close` (`g`), a `<df>` that holds anything is `Open`/`Close` (`g`), every other element
  (a lone `<ut>`, an unpaired start or end, an empty `<df/>`) is `Standalone` (`x`). The original bytes of every code
  are kept and written back.
- Existing translations (Brent, 11-05): a `<Tu>` whose target `<Tuv>` has text is a unit with that target loaded; its
  state is `Translated` for `Origin="manual"` or a match of 100 and over, else `Draft`; `Origin` and `MatchPercent`
  become the segment origin. The tags of the target take the ids of the source tags with the same markup.
  `Ttx::locos()` (`skip_translated`) is the LocOS behaviour: such a `<Tu>` is not a unit (a locked one with
  `include_locked`). The LocOS adapter uses that.
- Merge splices only what changed: a written target `<Tuv>` (filled, expanded from `<Tuv .../>`, or created after the
  source with the header's target language), and the `<Tu>` start tag, which gets `MatchPercent="0"` and
  `Origin="mt"` (or the segment's own origin when it has one). A unit whose target is empty or unchanged since it was
  read, and a file where nothing changed, come back byte for byte. The source's leading and trailing white space is
  kept around a translation. Unknown, repeated or stray tags in a target are dropped and open ones closed, so the
  output is always well-formed. Text is escaped (`&`, `<`, `]]>`), XML-illegal characters are dropped, and a newline
  follows the file's CRLF convention.
- Target language: when at least one unit was translated and the header's `TargetLanguage` is empty, it is stamped
  from `Document.target_lang` (upper case `FR-FR`). A merge where nothing is translated returns the original bytes.
  A file's own target language is never changed.
- Encodings: UTF-16 LE or BE with a BOM (the usual TTX), UTF-16 without a BOM, UTF-8 with or without a BOM; written
  back the same way. A file that declares another encoding (`windows-1252`) or is not valid in its encoding is
  refused with `Error::Encoding`, as LocOS does.
- Safety (`bilingua_model::guard`): input size, the XML guard (`<!ENTITY`, depth, element count), the LocOS pre-parse
  budget (segments and tags against `max_units`), lines, units and unit length. Any DOCTYPE is refused, only the five
  predefined entities and valid character references are read, prefixes must be declared, only XML characters.

## Differences from LocOS and Okapi

- LocOS refuses a file with no `<Tu>`, and one where every segment is translated; here the first is an empty document
  with a warning, and the second has units with targets (LocOS mode: no units).
- Okapi makes one unit per paragraph holding all its segments, and reads loose text between segments; here a unit is a
  `<Tu>` (LocOS) and loose text is only listed locked. Text with no letter outside segments is not listed (LocOS).
  Okapi's own TTX merge could not re-read its output in the oracle setup (`Unexpected close tag </df>`), so merge
  comparisons are against LocOS and Bilingua's own round trip.
- Newlines in a translation follow the file's CRLF convention (LocOS writes `\n`).
- Rust's `char::is_alphabetic` differs from Python's `[^\W\d_]` for combining marks and numeric letters (used to
  decide whether loose text has a letter).

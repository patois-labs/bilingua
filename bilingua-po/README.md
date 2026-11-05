# bilingua-po

gettext PO and POT filter for Bilingua.

```rust
use bilingua_po::Po;
use bilingua_model::*;

let mut doc = Po.extract(&bytes, &ExtractOptions::new().with_target_lang("es"))?;
doc.set_target("e2", vec![Inline::text("Hola")])?;
let out = Po.merge(&doc)?;
```

## What it does

- One unit per `msgid`. `msgctxt` and `msgid` together are `context.key` (`"menu :: File"`). `#.` comments, `#:`
  references (as `ref: ...`) and translator comments (as `comment: ...`) go to `context.notes`. Other flags
  (`c-format`, ...) go to `extra["flags"]`; `fuzzy` goes to `extra["fuzzy"] = "true"`.
- Plural entries give one unit per plural form with `extra["plural"] = "0" | "1" | ...`. Form 0 has the `msgid`
  as source, every other form the `msgid_plural`. The number of forms is the target language's rule when it is
  known (en, es, de, it, nl, pt: 2; fr: 2; ja, zh, ko: 1; ar: 6), otherwise the number of `msgstr[N]` the entry
  has, otherwise the header's `nplurals`, otherwise 2. The target language is `target_lang` or the header's
  `Language`.
- Existing `msgstr` text becomes the segment target (state `Translated`, or `Draft` when fuzzy).
- printf (`%s`, `%1$d`, `%-5.2f`), Python (`%(name)s`) and brace (`{name}`, `{0}`, `{}`) placeholders become
  standalone tags. `%%` and look-alikes such as `50% off` stay text. Placeholders in an existing translation are
  linked to the source tags by their text.
- The header entry and obsolete (`#~`) entries are skeleton. Only UTF-8 (with or without BOM) is supported.
- Merge writes `msgstr` only where the text changed. Every untouched entry, comment, blank line and line ending
  (LF or CRLF) comes back byte for byte. A changed `msgstr` keeps the layout of the original: a multi-line
  `msgid` or `msgstr` is written as `msgstr ""` followed by one line per `\n`. Long lines are not re-wrapped.
- `#, fuzzy` is removed once every form of the entry is `Translated` or `Confirmed` (use `set_state`); editing
  with `set_target` keeps it, because that leaves the segment a draft.
- When something changed and a target language is set, the header's `Language` line is updated, and
  `Plural-Forms` too if the language changed or the form count differs (only for the languages listed above).

## Hidden codes

`Unit::codes` holds reserved ids that start with `~` (original text, entry id, flag line). They are never used
by a segment. The `#, fuzzy` line is a second skeleton reference to the first unit of the entry.

## Not done

- No `msgmerge`-style fuzzy matching, no `.mo` output, no wrapping of long lines.
- Only the first `#,` line holding `fuzzy` is rewritten.
- Non-UTF-8 PO files return `Error::Unsupported`.

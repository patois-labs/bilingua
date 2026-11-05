# bilingua-mo

gettext binary message catalog (`.mo`, `.gmo`) filter for Bilingua. Spec and baseline: the Localization OS filter
`app/filters/mo_filter.py` and its suite `tests/verify_mo_filter.py`.

```rust
use bilingua_mo::Mo;
use bilingua_model::*;

let mut doc = Mo.extract(&bytes, &ExtractOptions::new().with_target_lang("ru-RU"))?;
doc.set_target("e2.0", vec![Inline::text("%d файл")])?;
let out = Mo.merge(&doc)?;
```

## What it does

- Reads both byte orders, revision 0 and 1 (minor 1 without system-dependent strings), with or without a hash table.
  The hash table is ignored on read and never written, as LocOS does.
- One unit per `msgid`, and one more per `msgid_plural` (when it is not empty): `e<N>` for a message, `e<N>.0` and
  `e<N>.1` for a plural pair, `N` being the position in the file's table from 1. `context.key` is `ctxt :: msgid`,
  `extra["msgctxt"]` the context, `extra["plural"]` the form. The header entry, and any entry with an empty msgid,
  are skeleton. The number of units does not depend on the target language.
- The existing translation is the segment target (`msgstr`, `msgstr[0]`; the plural unit takes `msgstr[1]`).
- printf, Python and brace placeholders are standalone tags, `label` `placeholder` with the exact text as display,
  free (movable) when they name their argument.
- Strings are decoded with the charset the header declares: UTF-8, ASCII, ISO-8859-1, the Windows code pages, KOI8,
  Shift_JIS, EUC-JP, GBK, GB18030, Big5, EUC-KR. Anything else, and the template word `CHARSET`, is UTF-8.
- Every count, offset and length is checked against the file size before use; a file whose strings overlap so that
  they decode to more than twice its size is refused. Typed errors: `Parse` (damaged), `Unsupported` (revision 2 or
  later, system-dependent strings), `Limit` (more strings than `max_units`, a string over 8 x `max_unit_chars`
  bytes, overlap). Strings that are not valid in the declared charset are `Parse` errors too.

## Merge

- Nothing changed: the original bytes come back (byte order, hash table, encoding kept), even with a target language.
- Something changed: a new catalog written as LocOS writes it. Little-endian, revision 0, no hash table, originals
  sorted bytewise, messages without a translation left out (the header stays), the first of two equal keys wins, an
  empty plural slot of a partly translated plural falls back to `msgid` or `msgid_plural`, and a translation follows
  the msgid on leading and trailing newlines (`msgfmt -c`).
- Encoding: the file's own charset is kept when every character fits (LocOS always writes UTF-8). A translation that
  does not fit makes the whole file UTF-8 and every charset label in the header `UTF-8`. A UTF-8 file whose header
  said `CHARSET` or another spelling says `UTF-8` after a merge, as in LocOS.
- With `target_lang` set and at least one unit changed: `Language` (`ru_RU`) and `Plural-Forms` of the target (rules
  from CLDR for every language LocOS knows, 309 rows) are written, a header with no `Language` gets one, a catalog
  with no header gets one, and every `msgstr[N]` of the target rule is filled: the singular goes to the slot n = 1
  lands on, the plural to every other slot; a one-form language takes the plural. Slots past the two source forms
  are kept only when the catalog already names the target language. A language Bilingua has no rule for, or an
  unusable tag, leaves the header alone (LocOS appends an empty `Language:` field there; this crate does not).
- "Supplied" for the fill means a unit whose text changed. The old `msgstr` of an untouched unit is never spread over
  other slots (it is usually in another language).

## Not done

- No hash table is written. No `.po` export (that is the PO filter's job; MO units are the PO units of the same
  catalog). System-dependent strings are refused, as in LocOS.
- A `msgid_plural ""` plural entry has one unit; its translation goes to slot 0 (LocOS loses it).

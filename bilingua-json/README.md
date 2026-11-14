# bilingua-json

JSON filter for Bilingua (`.json`).

- Every string **value** is a unit. `context.key` is its JSON pointer (`/product/tags/0`). Keys are never units.
- The file is not re-serialised. The skeleton holds every byte between string values (spacing, key order, number
  spelling, BOM). Only the string token of a translated value is rewritten, escaped as JSON requires (`"`, `\`,
  control characters); other characters, including non-ASCII, are written as they are.
- A value that was not translated is written back as its original token, so `\/` and `é` spellings survive.
  (The token is kept in `Unit::codes["$raw"]`; it is not a tag id.)
- Placeholders become standalone tags: `{name}`, `{{count}}`, `${total}`, `%s`, `%1$d`, `%.2f`, `%(name)s`, `%%`.
  HTML tags become paired tags (`<b>`...`</b>`, `<a href=..>`) or standalone tags (`<br>`, unpaired ones).
  The original markup is in `Unit::codes`, so a translator can move a tag but not edit it.
- Strict parsing: invalid JSON (comments, trailing commas, bad escapes, nesting deeper than 200) is an error with
  a line and column. UTF-16 is refused.

## Values that are left alone

They stay in the skeleton (or become locked units with `ExtractOptions::include_locked`):

- empty or blank values, and values with no letter at all (`42`, `12:30`, `{count}`, `%s`, `...`);
- URLs (`https://`, `mailto:`, `tel:`, `www.`), e-mail addresses, paths and routes (`/api/v1`, `./img/a.png`);
- ISO dates and times, UUIDs, long hex hashes, `#rrggbb` colours;
- key-like words with no spaces: `user.profile.title`, `app_title`, `MAX_RETRY`, `userName`, `en-US`, `pt_BR`,
  and kebab-case with two or more hyphens or a digit (`page-title-main`).
- a whole-value vue-i18n linked message such as `@:common.save` or `@.lower:(common.save)` (a reference, not text).

Plain words such as `Home`, `e-mail` or `well-known` are extracted. A lower-case camelCase product name such as
`iPhone` is treated as a key and skipped; add it to a project glossary if that is wrong.

## Use

```rust
use bilingua_model::{ExtractOptions, Filter};
let doc = bilingua_json::JsonFilter.extract(bytes, &ExtractOptions::default())?;
let out = bilingua_json::JsonFilter.merge(&doc)?;   // byte-identical when nothing is translated
```

## Not done

ICU MessageFormat plurals (`{n, plural, one {..} other {..}}`) are not parsed; such a value is one unit with its
braces as text. No JSON5, JSONC or JSON Lines. No key filters (translate only some paths); a caller can lock units
by pointer. Right-to-left marks are not added.

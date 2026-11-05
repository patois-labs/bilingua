# bilingua-jsmodule

JavaScript and TypeScript locale module filter for Bilingua: `.js .cjs .mjs .ts .mts .cts` files whose only content
is a static message object (vue-i18n, i18next resources, hand-rolled translation tables). Same rules as the
Localization OS `js_module_filter`.

```rust
use bilingua_jsmodule::JsModule;
use bilingua_model::*;

let mut doc = JsModule.extract(&bytes, &ExtractOptions::default())?;
doc.set_target("u3", vec![Inline::text("Hallo")])?;
let out = JsModule.merge(&doc)?;
```

## What it does

- The file is **tokenised, never executed**: a lexer for strings, template literals, numbers, identifiers, comments
  and punctuation, and a parser that accepts only object and array literals with literal keys, string / template /
  number / `true` / `false` / `null` values, static `'a' + 'b'` concatenation and references to a const declared
  earlier in the file. Accepted shapes: `export default {...}`, `module.exports = {...}`, `exports.x = {...}`,
  `const m = {...}; export default m`, `export const en = {...}`, `export { en, en as default }`; TypeScript
  `: Type`, `as const`, `as T`, `satisfies T`, `import type`, `type X = ...` and `interface X {...}` (types are
  skipped). A hashbang and `'use strict'` are fine.
- Anything else (a call, a spread, a computed key, a method, a `${}` template, an arrow function, any other
  statement) refuses the whole file with `Error::Parse` naming the problem and its line. Oversized files give
  `Error::Unsupported`. Nothing is ever half translated.
- Units: every string value with a letter or digit outside its placeholders, in source order. Keys never. A
  concatenation is one unit. `context.key` is the dotted path (the default export's own name is dropped),
  `context.location` the line. Unit ids are `u1`, `u2`, ... over the units found.
- Placeholders become standalone tags with the LocOS split: i18next `{{x}}` and `$t(key)`, vue-i18n and ICU `{x}`,
  `{'@'}` literals, `@:key` links, `${x}`, printf (`%s`, `%1$s`, `%(name)s`), HTML and Trans tags (`<1>`, `<b>`; one
  standalone tag each, as in LocOS), and ICU plural / select / selectordinal structure (`{n, plural, one {`, `} other {`,
  `#`, `}}` are tags; the variant messages are text). Tags that name their argument (`{{x}}`, `{x}`, `%1$s`, `@:k`,
  `$t()`, `${x}`) are `free` (LocOS `p` tokens, may move); the rest keep their order (`x` tokens).
- Merge replaces only the literals that changed, each in its own quote style, escaped for that quote
  (`\n`, `\t`, ` `, `\xNN`, `\$` in a template). A translated concatenation becomes one literal in the first
  piece's quote. A multi-line template stays multi-line. Everything else (comments, code, types, numbers, other
  strings) is byte for byte.
- Encodings: UTF-8 (BOM kept), UTF-16 with a BOM, Windows-1252 (LocOS's fallback). An untouched merge gives back the
  exact bytes in the original encoding; a merge that changes something writes UTF-8 (the BOM stays if the source had
  one), exactly like LocOS.
- `sniff` is strict: the whole file must parse and hold at least one export or declaration. XML (a Qt Linguist `.ts`)
  is refused (see `WIRING.md`).

## Differences from LocOS (all deliberate or neutral)

- A `/` where a TypeScript type is being skipped refuses the file (LocOS skips it). No type holds a `/`, and the
  lexer does not read regular expressions, so code such as ``x = /`/ + `...` `` after a type with no `;` could hide a
  real template literal and get a string inside it translated.

- Unknown or paired `⟦..⟧` tokens in a translation cannot occur: Bilingua targets are tags, and `set_target`
  refuses an invented one. The LocOS adapter drops them before they get here.
- A string made only of placeholders or symbols stays in the skeleton, as in LocOS; `include_locked` lists it as a
  locked unit.
- Limits are checked inside the crate (50000 strings, 100000 characters per value, token / quote / line budgets
  derived from them, 128 MiB per file); `bilingua-model` has no limits in this tree.
- A TypeScript type that has no `;` is skipped up to the next statement keyword at the start of a line, as in LocOS;
  code that follows an `interface` or `type` with no such keyword is skipped with it and kept byte for byte.

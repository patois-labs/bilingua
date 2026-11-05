# bilingua-mdx

MDX filter for Bilingua (`.mdx`): Markdown with JSX components, `{expressions}` and ES modules. It reads a file the
way the Localization OS `mdx_filter` does (which is built on LocOS's own Markdown line rules, "walk 3"), unit for
unit, so the LocOS adapter sees the same units and the same inline tokens.

```rust
use bilingua_mdx::Mdx;
use bilingua_model::*;

let mut doc = Mdx.extract(&bytes, &ExtractOptions::default())?;
doc.set_target("u3", vec![Inline::text("Bonjour")])?;
let out = Mdx.merge(&doc)?;
```

## What is a unit

In reading order: front matter values, then one unit per translatable line, then the string attributes.

- **Front matter** (`---` YAML or `+++` TOML at the very start): only the one-line string values of the keys `title`,
  `description`, `summary`, `excerpt`, `subtitle`, `sidebar_label`, `pagination_label`, `linkTitle`. A written value
  is a valid scalar again (source quoting if it survives, else single, else double quotes; TOML a basic string).
  YAML is checked by a small reader of our own, TOML by the `toml` crate.
- **Markdown lines**: the markup prefix (heading `#`, bullet, number, `>`, footnote label) is skeleton, the rest of the
  line is the unit. Fenced code, rules, table separators, pure symbol lines, HTML comment blocks and link
  reference definitions (only a one-line title is a unit) are skipped. A table row is one unit. MDX has no indented
  code.
- **Never translated**: `import`/`export` blocks (to the next blank line, not when they would interrupt a paragraph),
  lines made only of JSX tags (also over several lines), lines made only of a `{...}` expression.
- **Inline codes** inside a line: a paired component (`<Kbd>Ctrl</Kbd>`) is an open and a close tag; a self-closing
  component, an unpaired tag, an orphan close tag or an `{expression}` is a standalone tag. Pairing is by tag name,
  as LocOS does. Code spans and backslash escapes are text. Braces in JS strings, template literals and comments do
  not unbalance an expression.
- **String attributes** (`alt`, `aria-label`, `caption`, `label`, `placeholder`, `title`; `MdxOptions::attrs`
  changes the list) are units after all line units, `context.style = "a11y:mdx-attr"`. Expression attributes never.

## Writing

Only the byte range of a changed unit is rewritten, so an untouched merge is the original bytes (BOM, newlines,
legacy code page). A translated line gets what LocOS gives it: tokens put back (reordered is fine, an unknown
token is dropped), `{ } <` the translation adds are backslash-escaped outside code spans, a line break in a one-line
unit becomes a space, emphasis marks glued to CJK text are repaired (#695), a bare URL glued to CJK text becomes
`[url](url)` (#700), a number/`#`/bullet/`>` that would open a block is escaped (#704), the `\|` of a table cell is
restored (#710). A translated attribute value has `{ } < > =` written as character references and its quote as an
entity. Attributes inside a translated line travel with their tag; if the translation drops the tag they go with it.

## Reports

Images with empty alt text (`![](x.png)`) and `<img>`/`<Image>` with no or empty `alt` are reported as
`Warning { kind: "missing_alt", part: "empty" | "missing" }` (LocOS's `a11y_images` report).

## Limits and safety

Nothing is executed. One expression or tag spans at most 100 000 characters (past that `{` or `<` is a literal), and the
whole walk has a step budget, so a file of unclosed braces gives `Error::Limit` instead of a rescan from every
brace. Size limits are `bilingua_model::Limits` (input bytes, lines, units, characters per unit).

## Known differences from LocOS

- A Unicode space inside a JSX tag (`<a b>`) makes LocOS raise an `AttributeError`; here it is a separator.
- A `{`, `}` or `<` a translation adds after an even run of backslashes (`\\{x}`, `\\<X/>`) is escaped here; LocOS
  checks for one preceding backslash only, so such a translation puts a live expression or component in its output.
- The YAML front matter reader is not PyYAML. It follows PyYAML on the shapes in the test corpus (duplicate keys and
  anchors, bad continuation lines, nesting errors, implicit numbers/dates/booleans) and treats unusual YAML as valid.
- Letter tests use tables generated from Python 3.13 (Unicode 15.1), so `isalnum` agrees with LocOS; a newer Unicode
  may differ for characters added since.
- Walks 1 and 2 (documents stored before front matter and block structure were added) are not supported; there are no
  such documents in Bilingua.
- A budget counted in characters in LocOS is counted in bytes for non-ASCII flood files (the refusal threshold moves,
  never the result of an ordinary file).

## Tests

`tests/locos_suite.rs` ports every case of the LocOS suite; `tests/locos_golden.rs` replays 620 MDX files (random, Markdown
structure heavy, front matter heavy, and the corpus) through the LocOS filter's recorded units, inline maps and five different builds
(`tests/tools/gen_golden.py`; set `GOLDEN_MODE=struct|fm` and `GOLDEN_COUNT` to generate more, `BILINGUA_MDX_GOLDEN`
to point the test at the result); `roundtrip.rs`, `hostile.rs` (including proptest) and `speed.rs`
(`tests/tools/speed_locos.py` times LocOS on the same file).

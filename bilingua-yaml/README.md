# bilingua-yaml

YAML filter for Bilingua: generic YAML, Rails i18n files and Symfony translation files.

```rust
use bilingua_yaml::Yaml;
use bilingua_model::*;

let mut doc = Yaml.extract(&bytes, &ExtractOptions { target_lang: Some("de-DE".into()), ..Default::default() })?;
doc.set_target("u1", vec![Inline::text("Hallo")])?;
let out = Yaml.merge(&doc)?;
```

## What it does

- One unit per string value, in document order (`u1`, `u2`, ...). Keys are never units. `context.key` is the dotted key
  path (`app.items[0]`; the Rails locale root is left out), `context.notes` holds the comment lines directly above the
  key, `context.style` the scalar style, and the members of a CLDR plural map (`one:`, `other:` ...) carry
  `extra["plural"]`.
- Which scalars are strings is decided like LocOS does: YAML 1.1 resolution, so `yes`, `off`, numbers, dates and
  `null` are left alone, as are values with a non-string tag (`!!int`, `!ruby/object:Foo`), complex keys, and values
  with no letter or digit. Aliases are never expanded, so an anchored value is one unit and a billion-laughs file is
  harmless.
- Placeholders become inline tags: Rails `%{name}`, `{name}`, `{0}`, `{{x}}`, `${x}`, `%(name)s`, `%s`, `%1$d`, and
  HTML tags (pairs are `Open`/`Close`). Named ones are free to move in a translation; `%s`, `%d` are not.
- The reader is a small hand-written YAML 1.1 scanner that gives every scalar its exact byte span. Its events
  (value, style, tag, span, errors) are checked against PyYAML/libyaml: 360 documents frozen in
  `tests/data/events_golden.json`, plus, outside the test suite, 155 Okapi test files and 12,000 generated documents.
- Merge edits values in place. Comments, key order, indentation, anchors, tags, blank lines, line endings and the
  quoting of every untouched value come back byte for byte. A translated value keeps its style when the new text still
  fits (plain stays plain, single stays single, literal and folded blocks keep their header and indentation, a folded
  block with a translated line break becomes literal) and is double-quoted only when YAML needs it.
- Rails locale files: with a target language and at least one translated unit, the root key (`en:`) becomes the target (`de-DE:`; underscores kept when
  the source used them; `no` is quoted) and plural maps get the categories the target needs (Polish `few`, `many`;
  Arabic `zero`, `two`, ...) filled with the translated `other`.
- Encodings: UTF-8 (BOM kept), UTF-16 with BOM and Windows-1252. An untouched file comes back in its own encoding; a
  file with changes is written as UTF-8 (like LocOS).
- Limits (see `Limits`): 50,000 units, 100,000 characters per value, nesting 200, 64 MiB; plus a line budget and a
  flow-collection budget of 20 times the unit cap, as in LocOS.

## Tests

`cargo test -p bilingua-yaml` runs the ported LocOS cases (`tests/locos_yaml.rs`), the PyYAML event comparison
(`tests/golden.rs`), eleven languages and encodings (`tests/formats.rs`), the property tests (`tests/hostile.rs`; set
`PROPTEST_CASES` for more) and the unit tests. `cargo test --release --test speed -- --ignored --nocapture` prints
the timing of a 5 MB generated file.

Dev tools in `examples/`: `events_dump` (events as JSON, for comparing with PyYAML) and `pseudo_dump` (unit texts and a
pseudo-translated merge, for comparing with the LocOS filter byte for byte).

Corpus (`oracle/corpus/yaml/`): the three LocOS fixtures (Apache-2.0, ours; `rails.en.yml` here also has the `many` forms Spanish needs, so the shared round-trip test sees no new plural units; `symfony.en.yaml` is LocOS
`messages.en.yaml`), `bom-crlf.yaml` and `languages.yml` (made up; the same two strings in 11 languages).

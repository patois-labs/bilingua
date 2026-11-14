# bilingua-plist

Apple property lists and `.stringsdict` for Bilingua: `Info.plist`, `Settings.bundle/Root.plist`, any other XML or
binary plist, and the plural, device and width rules of `Localizable.stringsdict`.

```rust
use bilingua_model::*;
use bilingua_plist::Stringsdict;

let mut doc = Stringsdict.extract(&bytes, &ExtractOptions::new().with_target_lang("ru"))?;
doc.set_target("s5", vec![Inline::tag("1", TagKind::Standalone, "%lld"), Inline::text(" файла")])?;
let out = Stringsdict.merge(&doc)?; // the Russian few and many forms are added, filled from `other`
```

Two filters: `Plist` (id `plist`, extension `plist`) and `Stringsdict` (id `stringsdict`, extension `stringsdict`).
`filters()` returns both, `Stringsdict` first.

## What it does

- **Which strings are units** (Localization OS policy). `.stringsdict`: every string value except the
  `NSStringFormatSpecTypeKey` and `NSStringFormatValueTypeKey` type tags. `Info.plist` (a root with any `CFBundle*`
  key): `CFBundleDisplayName`, `CFBundleName`, `CFBundleSpokenName`, `CFBundleGetInfoString`,
  `NSHumanReadableCopyright`, `CFBundleTypeName`, `UTTypeDescription`, `INAlternativeAppName`, `NSMenuItem`,
  `default`, every `*UsageDescription` and everything under a `*UsageDescriptionDictionary`. Settings bundle (a root
  with `PreferenceSpecifiers`): `Title`, `Titles`, `ShortTitles`, `FooterText`. Any other plist: every string that is
  not an identifier, number, version, URL, build variable (`$(NAME)`), snake_case, camelCase or colour. A value with
  no letter or digit outside its format specifiers is not a unit. `include_locked` lists the rest as locked units
  with the reason.
- **Format specifiers are tags**: `%@`, `%d`, `%lld`, `%.2f`, `%1$@`, `%#@files@`, `%1$#@files@`, `%arg`. A
  positional one (`%1$@`) is a *free* tag that may move; an unnumbered one keeps its order (the LocOS `p` and `x`
  tokens). `%%` stays text. White space around a value is kept and never shown to the translator.
- **Merge keeps everything else byte for byte** (XML): the declaration, DOCTYPE, comments, processing instructions,
  indentation, line endings (CRLF), the BOM, UTF-16 and the entity spelling of every value that was not changed. A
  changed value is escaped (`& < >`), loses characters XML does not allow, and a `\n` becomes `\r\n` in a CRLF file.
  A CDATA value that changes becomes plain escaped text.
- **Binary plists** (`bplist00`): read into the same tree; an untouched file comes back as the same bytes. A
  translated one is written again from the tree (`bplist00`, every non-string value and the key order kept), as LocOS
  does. The skeleton keeps the original file as base64.
- **Plural fill on a target-locale merge** (`.stringsdict` only, LocOS `build_localized`): when the document has a
  `target_lang` and at least one translation, every `NSStringPluralRuleType` dictionary with an `other` form gets
  the CLDR cardinal categories of the target language that it lacks (`zero one two few many other`), inserted before
  `other` in CLDR order and filled with the (translated) `other` text. Device and width rules are not touched. An
  unknown language adds nothing. An untouched merge, or a merge without a target language, is the original file.
  The category table is generated from the CLDR data LocOS reads through babel (the shared `bilingua_model::cldr` table).
- **Records**: `plist_records(bytes)` (every string with its path as the key) and `stringsdict_records(bytes)` (one
  record per entry, plural forms and which unit carries each) are the key-based views LocOS has.
- `context.key` is the path (`PreferenceSpecifiers/1/Titles/0`), `context.style` the nearest dictionary key,
  `context.location` the line (XML). Unit ids are `s<N>` where N counts every string value of the file, so an id
  does not move when another value becomes translatable.

## Refused

DTDs with entities (entity bombs, external entities: `LimitKind::XmlEntities`), custom entities, nesting over 128
levels, shared-object bombs in binary plists, duplicate dictionary keys, a key without a value, unknown elements,
bad integers, reals, dates and data, old-style OpenStep plists, XML in any encoding other than UTF-8 or UTF-16 with a
BOM, and `bplist` versions other than `00`. Limits are `bilingua_model::Limits` (`ExtractOptions::limits`); the
element budget is forty times the unit limit, as in LocOS.

## Differences from LocOS

- Specifiers are tags here and tokens there; the unit texts are the same once a tag is a token (32 of 32 corpus files
  give the same units; 31 of 32 pseudo-translated merges for `es-ES` are byte-identical, the 32nd, a binary plist, is
  the same plist written by `plistlib`).
- An invented tag in a translation is refused by `Document::set_target`; LocOS drops an unknown token on build.
- Plural fill happens only when something is translated (an untouched merge for a target language is the original
  file); LocOS fills whenever it is asked for a locale.
- A binary plist with a repeated dictionary key keeps the last value, like Python; a repeated key in XML is refused,
  like LocOS.
- "Has a letter or digit" is Python's `str.isalnum`, exactly (`bilingua_model::py_isalnum_char`).

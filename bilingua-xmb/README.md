# bilingua-xmb

Google / Angular message bundles for Bilingua: `.xmb` (the source file `ng extract-i18n --format=xmb` writes) and `.xtb`
(the translation files Angular reads).

```rust
use bilingua_xmb::Xmb;
use bilingua_model::*;

let mut doc = Xmb.extract(&bytes, &ExtractOptions::new().with_target_lang("fr"))?;
doc.set_target("u1", vec![Inline::text("Bonjour")])?;
let xtb = Xmb.merge(&doc)?; // an XTB: <translationbundle lang="fr">
```

## What it does (the LocOS `angular_xmb_filter` behaviour)

- One unit per `<msg>` / `<translation>` (ids `u1`, `u2`, ...; `context.key` is the message id, `extra["index"]` the
  0-based message position that LocOS calls `p_index`). Messages with no id, a repeated id (Angular keeps the first),
  or only placeholders are not units; they are copied as they are. `<source>` locations are never translated.
- `<ph name="X">` is a tag. `START_X` ... `CLOSE_X` that nest properly are an `Open`/`Close` pair (LocOS `g` token); every
  other `ph`, an unpaired START/CLOSE, an unknown child element and a comment or processing instruction is a
  `Standalone` tag (LocOS `x` token). The display is the placeholder's original text, else its `<ex>` example, else `{NAME}`.
- Notes: `context.notes` has one note like LocOS: `Description: .. | Meaning: .. | Placeholders: NAME = example; ..`
  (cut at 2000 characters); `extra["description"]` and `extra["meaning"]` have the parts.
- ICU expressions (`{VAR_PLURAL, plural, =0 {..} other {..}}`) are message text, as in LocOS: translators edit the case
  bodies and keep the keywords.
- Edge white space of a message stays outside the unit and comes back unchanged.
- **XTB, an untouched merge**: the original bytes (BOM, newlines, encoding, comments, quotes, spacing, `<ph name="x" />`).
  A changed translation replaces only the content of its `<translation>`, with the original `<ph>` markup. The
  document's target language becomes `lang` (the existing attribute is edited in place, quote style kept; added when
  missing) only when some translation really changed; `Xtb.merge_localized` writes it always.
- **XMB, an untouched merge**: the original bytes. **With a translation it gives the XTB** Angular reads: LocOS's export
  (`<?xml ..?><!DOCTYPE translationbundle><translationbundle lang="..">`, one `<translation>` per distinct id in XMB
  order, ids byte for byte, empty `<ph name=".."/>`, untranslated messages and placeholder-only messages copied with their
  source text). `Xmb.to_xtb(&doc)` always writes the XTB.
- Tolerant writer: an unknown or repeated tag and a stray close are dropped, an unclosed pair is closed.
- Encodings: UTF-8 (with or without BOM), UTF-16 with a BOM, and the legacy code pages an XML declaration names (the
  engine's `enc` rules). A character a legacy page cannot hold becomes `&#N;`.
- Safety: the engine's XML guard (no `<!ENTITY`, depth 256, 4M elements), no external DTD (SYSTEM/PUBLIC refused), the
  DOCTYPE and its internal subset are kept but never read, 40 markup characters per allowed unit, a strict reader
  (balanced, XML characters, defined entities, unique attributes). Nothing recurses. Limits: `Xmb.extract_with(.., &Limits)`.
- `sniff` is tight (the first start tag is `<messagebundle` / `<translationbundle`).

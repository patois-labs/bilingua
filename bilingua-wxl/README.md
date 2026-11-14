# bilingua-wxl

WiX localization (`.wxl`) filter for Bilingua.

```rust
use bilingua_wxl::Wxl;
use bilingua_model::*;

let mut doc = Wxl.extract(&bytes, &ExtractOptions::new().with_target_lang("de-DE"))?;
doc.set_target("u1", vec![Inline::text("Hallo")])?;
let out = Wxl.merge(&doc)?; // Culture="de-de" on the root, everything else as it was
```

## What it does

- Reads `<String Id=".." Value=".."/>` (WiX 4 and later), `<String Id="..">text</String>` (WiX 3) and the `Text`
  attribute of `<UI Dialog=".." Control=".." Text=".."/>`, as direct children of the root. Any namespace and any
  prefix. `Localizable="no"` / `"false"` is skipped (a locked unit with `include_locked`). A `<String>` with a child
  element is skipped; so is one with comments between two runs of text. Comments around a single run of text
  (the `_locID_text` comments WiX loc tools write) are left in place and the text is the value (LocOS skips these). Values with no letter or digit are not units.
- `context.key` is the `Id` (or `Dialog.Control`), `extra["overridable"]` and `extra["localizable"]` are the
  attributes, `extra["p_index"]` is the LocOS slot index. Unit ids are `u<p_index + 1>`.
- `[ProductName]`, `[#File]`, `[!File]`, `[$Comp]`, `[%ENV]`, `[1]`, `[~]`, `[\c]` are free standalone tags
  (code label `placeholder`, display = the exact text). The LocOS filter keeps them as plain text; the adapter
  flattens them back.
- Merge changes only the translated attribute value or element content. Declaration, comments, processing
  instructions, DOCTYPE, CDATA (kept as CDATA unless the translation holds `]]>`), attribute order and quoting
  (`'` or `"`), white space, line endings, BOM and the original spelling of unchanged values (`&#233;`) stay.
  Characters XML cannot hold are removed from a translation.
- `Culture` on the root is rewritten (lower case, as LocOS) when `target_lang` is set and a unit is translated;
  added when missing. `Codepage` and `Language` are never touched (set them for the target before building).
- Encodings: UTF-8 (BOM kept), UTF-16 with a BOM, and the legacy charsets an XML declaration names (Windows-125x,
  Shift_JIS, GBK, EUC-KR, ...) through `bilingua_model::enc`; written back in the same encoding, in UTF-8 when a
  translation does not fit.
- Safety: `bilingua_model::guard` (input size, entity declarations, XML depth and element count, unit count and
  length), then a strict well-formedness check (undefined entities, forbidden characters, `<` in attributes,
  repeated attributes, several roots, text outside the root). No DTD is read and no entity is expanded.

## Differences from LocOS and Okapi

- LocOS always exports UTF-8; Bilingua writes the encoding the file came in.
- LocOS stamps `Culture` on every localized build; Bilingua only when something was translated.
- LocOS cleans illegal characters only in element content; Bilingua also in attribute values.
- LocOS refuses a DOCTYPE with an internal subset; Bilingua refuses entity declarations (the B6 guard) and accepts
  the rest, keeping it byte for byte.
- Okapi `okf_xml-WixLocalization` reads only text-form strings, and (WiX 3 namespace) only those with an
  `Overridable` attribute other than `no`. It reads no `Value` attribute and no `<UI>`. Every unit Okapi finds is
  in the Bilingua list with the same text.

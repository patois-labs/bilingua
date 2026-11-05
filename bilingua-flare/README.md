# bilingua-flare

MadCap Flare projects (`.flprjzip`) for Bilingua, written to match the Localization OS `flare_filter`. Every test of
the LocOS suite `verify_flare_filter.py` that is about the file is ported to `tests/locos_flare.rs`.

## What is a unit

Zip members in order, and inside a member:

* topics and snippets (`.htm .html .xhtml .flsnp`), through `bilingua-html`: block text, then the accessibility
  attributes LocOS offers (`alt`, `title`, `aria-label` on `img` and `area`; `title`, `aria-label` on `a`), then the
  index keywords of `MadCap:keyword term="..."` (`;` separates keywords, `:` levels, `\:` is a literal colon);
* `.flvar`: the text of each `Variable` that is plain text (`Type` empty or `text`) and holds a letter; a matching
  `EvaluatedDefinition` attribute is written with it;
* `.fltoc`: the `Title` of each `TocEntry`; `[%=...%]` tokens are standalone codes, a title that is only tokens is
  skipped;
* `.flglo`: `Term` and text-only `Definition`.

Everything else is copied byte for byte (project file, targets, condition tag sets, style sheets, images, skins,
`__MACOSX`). Attributes are never changed except the translated ones; `MadCap:conditions` is never touched.

## How the MadCap inline elements reach the HTML filter

`bilingua-html` knows the HTML inline elements only. Before a topic is read the MadCap phrasing elements are swapped
for an HTML inline element that carries a number (`<span mcf="3">`, `</span mcf="3">`, `<wbr mcf="3"/>`), and the
original markup is kept in a table; after the HTML filter has written, every mask is swapped back. The marker name is
chosen so it is not in the topic. This keeps the HTML filter unedited and costs one linear pass each way.

## Merge

* Nothing translated: the original bytes. A unit whose target equals its source writes nothing.
* Something translated: all members in order, with their compression and time stamps; members that did not change are
  copied without recompression; the archive comment is kept.
* With a target language and a translation (or `stamp_target_always`): `lang`, `xml:lang` (when present) and `dir`
  on each topic's `<html>`, and the content-language `<meta>`.
* Targets are plain text: `&`, `<`, `>` escaped, characters XML 1.0 forbids dropped.

## Refusals

Not a zip; a zip with no Flare file; unsafe, repeated or link members; zip bomb or too many members; a part that is
not UTF-8 (UTF-16 included); a DTD or entity declaration; a structured part that is not well-formed XML; nesting
deeper than the limit; too many units or too much markup.

## Known differences from LocOS

* Character references: LocOS keeps `&#160;` and `&eacute;` in topic unit text as typed; Bilingua decodes them and
  writes them back escaped.
* A translation is text, not markup: LocOS rewrites `&nbsp;` in a target to `&#160;`, Bilingua writes the five
  characters (`&amp;nbsp;`).
* `<code>` is one standalone (protected) code here, a paired code in LocOS (`bilingua-html` behaviour).
* A `<title>` holding markup is kept as it is (locked); LocOS offers it with the markup as codes.
* Orphan end tags of inline elements are kept in place; LocOS drops them from a changed unit.

## What other crates would need

* `bilingua-zip`: keep the archive comment and each entry's comment and permission bits (this crate patches the
  comment onto the new end record; entry comments and unix modes are lost on a rewrite).
* `bilingua-html`: an option for extra inline element names (then the masking here would go).

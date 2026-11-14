# bilingua-epub

EPUB 2 and EPUB 3 publications (`.epub`) for Bilingua. Written to match the Localization OS `epub_filter` (the Okapi
1.48.0 build has no EPUB filter); every case of its test suite is ported to `tests/locos_epub.rs`.

## What is a unit

In this order:

1. OPF metadata: every text-only `dc:title` and `dc:description` (`opf-1`, ...).
2. Every XHTML content document in spine order, read by `bilingua-html` (`h1-u1`, ...): block text with inline
   `Open`/`Close`/`Standalone` codes, then the accessibility attributes LocOS offers (`alt`, `title`, `aria-label` on
   `img` and `area`; `title`, `aria-label` on `a`). The EPUB 3 navigation document, when it is not in the spine, comes
   after the spine chapters.
3. The EPUB 2 NCX: `docTitle`, `navLabel` and `navInfo` text (`ncx-1`, ...).

CSS, fonts, images, audio, scripts and the rest of `META-INF` are never touched. Font obfuscation is accepted; any
other encryption is refused (`Error::Unsupported`).

## Merge

* Nothing translated: the original bytes.
* Something translated: `mimetype` first, stored, no extra field; the other members in their original order, with
  their original compression and time stamps; members that did not change are copied without recompression; OPF
  and NCX are edited in place (only the changed text spans and the language attributes change); XHTML goes through the
  HTML filter.
* With a `target_lang`: `dc:language`, package `xml:lang`, `page-progression-direction` for a right-to-left target,
  `lang`, `xml:lang` and `dir` on every `<html>`, the content-language `<meta>`, the NCX `xml:lang`.
* Each XML member keeps its encoding (UTF-8 with or without a mark, UTF-16, a declared legacy charset). An XML member
  that is neither UTF-8 nor UTF-16 and declares nothing is a typed `Error::Encoding`.
* Targets are plain text; `&` is always escaped and characters XML 1.0 forbids are dropped, so the result is
  well-formed whatever a translation holds.

## Safety

The zip goes through `bilingua_zip::read_zip` (size, entry count, ratio, path escapes, duplicates, real sizes),
every package XML part through the XML guard and a strict reader (no DTD, no entities, depth and element caps), each
XHTML chapter through an entity and depth check. A manifest or spine reference that leaves the package, points to a
remote URL or to a missing member is skipped, never read.

## Tests

`cargo test -p bilingua-epub`: `locos_epub` (the LocOS suite), `behaviour`, `hostile` (with proptest), `oracle`
(LocOS units on the corpus), `speed` (`cargo test --release -p bilingua-epub --test speed -- --nocapture`; set
`EPUB_SPEED_OUT` to save the file for `tools/time_locos.py`). `tools/gen_corpus.py` writes the corpus,
`tools/gen_oracle_units.py` the LocOS answers.

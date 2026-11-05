# bilingua-image

SVG text and the text layer of raster images for Bilingua. Port of Localization OS's `app/filters/image_filter.py`.

## SVG (`SvgFilter`, id `svg`)

* A unit is the text inside `<text>`, `<tspan>` and `<textPath>`, cut at every tag (so `a <tspan>b</tspan>` is two
  units), whitespace stripped and put back, entities decoded once and encoded once. Text with no letter or digit is
  not a unit.
* `<title>` and `<desc>` whose whole content is one text node are units too, appended after the text units
  (`context.style` is `a11y:svg-title` / `a11y:svg-desc`). Inside `<style>`, `<script>`, `<metadata>` they are not.
* Everything else is kept byte for byte: paths, styles, scripts, comments, attributes, the XML declaration.
* A merge for a target language stamps `xml:lang` (and `lang` when present) on the root `<svg>`, and for a
  right-to-left target sets `direction="rtl"` and mirrors `text-anchor` (start <-> end) on every translated `<text>`.
* Writing: `&`, `<`, `>` are escaped; control characters XML 1.0 forbids are dropped (vertical tab and form feed
  become a newline), so no translation can make the file ill-formed or add markup.
* Reading: UTF-8, UTF-16 and the legacy encodings a file declares (the model's text reader); the declaration is
  binding. Entity declarations (`<!ENTITY`) and nesting over 256 levels are refused before parsing; entities are
  never expanded.
* Differences from LocOS, on purpose: text the LocOS tag splitter cuts out of a comment, CDATA section, DOCTYPE or
  quoted attribute value (a `>` there ends its tag early) is not a unit, and a warning says how many were left alone;
  XML-illegal control characters in a translation are dropped; the declared charset is rewritten when the output is
  UTF-8 even for a pure-ASCII file (LocOS leaves a stale declaration there).

## Raster images (`ImageFilter`, id `image`)

LocOS finds the text of a PNG in a JSON sidecar (`loc_tms_regions`, a `tEXt`, `zTXt` or `iTXt` chunk) through its
mock detector; real OCR and vision stay in LocOS. Bilingua reads that sidecar: one unit per region in reading order
(top to bottom, then left to right, at most 100), geometry in `context.extra["image_region"]`. A merge rewrites the
`text` of the translated regions in the sidecar, in a chunk of the same kind (`zTXt` becomes `tEXt`), and recomputes
its checksum; pixels and every other byte are untouched. Nothing translated: the original bytes.

Checks, as LocOS (Pillow) makes them: chunk checksums, header, 8000 px per side and 25 megapixels, one frame (APNG,
animated GIF/WebP and multi-page TIFF are refused with LocOS's message), pixel data inflated to prove it is complete,
text chunks capped at 1 MB each and 64 MB in all. JPEG, GIF, BMP, WebP and TIFF headers are checked the same way and
give no units.

Tests: `tests/locos_golden.rs` (782 files against LocOS output, made by `tests/tools/gen_golden.py`),
`tests/locos_ported.rs` (the LocOS suite cases), `roundtrip.rs`, `hostile.rs` (proptest included), `speed.rs`
(`tests/tools/speed_locos.py` times LocOS on the same file), `tests/tools/okapi_compare.py` (Okapi okf_xml).

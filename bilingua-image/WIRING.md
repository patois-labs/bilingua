# Wiring bilingua-image (the lead replaces the throwaway wire commit at merge)

- Two filters from one crate. Format ids and extensions:
  - `bilingua_image::SvgFilter`: id `svg`, extension `svg`. Sniff is tight: the root element (after an XML
    declaration, comments, processing instructions and a DOCTYPE) is `<svg`, in UTF-8 or UTF-16. An HTML page with an
    inline `<svg>` does not match.
  - `bilingua_image::ImageFilter`: id `image`, extensions `png jpg jpeg tif tiff bmp gif webp`. Sniff is the raster
    magic numbers (PNG, TIFF, GIF, WebP; JPEG needs `FF D8 FF`; BMP also needs a DIB header size). Given SVG bytes it reads them as
    `SvgFilter` does (the LocOS `image` filter does the same), and then `Document::format` is `svg`.
- Facade `filters()`: add both boxes anywhere before `HtmlFilter` and the text filters (done in the wire commit, after
  `Xaml`). The sniffers are strict, so the exact place does not matter beyond that.
- Facade `Cargo.toml`: `bilingua-image = { workspace = true }`. Workspace: member `bilingua-image` and
  `bilingua-image = { path = "bilingua-image" }`. Dependencies new to the workspace graph: none (`flate2` with the
  `zlib-rs` backend and `crc32fast` are already in the lockfile through `zip`; both build for wasm). Dev-dependencies
  `proptest`, `quick-xml`, `serde_json` are workspace ones.
- `bilingua/tests/contract.rs` `crate_of`: `"svg" | "image" => "bilingua-image"`.
- `tools/check-portable.sh` and `.ps1`: add `image` to `CRATES`.
- Oracle corpus: `oracle/corpus/svg/` (31 files) and `oracle/corpus/image/` (4 PNGs), made by
  `bilingua-image/tests/tools/gen_corpus.py`. `oracle/run.py` `FORMATS` would need `svg: svg` and `png: image`
  (the LocOS dump script already routes `.svg` and `.png` to its image filter). Row for `oracle/corpus/SOURCES.md` is in
  the wire commit.
- Options: none. No `format_options` keys. Merge options honoured: `output_encoding` (`Keep` writes the file's own
  encoding, `Utf8` UTF-8 with the XML declaration rewritten) and `stamp_target_always` (through
  `MergeOptions::stamp_lang`). For the LocOS adapter use `profile::locos().merge_options("svg")` (and `"image"`).
- What the LocOS adapter must know:
  - LocOS numbers SVG units text first, then `<title>` / `<desc>` (appended, style `a11y:svg-title` /
    `a11y:svg-desc`): Bilingua's unit order is the same, and `Unit::context.style` holds the style.
  - Raster: LocOS draws the translation onto the pixels (`compose_overlay`, fonts, erase). Bilingua does not: its
    `ImageFilter::merge_with` writes the translated text back into the region sidecar and leaves every pixel byte
    alone. The adapter keeps calling the LocOS overlay for the deliverable image and uses Bilingua for extract.
    Region geometry (the full LocOS contract as JSON) is in `Unit::context.extra["image_region"]`, the bbox in
    `Unit::context.location` (`x,y,w,h`).
  - An image with no sidecar (and every JPEG/GIF/TIFF/BMP/WebP) is a document with zero units and a warning of kind
    `no-text-layer`; OCR is LocOS's.
  - An SVG with no units in a legacy code page that does not round trip has a `Skeleton::Binary` (format `svg`, so
    no `source_encoding` from the skeleton); both filters merge it.
- Python wheel: nothing format-specific; it goes through the facade.

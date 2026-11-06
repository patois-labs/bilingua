# Migrating to Bilingua 1.0 (model round 2)

What changed in the public API before 1.0, for format-crate builders and for the Localization OS adapter. Every
item below breaks code written against 0.1; after 1.0 these types grow without breaking callers.

## Everyone

- **Build options and model values with constructors, not struct literals.** `ExtractOptions`, `MergeOptions`,
  `Limits`, `Document`, `Unit`, `Segment`, `UnitContext`, `CodeData`, `Note`, `Origin`, `ParserInfo`, `ZipEntry`,
  `Warning` and `LimitError` are `#[non_exhaustive]`; outside `bilingua-model` a struct literal (even with
  `..Default::default()`) no longer compiles.

  ```rust
  // before
  let o = ExtractOptions { target_lang: Some("de-DE".into()), limits: Some(Limits { max_units: 500, ..Limits::default() }), ..Default::default() };
  // after
  let o = ExtractOptions::new().with_target_lang("de-DE").with_limits(Limits::new().with_max_units(500));
  let m = MergeOptions::new().with_output_encoding(OutputEncoding::Utf8);
  let d = Document::new("txt", units, skeleton).with_options(&o);
  let s = Segment::new("u1", source).with_target(target).with_state(SegState::Translated);
  let u = Unit::from_segments("u1", segments).with_translate(false).with_context(UnitContext::new().with_kind("cell").with_max_len(Some(255)));
  let c = CodeData::paired("<b>", "</b>").with_label("bold");     // or CodeData::standalone("<br/>"), CodeData::new().with_open(..)
  let n = Note::new("Check the term").with_author("ana").with_kind("comment");
  let o = Origin::new(OriginKind::Tm).with_name("Acme TM").with_match(94);   // with_match_pct(Option<u8>) for an optional one
  let e = ZipEntry::raw("word/document.xml", bytes);              // or ZipEntry::new(name, ZipData::Parts(parts))
  ```

  Optional text fields take `"de"`, a `String` or an `Option` of either (`with_target_lang(opts.target_lang.clone())`):
  the setters take `impl IntoOptionString`, a sealed trait (`impl Into<Option<String>>` would refuse a plain `&str`).
  A value built with `.into()` must name its type first (`let s: String = x.into();`), since the setter is generic.
  Fields stay public for reading and assigning. `MergeOptions` is no longer `Copy` (`Limits` still is).
- **Inline codes.** `Inline::Tag` is a `#[non_exhaustive]` variant: build it with `Inline::tag(id, kind, display)`,
  `Inline::free_tag(id, display)` or `Inline::tag(..).with_free(bool)`, and match it with `..`
  (`Inline::Tag { id, kind, .. }`). `Inline` itself (text or a code) and `TagKind` stay closed.
- **Match with a wildcard arm.** `Error`, `LimitKind`, `OutputEncoding`, `Granularity`, `SegState`, `OriginKind`,
  `Skeleton`, `OptionKind`, `bilingua_xliff::Error` and `bilingua_seg::SrxError` are `#[non_exhaustive]`.
  `Skeleton`'s variants keep their fields for 1.x (new kinds of skeleton come as new variants); `SkelPart`,
  `ZipData`, `TagKind` and `Inline` are closed by design.
- **`format_options` are typed.** Each filter declares its keys (`Filter::format_option_keys`, all `<family>.<name>`).
  `bilingua::extract` and `bilingua::merge_with` refuse a key no filter declares, or a value of the wrong kind, with
  `Error::FormatOption { key, msg }`; a key for another format than the file's is accepted, so one option map can go
  with every file. A filter called directly (`CsvFilter::default().extract(..)`) refuses an undeclared key of its
  own families (`csv.colums`) and a bad value for one of its keys, and leaves other formats' keys alone (it cannot
  tell a good foreign key from a misspelt one; only the facade knows every filter's keys).
  `bilingua::format_option_keys()` (CLI: `bilingua options`) lists them.
- **`Skeleton::Binary`** is new (mo and stl use it); **`Unit::hidden`** holds parser-private values that used to be
  `~` codes in `Unit::codes`. Old JSON documents still read (the `~` codes move into `hidden`).
- **Empty translations.** An empty `target` still means untranslated (the source is written).
  `Document::set_target_empty(id)` marks a segment as translated to nothing (`Segment::empty_target`); it is written
  empty. `Segment::has_target()` / `Unit::is_translated()` are the tests to use.
- **Profiles.** `bilingua_model::locos` (the token conversion) is now `bilingua_model::profile::tokens`;
  `MergeOptions::locos()` is gone: use `profile::locos().merge_options(format)`. `bilingua_ttx::Ttx::locos()` is gone:
  extract under the LocOS profile (the facade's TTX filter then skips finished translations, as LocOS does), or build
  `Ttx::new().with_skip_translated(true)`.
- **`Error::Unwritable { unit, reason }`** is new: the translation cannot be written in this format (does not
  fit, a character has no code in the code table, it would read back as structure). `Error::Write` now only means
  a broken or foreign skeleton. rc, properties, strings, scc, stl, subtext, htmlhelp (NUL, code page, a sitemap a
  browser would read differently) and yaml (text that does not fit the file's encoding) use it;
  `Error::UnknownProfile` and `Error::FormatOption` are new too.
- **A declared encoding is binding.** `enc::decode_text` refuses bytes that are not valid in the encoding a file
  declares (XML declaration, meta charset, or a BOM in a declared format) with `Error::Encoding`, where it used to
  fall back to a code page. Undeclared formats still fall back, as LocOS's `read_text_upload` does; plain text and
  subtitles read a UTF-8 BOM plus invalid bytes as UTF-8 with U+FFFD (LocOS). `enc::DecodeOptions` is
  `#[non_exhaustive]`: `DecodeOptions::new(&opts)` or `DecodeOptions::with_limits(l).with_source_lang(..).declared()`.
- **The encode path never writes a file its own extract refuses.** `enc::encode_text` (and `encode_text_with` with
  `Keep`) writes UTF-8 when a legacy code page result would read back as binary (a merge that shortened a file below
  its control-byte allowance); a crate with its own legacy codec passes its bytes through
  `enc::legacy_output_or_utf8`.
- **Per-crate option and filter types.** These are `#[non_exhaustive]` too; build them from `Default` / `new()` and
  assign or use their `with_*` methods: `bilingua_csv::{CsvOptions, CsvFilter, TsvFilter, Spec, HeaderMode}`,
  `bilingua_xlsx::{XlsxOptions, Xlsx}`, `bilingua_odf::OdfOptions`, `bilingua_mdx::MdxOptions`,
  `bilingua_json5::Json5Filter`, `bilingua_latex::Latex`, `bilingua_ttx::Ttx` (`Ttx::new().with_skip_translated(..)`),
  `bilingua_wordfast::{Txml, Txlf}` (`Txml::new().with_load_targets(..)`, const), `bilingua_xaml::Xaml`
  (`Xaml::new()` / `Xaml::with_view_attributes()`, const), `bilingua_xml::{AttrRule, Dialect}`, the per-format
  `Limits` of arb, ftl, ini, jsmodule, json5, properties, resjson, strings and yaml
  (`let mut l = Limits::default(); l.max_units = 5;`), the parity `Record`s (desktop, ini, jsmodule, php, plist,
  properties, resx, stf, strings, xcstrings) and `stf::OverLimit`, the `MergeReport`s of docx, pptx and xlsx, sdl's
  `FileRole`, `PackageFile`, `SdlPackageKind`, `ReturnStamp` and `Entry`, `sdlproj::ProjectInfo`,
  `serverpages::Dialect`, xliff's `Format`, `MxMeta`, `SegView`, `RawTag` and `TagDef`, and `bilingua_zip::ZipLimits`.
  `bilingua_csv::plan`, `bilingua_idml::{story, xml}`, `bilingua_yaml::scan` and `bilingua_zip::hostile` are
  `#[doc(hidden)]`: shared with other Bilingua crates and their tests, not part of the semver API. The rest of
  `bilingua_zip` (`EntryMeta`, `RawEntry`, `Packed`, `Body`) is plumbing for Bilingua's own zip formats and moves in
  lockstep with them.
- **bilingua-html (D4).** The filter is now LocOS's `HtmlFilter` on the engine it shares with the server pages
  (`bilingua-htmlcore`, plumbing, not semver API). `Html` stays; `HtmlFilter::new().with_page_meta(false)` and
  `.with_template_pattern(re)` are the LocOS variants (`#[non_exhaustive]`). Units change: block units first, then
  the accessibility attributes, then the page metadata (ids move); character references stay as typed in unit text
  and a target is markup text (a bare `&`, `<`, `>` is escaped, a reference is kept); `code`/`kbd`/`samp` are paired
  codes with their text, `pre`/`textarea`/`<title>` markup are read as LocOS reads them; `placeholder`, `value`
  and `title` of other elements are no longer units; `translate="no"` text is never a unit (`include_locked` has no
  effect). Block units have no `style` (LocOS's), attribute units have LocOS's (`a11y:img-alt`, ...), `key` the
  attribute and `extra["element"]` the element; the hidden `~raw`/`~attr` values are gone (`v` = `attr` stays,
  `bilingua_html::VALUE`). `SKELETON_SCHEMA` is 2 (the skeleton is the page); a schema 1 document with units is
  refused at merge with `Error::ParserVersion` (Python `code == "parser_version"`): open the file again. EPUB,
  e-mail and Flare are at schema 2 for the same reason (their units are the HTML filter's) and refuse the same way. A `.html` page with closed server code is read through
  `bilingua-serverpages`. EPUB, e-mail and Flare use `with_page_meta(false)`; for an XHTML target they write
  `bilingua_html::xml_named_refs` (LocOS `_xml_named_refs`) instead of the old `&` sentinel.
- **bilingua-xml / bilingua-idml.** `MlConfig`, `MlInfo`, `Inspection`, `LangStamp`, `Detect` and `MlMode` are
  `#[non_exhaustive]` (`MlConfig::new().with_source("en")`, `LangStamp::new().with_update(["xml:lang"])`,
  `Detect::new().with_root_names(["recipe"])`); idml's `story::Where`, `Point` and `Part` are private.
- **`sdl` limits.** `bilingua_sdl::ZipLimits` is gone: `SdlPackage::from_bytes_with_limits` and
  `MqPackage::from_bytes_with_limits` take `bilingua_model::Limits` (LocOS's numbers by default: 10 000 entries,
  256 MB unpacked; raise them for desktop use with `Limits::new().with_max_zip_bytes(..).with_max_zip_entries(..)`).

## Format-crate builders (and the branches still on the old API: Flare, DXF, PDF, SVG and images)

A checklist for a rebase onto this round, in the order the compiler will ask for it:

1. **`Filter::merge_with(doc, &MergeOptions)` is the required method**; `merge(doc)` is provided. Rename your
   `merge` to `merge_with`, take the options, and honour both:
   - `output_encoding`: `Keep` writes the file's own encoding; `Utf8` writes UTF-8 literally (a UTF-8 BOM is kept,
     UTF-16 and code pages are not). Use `enc::encode_text_with`, or `enc::utf8_output` when the crate has its own
     codec.
   - stamping: ask `opts.stamp_lang(doc, translated)` (or `opts.should_stamp(doc)`), never `stamp_target_always`
     directly. It returns the target language only when it is not blank and something was translated or
     "always" is set.
2. **Start `extract` and `merge_with` with `bilingua_model::check_own_options(self, &opts.format_options)?;`**
   (every Bilingua filter does), and **declare your `format_options` keys** in `Filter::format_option_keys()` as
   `FormatOptionKey::bool/int/text`, with the `<format>.` prefix (filters that share a family, like csv and tsv or
   the four odf filters, all declare every key of it; `tests/format_options.rs` in the facade checks this).
3. **Struct literals of model types** become constructors (see "Everyone"): `Inline::Tag { .. }` construction ->
   `Inline::tag(..)`, patterns get `..`; `CodeData { standalone: Some(x), ..Default::default() }` ->
   `CodeData::standalone(x)`; `UnitContext { kind: Some(k), ..Default::default() }` -> `UnitContext::new().with_kind(k)`;
   `ZipEntry { name, data: ZipData::Raw(b) }` -> `ZipEntry::raw(name, b)`; `ParserInfo { .. }` -> `ParserInfo::new`.
4. **Binary files use `Skeleton::Binary`.** A crate that keeps the original bytes of a non-zip file (mo and stl do;
   PDF and images will) stores them as `Skeleton::binary(bytes, parser_info!())`, not as a one-entry
   `Skeleton::Zip`; if it shipped the zip form, bump `SKELETON_SCHEMA` and keep reading the old form in `merge_with`
   (see bilingua-mo's `original_bytes`).
5. **`Error::Unwritable { unit, reason }`** for a translation the format cannot hold (keep LocOS's message as
   `reason`); `Error::Write` only for a broken or foreign skeleton.
6. **Use `Unit::hidden`** (`set_hidden`, `hidden`, `has_hidden`, `with_hidden`) for values only your writer needs,
   not `~` codes in `codes`. (Several older crates still keep such values in a `$name` code; they move in a later
   round, with a JSON migration like the `~` one.)
7. **Translated = `Segment::has_target()`**, not `!target.is_empty()`, so empty translations are written.
8. **Declared encodings.** Decode with `enc::decode_text(bytes, &DecodeOptions::new(opts).declared())` when the
   format declares its charset; leave `.declared()` off when it does not.

### Formats where `Utf8` has no effect (documented exceptions)

`output_encoding: Utf8` changes nothing for these, on purpose; the LocOS profile asks them for `Keep` where LocOS
keeps the encoding:

- **sdlproj**: the project manifest is kept byte for byte (its gate's decision).
- **dxf**: a drawing declares its code page (`$DWGCODEPAGE`) and AutoCAD before 2007 has no UTF-8; LocOS keeps the
  code page too (lead decision 11-05). In the LocOS profile's `Keep` list with ini and strings.
- **email**: each MIME part keeps its declared charset.
- Formats that are UTF-8 already: epub, idml, icml, mif, odf's XML parts, the Office formats.

## The Localization OS adapter

- Extract with `profile="locos"` (Rust: `ExtractOptions::new().with_profile("locos")` or
  `profile::locos().extract_options()`); merge with `profile::locos().merge_options(&doc.format)` (Python:
  `merge(doc, profile="locos")`). The profile means UTF-8 out except ini, strings and dxf (kept in their own
  encoding, as LocOS does) and the target language stamped even when nothing is translated. TTX under the profile
  skips `<Tu>`s with a finished translation, as LocOS does.
- Under the profile, units carry `context.extra["locos.p_index"]` where the format knows LocOS's number (odf, Qt ts,
  wxl, icml, idml, resx, json) and `["locos.style"]` where it knows LocOS's style (md, dtd); unset means "fall back to
  position".
- Track D group A (txt, md, srt, vtt, po, json, strings, properties, ftl, dtd, resx, wxl, xaml, json5, arb,
  resjson): under the profile these give LocOS's units, ids, styles and builds. txt, md, srt/vtt and po are ports
  of the LocOS filters (one unit per line for txt and md, LocOS's cue tokens for subtitles, LocOS's PO build with
  CLDR plural slots and a Language header); their profile documents carry the decoded file as a
  `Skeleton::Binary`. In the key/value formats the placeholder codes are plain text again (ARB keeps its ICU
  arguments as codes, FTL finds its placeables in the translation again); JSON walks like `json.loads` (the
  last of a duplicate key, every string with a letter or digit, `NaN`/`Infinity` read). `locos.style` is set for
  md images (`a11y:md-image`) and dtd (`dtd:<name>`), and `locos.p_index` for resx and json. The goldens are in
  `bilingua/tests/locos/` (made by `oracle/locos_golden.py` with the LocOS venv) and checked by
  `bilingua/tests/locos_golden.rs`. Not ported: Markdown's CJK emphasis and autolink repairs, LocOS's JSON
  resource dialects (Chrome, FormatJS, Structured, go-i18n) and i18next plural fill.
- `Document::source_encoding` names the encoding Bilingua decoded a non-UTF-8 text file from (`utf-16le`,
  `windows-1252`, ...).
- A document stored before D4 for `html`, `epub`, `eml` or `flprjzip` (skeleton schema 1, with units) is refused at
  merge with `parser_version`: rebuild it from the stored original (the parser stamp plus hash rule) and carry the
  translations over by text, not by unit id; ids and order changed.
- An empty LocOS target is a translation to nothing: `profile::locos_set_target` (Python: set
  `"empty_target": true` on the segment).
- Python: `merge(document, *, profile=None, output_encoding=None, stamp_target_always=None, format_options=None)`
  is keyword-only (the positional options dict is gone; a keyword given with a profile overrides the profile's
  value); `extract` takes `profile=`; `format_options` values may be str, bool or int; `bilingua.profiles()` returns
  `{"locos": 1}`. A bad option key or value or an unknown profile raises `BilinguaError` with
  `code == "invalid_argument"`; a translation the format cannot hold raises `code == "unwritable"`. The stubs
  (`bilingua.pyi`) ship in the wheel and a test checks them against the module.
- Python `extract`'s default granularity is `"paragraph"` (what the adapter uses); Rust's `ExtractOptions` default is
  sentence.
- Track D group B, XLIFF family (xliff, sdlxliff, mqxliff, mxliff, xlz, mqxlz, sdlppx/sdlrpx): under the profile
  these are ports of LocOS's `xliff_filter.py`, `cat_xliff.py`, `xliff_package.py` and `sdl_package.py`
  (`bilingua_xliff::locos`). Units are LocOS's (protected units, blank sources and code-only plain sources left
  out); `locos.p_index` is LocOS's key (the `<source>` position, `1000000000` plus the seg `mrk` position, and in a
  package the member's offset plus the unit's walk slot). Tag ids are LocOS's token numbers; `bpt`/`ept`/`it` and
  the other begin/end codes are standalone codes, `<x id>`/`<ph id>` free unless they stand for a markup side or an
  unnumbered argument, duplicate `g` ids are separate pairs, comments in a source are pairs as in lxml. A plain
  XLIFF is written the way lxml serialises it in LocOS's envelope (a new `<target>` after its `<source>` with no
  `state`, `new`/`needs-translation` become `translated`, `target-language` or `trgLang` set on every file from
  the canonical tag, `es_ES` giving `es-ES`); vendor and seg-source files are LocOS's byte splice. A Trados package
  is LocOS's one-target-file document and comes back as LocOS's return package (the `.sdlproj` with
  `ReturnPackage` and the rebuilt SDLXLIFF only); `.mqxlz` goes through the XLIFF package port. The profile's
  documents carry the original as a `Skeleton::Binary`. Known gap: an MXLIFF segment mixing element codes and
  `{1}` text marks numbers its tokens in LocOS's order (elements first), which `profile::tokens` renumbers by
  position.
- Track D group B, PPTX (and pptm, potx, potm, ppsx, ppsm): under the profile this is a port of LocOS's
  `pptx_filter.py` (`bilingua_pptx::locos`). The walk is LocOS's: per slide in `p:sldIdLst` order the slide's
  paragraphs (outside `mc:Fallback` and merged table cells), its SmartArt data and charts in reference order, its
  first notes slide, then the slide's alt text (`descr`, `title`); slide layouts and masters are not units.
  `locos.p_index` is the paragraph's slot in that walk (empty paragraphs counted, so there are gaps; alt text after
  the last paragraph) and `locos.style` LocOS's style (`slide 1`, `slide 1 SmartArt`, `slide 1 chart`,
  `slide 1 notes`, `slide 2 hidden`, `a11y:img-alt`, `a11y:img-title`). A paragraph with one run is plain text;
  otherwise every run is a pair whose tag id is LocOS's run number and every `a:br` / `a:fld` a standalone code
  numbered after the runs. LocOS does not number those tokens in reading order, so the adapter must turn them into
  text with `profile::tokens::to_locos_text_for("pptx", ..)`, which for pptx is the new `to_locos_text_numbered`
  (tag ids as token numbers; it falls back to `to_locos_text_as` when the ids are not usable numbers), to get
  LocOS's `⟦r1⟧a⟦/r1⟧⟦x3⟧⟦r2⟧b⟦/r2⟧`. Writing is
  LocOS's `_rebuild` (its token rules on the target text, run properties copied, a dropped break or field put back
  at the end, new lines as `a:br`, `lang` on every run of a written paragraph and `rtl="1"` for a right-to-left
  target, the SmartArt drawing cache mirrored), re-serialising only the parts that changed; Strict Open XML goes
  through LocOS's namespace swap. The profile's documents carry the original as a `Skeleton::Binary`. Not ported:
  slide comments (LocOS's `comments=True`), LocOS's markup budget, XML parts in a legacy code page.
- `profile::lang_tag` and `profile::is_rtl` are LocOS's `html_lang` and `text_direction`.
- LocOS export keywords are accepted and ignored for now: `docx.walk`, `xlsx.walk`, `pptx.walk`, `xlsx.table`,
  `csv.table`, `markdown.walk`, `markdown.md_walk`, `markdown.fm_keys`, `json.dialect`.

## CAT OS core (note for the lead)

- `catcore` re-exported `bilingua_sdl::ZipLimits`; it now re-exports `bilingua_model::Limits as PackageLimits`
  (`catcore/src/lib.rs`), and `catcore/tests/packages.rs` builds it with `PackageLimits::new().with_max_zip_*`.
  `SdlPackage::from_bytes` / `MqPackage::from_bytes` keep LocOS's numbers (10 000 entries, 256 MB); the desktop
  open-package path should call `from_bytes_with_limits` with larger limits of its own (not done here: no call
  site in core passes limits today).
- `catos-server/src/workspace.rs` builds its `ExtractOptions` with the builders; `catcore/src/inline.rs` maps a
  future `SegState` by `is_accepted()`.

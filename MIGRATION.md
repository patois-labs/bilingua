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
- **XLIFF segment ids (wave 12).** Without a profile, a trans-unit with one segment now names that segment by its
  unit id, so "segment id = unit id in paragraph mode" holds for every format. This changes the ids of an SDLXLIFF
  (and of any XLIFF 1.2 with `<seg-source>`): a Trados trans-unit `id="3f1c...a1b3"` holding `mrk mid="1"` gave
  segment `1` and now gives `3f1c...a1b3`; in a Trados package, `de-DE/a.sdlxliff|1` becomes
  `de-DE/a.sdlxliff|3f1c...a1b3`. A trans-unit with several segments keeps the file's own ids (the `mid`s); a unit
  id already used as such a `mid` gets `~2`. Unit ids, the Localization OS profile and the `Doc` handle
  (`SegView::id`, CAT OS) do not change. `bilingua_xliff::segment_ids(&doc)` gives the new ids. The skeleton schema
  of `bilingua-xliff` and `bilingua-sdl` is now 2; a stored schema 1 document still merges, by the old ids
  (`apply_units_prefixed_file_ids`). Re-extract to get the new ids; do not mix ids from the two.
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

- **bilingua-richtext (new).** Contentful Rich Text, Sanity Portable Text, Jira ADF and Notion blocks as JSON files
  (`contentful`, `sanity`, `adf`, `notion`). New option `adf.units` (text, `inline` default | `leaf`): `inline` is one
  unit per paragraph with marks as tag pairs (the Jira connector's walker), `leaf` one unit per text leaf with no tags
  (LocOS's older `adf.extract`). A document remembers its mode, so merge needs no option. Behaviour without the option
  is the default (`inline`); no existing format changes. The four filters sit last in `filters()`: a file is opened
  with them by extension (`.contentful`, `.portabletext`, `.adf`, `.notion`) or format id, never by sniffing, so no file
  that opened as `json` before changes format. Differences from the LocOS walkers, on purpose:
  - Outside `profile::locos()` a translation may hold the bracket characters `⟦` `⟧` as text (Bilingua's tags
    are structural). Under the profile, as in LocOS, such a translation is `Error::Unwritable`, and so is an ADF inline
    merge that changes the order of its mentions, emoji and other inline nodes (the Jira connector's
    `structure_ok_inline`; text may still move around them).
  - A Contentful hyperlink with no content is a standalone tag kept verbatim; LocOS makes it an empty pair it can
    never write back (its rebuild refuses an empty link), so there such a field could not be translated at all.
  - A merge is per unit: an untranslated unit keeps its source (LocOS's Contentful and Sanity rebuilds are all or
    nothing), and a whitespace-only target counts as untranslated (`set_target`), where LocOS refuses it (Contentful,
    Sanity) or writes it (ADF).
  - The merged file is the original bytes with only the changed values rewritten (LocOS re-serialises the field).
  - JSON that Python's `json.loads` reads but that is not JSON or not Unicode text is `Error::Parse`: lone surrogates
    (`"\ud800"`), `NaN`, `Infinity`; so is nesting deeper than 256 levels.

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
  `merge(doc, profile="locos")`). The profile means UTF-8 out except ini, strings, dxf, txt and properties (kept
  in their own encoding, as LocOS does; po, srt and vtt keep theirs in their own LocOS writers) and the target language stamped even when nothing is translated. TTX under the profile
  skips `<Tu>`s with a finished translation, as LocOS does.
- Under the profile, units carry `context.extra["locos.p_index"]` where the format knows LocOS's number (odf, Qt ts,
  wxl, icml, idml, resx, json) and `["locos.style"]` where it knows LocOS's style (md, dtd); unset means "fall back to
  position".
- Track D group A (txt, md, srt, vtt, po, json, strings, properties, ftl, dtd, resx, wxl, xaml, json5, arb,
  resjson): under the profile these give LocOS's units, ids, styles and builds. txt, md, srt/vtt and po are ports
  of the LocOS filters (one unit per line for txt and md, LocOS's cue tokens for subtitles, LocOS's PO build with
  CLDR plural slots and a Language header, Markdown's front matter, emphasis and autolink repairs); the md, srt,
  vtt and po profile documents carry the decoded file as a `Skeleton::Binary` (srt, vtt and po after a line with
  the file's encoding tag, so a build writes the BOM, line breaks and code page back as LocOS does; a txt profile
  document keeps a text skeleton). In the key/value formats the placeholder codes are plain text again (ARB keeps
  its ICU arguments as codes, FTL finds its placeables in the translation again); JSON walks like `json.loads`
  (the last of a duplicate key at the first one's place, every string with a letter or digit, `NaN`/`Infinity`
  read). `locos.style` is set for md images (`a11y:md-image`) and dtd (`dtd:<name>`), and `locos.p_index` for resx
  and json. `profile::locos_set_target` writes a translation of white space only as that white space (as the
  adapter's JSON target does). The goldens are in `bilingua/tests/locos/` (made by `oracle/locos_golden.py` with
  the LocOS venv) and checked by `bilingua/tests/locos_golden.rs`; `oracle/locos_fuzz.py` writes a random corpus
  with LocOS's goldens that the same test reads from `BILI_LOCOS_ROOT`. JSON resource dialects and i18next plural fill are in (see "JSON dialects" below). Known differences on purpose: a NUL in a
  translation is refused (strings, `Error::Unwritable`) or dropped (wxl, resx, xaml), where LocOS writes it into
  the file; a Markdown line with thousands of source URLs skips the autolink repair (LocOS's takes minutes).
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
- Track D group B, xlsx: under the profile this is a port of LocOS's `xlsx_filter.py` (cell mode, walk 2) and
  `xlsx_package.py`. Units are openpyxl's: every worksheet in workbook order (hidden and very hidden ones too,
  chartsheets left out), cells by row then column (a later duplicate address wins), a cell whose value is a string
  that is not a formula and has a letter or digit; shared and inline rich strings are their plain text (runs
  joined, phonetic runs left out, `x005F_` removed from shared strings as openpyxl does). Then the text-box and
  shape paragraphs of each worksheet's drawing (`locos.style` `text box`, `mc:Fallback` copies skipped), numbered
  after the last cell. Sheet, workbook and cell protection and hidden rows and columns do not change the walk, and
  the Bilingua options (`skip_hidden`, comments, sheet names) do not apply. `locos.p_index` is the unit's position.
  The export patches the original package as LocOS does: a translated shared-string cell keeps its `si` (rewritten
  in place when every cell using it gets the same text, else a new `si` appended and the cell repointed, and
  `uniqueCount` grown), any other translated cell becomes an inline string, a rich string keeps its run format
  only when all its text runs have the same `rPr`; the target language goes into `docProps/core.xml` (made, with
  its relationship and content type, when the package has none) and onto the runs of each translated text-box
  paragraph; a right-to-left target sets `rightToLeft="1"` on every worksheet view (a `sheetViews` added where a
  sheet has none). Untouched parts keep their bytes; a Strict Open XML workbook comes back Strict. Copied from
  LocOS on purpose: an error literal (`t="e"`, such as `#N/A`) and a `t="str"` cell without a formula are units,
  and a negative shared-string index counts from the end of the table. A text-box paragraph's tokens are LocOS's
  PowerPoint tokens: `profile::tokens::to_locos_text_for("xlsx", ..)` gives `r` run pairs and numbers each token by
  its tag id, so a break or field after two runs is `x3`, as LocOS writes it (the adapter's `_PAIR_LETTER` needs
  `xlsx` too). XML parts are read in UTF-8 or UTF-16 (with its BOM), as lxml reads them, and a rewritten part is
  written as UTF-8.
  Wave 12: column mode and comment units are ported (below).
- Wave 12, xlsx column mode and comment units (`xlsx.columns`, `xlsx.target_locales`, `xlsx.translate_comments`):
  new `XlsxOptions::columns` (key `xlsx.columns`, default none) and `XlsxOptions::target_locales` (key
  `xlsx.target_locales`, comma separated, default none). With a column mapping (any non-empty string, as in LocOS;
  the CSV filter's `source=`, `target=`, `target.<locale>=`, `key=`, `context=`, `maxlen=`, `header=`) each sheet is
  planned from its first non-empty row (`bilingua_csv::plan`, so CSV and XLSX read a mapping the same way) and the
  source column's text cells (a letter, not a formula) are the units, in sheet and row order, numbered 0.. with
  `locos.p_index`; the note columns are `context.notes`. The merge writes the translation into the target column of
  the same row for `ExtractOptions::target_lang` (`target.<locale>` first, then `target=`), making the row and cell
  where there are none (rows and cells get explicit addresses, `dimension` grows) or over the source cell when the
  plan has no target column; a filled target is kept when the translation equals the source. A target cell that
  held a formula becomes text: dependents of a shared formula get their own formula and `calcChain.xml` is dropped
  (Excel rebuilds it). A sheet the mapping does not fit is left alone; a mapping that fits no sheet is refused.
  Column mode needs no profile: it uses this same engine either way (the document carries the original package),
  the profile only decides the merge options (`merge_with_report` uses the plain ones for a document read without
  it). Text boxes and comments are not units in column mode, as in LocOS. `xlsx.table` (LocOS's stored plan) is still
  accepted and ignored: the plan is made again from `xlsx.columns` and kept on the units. Header detection, notes
  and the filled-target check read cells as openpyxl does: numbers as Python prints them, dates, times and durations
  from the number format and the workbook's epoch, ISO dates, shared-formula dependents moved from their master (a
  port of openpyxl's formula tokenizer and translator), merged cells empty; a workbook openpyxl cannot load (an
  unknown style index, a bad number, a shared formula that cannot be moved) is refused, as in LocOS. Comment units: under the profile `xlsx.translate_comments`
  now adds LocOS's comment units after the text boxes, numbered `1000000 + place in the comment walk` (threaded
  comments of a sheet, then its notes; style `comment`, text trimmed, a classic note without Excel's bold `Author:`
  run, the `[Threaded comment]` placeholder note skipped). The merge patches the comment parts in place (a threaded
  comment keeps its outer white space, a note's source text is replaced across its runs). Without the profile the
  option keeps its older meaning (units `comment<sheet>-<cell>` in the generic path). Goldens: LocOS output for
  both modes is in `bilingua-xlsx/tests/data/modes/`, made by `oracle/locos_xlsx_modes.py` (the generic golden
  harness has no way to pass `columns=` or `comments=`, so it is not changed).
- Track D group B, PPTX (and pptm, potx, potm, ppsx, ppsm): under the profile this is a port of LocOS's
  `pptx_filter.py` (`bilingua_pptx::locos`). The walk is LocOS's: per slide in `p:sldIdLst` order the slide's
  paragraphs (outside `mc:Fallback` and merged table cells), its SmartArt data and charts in reference order, its
  first notes slide, then the slide's alt text (`descr`, `title`); slide layouts and masters are not units.
  `locos.p_index` is the paragraph's slot in that walk (empty paragraphs counted, so there are gaps; alt text after
  the last paragraph) and `locos.style` LocOS's style (`slide 1`, `slide 1 SmartArt`, `slide 1 chart`,
  `slide 1 notes`, `slide 2 hidden`, `a11y:img-alt`, `a11y:img-title`). A paragraph with one run is plain text;
  otherwise every run is a pair whose tag id is LocOS's run number and every `a:br` / `a:fld` a standalone code
  numbered after the runs. LocOS does not number those tokens in reading order, so the adapter must turn them into
  text with `profile::tokens::to_locos_text_for("pptx", ..)`, which for docx, pptx and xlsx is
  `to_locos_text_numbered` (tag ids as token numbers; it falls back to `to_locos_text_as` when the ids are not
  distinct positive numbers), to get
  LocOS's `⟦r1⟧a⟦/r1⟧⟦x3⟧⟦r2⟧b⟦/r2⟧`. Writing is
  LocOS's `_rebuild` (its token rules on the target text, run properties copied, a dropped break or field put back
  at the end, new lines as `a:br`, `lang` on every run of a written paragraph and `rtl="1"` for a right-to-left
  target, the SmartArt drawing cache mirrored), re-serialising only the parts that changed; Strict Open XML goes
  through LocOS's namespace swap. The profile's documents carry the original as a `Skeleton::Binary`. Not ported:
  slide comments (LocOS's `comments=True`), LocOS's markup budget, XML parts in a legacy code page.
- Track D group B, DOCX (docx, docm, dotx, dotm): under the profile a port of LocOS's `docx_filter.py`, walk stamp 6
  (`bilingua_docx::locos`). Units are every paragraph with text in LocOS's numbering (the body in reading order
  with tables, content controls and text boxes, then headers, footers, footnotes, endnotes and comments), then the
  alt text and title of each drawing (`a11y:img-alt`, `a11y:img-title`); `locos.p_index` is LocOS's `p_index`, the
  style is the paragraph style's name ("Normal", "Heading 1"). A paragraph with more than one content run gives one
  `r` pair per run, plain runs included; object-mode paragraphs (pictures, fields, note and comment references,
  breaks, tracked changes, wrappers) give one `x` code per object group, numbered after the runs as LocOS numbers
  them; `profile::tokens::to_locos_text_for("docx", ..)` numbers docx tokens by tag id (LocOS's numbers) through the
  same `to_locos_text_numbered` path as pptx and xlsx, so `⟦x3⟧` after two runs stays `x3`. A Storyline "Export to Word" file
  gives its Translation column only; `docx.translate_comments=false` leaves the comments out, as LocOS does.
  Merging rebuilds each translated paragraph as LocOS does and, for a target language, declares it (docDefaults
  and the styles that had the old default get the target, untranslated runs that inherited the old default get it
  written on them, `w:bidi` on translated paragraphs for an RTL target, `dc:language` in core.xml, `w:bCs` /
  `w:iCs` / `w:szCs` twins on translated runs for a complex-script target); the package is saved the way
  python-docx saves it (new `[Content_Types].xml` and relationship files, unreachable parts dropped), Strict Open
  XML mapped back to Strict. A plain build that writes nothing, or only each unit's own text, is the upload. The
  XML tree the XLIFF and DOCX ports share is the internal crate `bilingua-xmltree`. Approximations: the bidi
  direction a re-created `w:bdo`/`w:dir` follows uses script blocks rather than Unicode's bidi table; tokens with
  non-ASCII digits are not read as tokens; a part in UTF-16 or UTF-8 only.
- Track D group B, all zip formats under the profile (docx, pptx, xlsx, xlz, mqxlz, sdlrpx): Bilingua refuses some
  packages LocOS opens, on purpose: an entry name that climbs out of the package (`../`, `..\`) or is absolute, the
  same part name twice in a DOCX, PPTX or XLSX (LocOS's `zipfile` reads the last copy; its XLIFF packages already
  refuse it), and a DTD in a DOCX part (LocOS's DOCX reader takes one; its PPTX and XLSX readers refuse it). LocOS
  also reads LocOS-token look-alikes (`⟦x2⟧`) in source text as tokens and drops or moves them on export; the
  profile keeps them as text, so such a unit's build differs.
- HTML family, text profile and token look-alikes (11-07, LocOS issues 810, 813, 814): Bilingua now differs from
  LocOS on purpose, and LocOS is getting the same fixes. An orphan end tag (`</b>` with no `<b>`) is a point code in
  the unit text and in the build; LocOS drops it from the unit and blanks it from a translated build. A file that
  declares ISO-8859-8 (or -3, -4, -6, -10, -13, -14, -16, Windows-874) is read in that code page; LocOS reads it as
  Windows-1252. Under the profile a Markdown BOM is not part of the first line, so `# Title` is a heading and the
  unit text is `Title`; LocOS lists `﻿# Title`. ARB and resx follow the file's CRLF for what they add or
  translate; LocOS wrote bare LF. The CJK autolink repair skips a URL that the source already had glued to text.
  Token-like text in a source (`⟦x2⟧`) stays text: Bilingua's real codes skip those numbers (so a unit's tokens can
  be numbered with gaps) and an unknown token in a translation is kept; LocOS drops or moves it.
- XML, DITA and DocBook (11-06): Bilingua opens a file whose DOCTYPE has an internal subset (entity declarations,
  as in most DocBook 4 manuals); LocOS refuses it. The subset is kept byte for byte and never read: no entity
  (internal, external or parameter) is expanded or fetched, every `&name;` stays a code, and the depth and element
  limits still apply. Every other XML format (ITS rules files, XLIFF, Office parts, resx, SVG, TTML and the rest)
  still refuses entity declarations.
- Every XML format that refuses entity declarations (11-06): Bilingua also refuses a part whose DOCTYPE, comment or
  processing instruction the XML readers would end in different places, so the depth, element and entity checks
  always cover what the reader builds: a DOCTYPE that is not closed, a `<`, `>` or bracket inside a quoted string of
  the DOCTYPE, a quote or bracket inside a comment or processing instruction of the internal subset, a `<`/`>` that
  `quick-xml` pairs differently from the quotes and brackets, a comment that starts with `>` or `->`, `<?>`, and a
  `<!` declaration outside the DOCTYPE. Real files never have these (an XMB or XTB subset of `<!ELEMENT>` and
  `<!ATTLIST>` declarations still reads). LocOS reads some damaged SVGs of this kind (a stray quote in the DOCTYPE);
  Bilingua refuses them.
- The XML guard (11-06) scans every reading a reader may give the bytes: a file that starts `<\0` or `\0<` without
  a byte order mark is scanned as UTF-16 and as UTF-8, and bytes that are not UTF-8 are also scanned as Shift_JIS,
  GB18030 and Big5 (their second byte can be `[` or `]`, so a CDATA section ends elsewhere than in the bytes). A
  UTF-16 file without a byte order mark (which XML does not allow) is refused when it has more than
  `max_xml_depth` tags.
- Wave 12, item D (facade options and xmlres records): three new `format_options` keys. `xaml.view_attributes`
  (bool, default unset: the filter's own setting, which is off for `Xaml::new()`; LocOS's `XAML_VIEW_ATTRIBUTES`;
  an explicit `false` also beats `Xaml::with_view_attributes()`). `xml.its_rules` (text, default none: the content
  of an external ITS 2.0 `<its:rules>` file, applied before the rules inside the document and after any set with
  `XmlFilter::its_rules`; malformed or unsafe rules are `Error::Parse`/`Error::Unsupported`, never ignored).
  `xml.multilingual` (text, default none: a JSON object with `mode`, `source`, `lang_attr`, `source_xpath`,
  `target_xpath`, `create`, `enabled`, as `MlConfig::from_json` reads it; replaces the filter's multilingual
  policy for that extract, `{"enabled": false}` turns the layout detection off). An empty or blank value is the same
  as no option (LocOS's constructor refuses a blank rules file or config; a typed option treats blank as unset).
  `xaml.view_attributes` takes `true/false/1/0/yes/no/on/off` and refuses anything else (LocOS's environment
  variable reads anything else as off). DITA and DocBook (`bilingua-dita`) run on the `XmlFilter` engine, as LocOS's
  `DitaFilter` and `DocBookFilter` subclass `ItsXmlFilter`, so the `xml.*` keys reach them: `xml.its_rules` applies
  (after the bundled rule set and `with_its_rules`, before the document's rules; LocOS `DitaFilter(rules=...)`),
  `xml.multilingual` is checked (a bad config is refused) but not applied, since LocOS reads multilingual layouts
  for generic `.xml` only; the same holds for an `XmlFilter` built with `no_multilingual()`. The option changes
  extraction only: a merge replays the layout and rules recorded in the document, so it needs no option (LocOS
  reads its setting again at export). `bilingua_xmlres::records(bytes)` (root `<resources>` or `<properties>`, else
  `Error::Unsupported`; LocOS crashes on another root), `AndroidStrings::records` and `JavaXmlProperties::records`
  give LocOS's `records()` as `bilingua_xmlres::Record` (`key`, `value` in LocOS token text with its outer white
  space, `comment`, `plural`, `index`, `translatable`): every entry in file order, translatable or not. LocOS's
  `ResourceRecord.max_length` and `context` are always empty for these dialects and are left out.
- `profile::lang_tag` and `profile::is_rtl` are LocOS's `html_lang` and `text_direction`.
- LocOS export keywords are accepted and ignored for now: `docx.walk`, `xlsx.walk`, `pptx.walk`, `xlsx.table`,
  `csv.table`, `markdown.walk`, `markdown.md_walk`, `markdown.fm_keys`.

- **JSON dialects and i18next plural fill (bilingua-json).** Two format options; both are off outside the LocOS
  profile, so existing behaviour does not change.
  - `json.dialect` (text; it used to be accepted and ignored). At extract: `plain` (every string is a unit; the
    default without the profile), `auto` (read the dialect from the file's shape; the default under the LocOS
    profile), or `chrome`, `formatjs`, `structured`, `go`, `arb` (the file must have that shape, else
    `Error::FormatOption`). The shapes are LocOS's and strict: a file with one entry outside the dialect stays
    generic JSON. Only the message strings are units (Chrome `message`, FormatJS `defaultMessage`, Structured
    `string`, go-i18n category and `translation` strings, ARB plain keys); descriptions and comments become unit
    notes. ICU messages (ARB, FormatJS, Structured) give one unit per plural, selectordinal and select branch and
    one for the text around it; the argument and `#` are standalone tags without a code (LocOS's `⟦xN⟧`), simple
    arguments are free `placeholder` tags (LocOS keeps them as text). Chrome `$name$` / `$1` and go-i18n `{{ ... }}`
    are standalone tags. `UnitContext::extra["json.dialect"]` names the dialect (a merge reads it from there and
    only checks the option), `context.key` is the JSON pointer of the message. Merge: a translation that loses a
    structural token or breaks the ICU braces keeps the source message; for a target language plural branches,
    go-i18n plural objects and suffix-style ARB plurals (`itemCountOne`) follow the target's CLDR categories and an
    ARB `@@locale` names the target (added first when absent). Under the profile a `⟦xN⟧` left in a translation
    keeps the source (LocOS `_restore`). A document without units is read again by shape at merge under the profile.
  - `json.i18next_plurals` (bool; default off, on under the LocOS profile). At merge for a target language, an
    i18next-style group of generic JSON (`item_one`, `item_other`, ordinals `place_ordinal_one`) gets every CLDR
    category the target uses that it lacks, holding the `_other` value, before `_other`. Without the profile the
    keys are spliced into the text and the layout stays; under the profile the file is written as LocOS writes it
    (parsed and dumped with two-space indentation, Python's number spelling, the original line endings), and only
    when a key was added. The categories come from the shared CLDR table (`bilingua_model::cldr`); babel's old and three-letter language codes (`deu`, `iw`, `und`) resolve as LocOS resolves them.
  - Known difference: a message with a lone surrogate escape is left alone (no unit), since a Rust string cannot
    hold it.

## Follow-ups (item G, 11-06)

- **Detection under the LocOS profile.** `bilingua::detect_with(name, bytes, profile)` is new (`detect(name, bytes)`
  is `detect_with(.., None)`); `bilingua::extract` uses it with `opts.profile`. Under `profile::LOCOS` a `.json`
  file with a string `@@locale` that has another resource dialect's shape (a go-i18n file with plural objects)
  opens in the JSON filter, as LocOS checks go-i18n before ARB; in that dialect the `@@locale` string is a message.
  Without the profile nothing changes: every such file still opens as ARB. `bilingua_json::dialect_of(bytes)` is new.
- **ARB, a message of only a token.** Under the profile a message (or plural branch) whose text, after removing
  literal `⟦x<digits>⟧` tokens, has no letter or digit has no unit (LocOS `_translatable`). Without the profile such
  a message is plain text and still a unit. A merge plans the messages as the extract did, so a document extracted
  under the profile merges without it and the other way round. Known difference: LocOS also deletes such a literal
  token from the file when it rebuilds the message (`"a": "⟦x1⟧"` becomes `"a": ""`); Bilingua keeps the text.
- **Repeated placeholders.** `profile::locos_set_target` now accepts a translation that repeats a standalone tag
  (`#` in an ICU plural, Chrome `$x$`, go `{{..}}`, `<br/>`), as LocOS does; `Document::set_target` still refuses it
  (`Error::DuplicatedTag`), and paired tags and tags the source lacks are refused by both. `Document::set_target_repeating`
  is the same call without the profile's other rules. The writers that write a repeat: all except those where LocOS
  itself keeps a tag once (ASS, TTML, TTX, TXLF/TXML, XMB/XTB, XTG, XSLT, generic XML/DITA/DocBook, Android strings,
  DOCX, IDML, ICML, MIF, ODT, VSDX, Qt Linguist, server pages, WebVTT, XLIFF packages including memoQ `.mqxlz` and
  Trados `.sdlppx`), which drop the second use as LocOS does (measured per corpus file against LocOS `build`).
  Contentful, Sanity Portable Text and Jira ADF refuse the merge of a repeat (`Error::Unwritable`), as the LocOS
  walkers refuse it and keep the stored field; Notion units are plain text runs, with no tag to repeat. The HTML part of an email now writes every use under
  the profile, as LocOS does; its default is unchanged (once). Known difference: a server-page tag that carries code
  (`<img src="<%= s %>">`) is written once, where LocOS writes the whole tag again, so page code never runs twice.
- **CLDR.** `bilingua-arb`, `bilingua-plist` and `bilingua-qtts` use `bilingua_model::cldr`; their private tables
  are gone, with no change in answers (each crate's test compares the old table over every language code of two or
  three letters, with and without region, script and white space). New in the model: `cldr::bits_babel(lang, ordinal)`
  and `cldr::babel_language(lang)` for babel's 234 alias codes (`deu`, `iw`, `und`, `root`), generated by
  `bilingua-model/tools/gen_cldr.py`. Qt's whole-number forms keep a short exception list (18 languages) in `bilingua-qtts`.

## Differences from LocOS: empty HTML Help files (wild files, 11-07)

- **HTML Help, valid file with nothing to translate.** A `.hhc` or `.hhk` with an empty `<UL>` (no `Name` or `Keyword`
  entries) and a `.hhp` with no `Title=` and no window caption open with zero units and one warning of kind
  `no_translatable_text`, like the other Bilingua formats; a merge gives the original bytes back. LocOS refuses these
  files (`EmptyDocumentError`). The LocOS adapter can map zero units to that error. Still refused: a file with no HTML
  Help structure (zero bytes, only whitespace, plain text, junk: a `.hhc` / `.hhk` needs a `<UL>` or `<OBJECT>` tag, a
  `.hhp` a `[section]` line), a sitemap whose entry names a browser reads but the walker cannot (an abrupt comment
  `<!-->`), and malformed files (binary, unreadable code page, limits). An empty file in a code page that does not
  round-trip (Shift_JIS IBM extension rows) merges back to its original bytes; asked for UTF-8 output it is refused with
  `Error::Unsupported`. Hand-written tests: `empty_but_valid_files_open_with_zero_units`, `empty_versus_malformed` and
  `hostile_inputs` in `bilingua-htmlhelp/tests/locos_suite.rs`.

## Differences from LocOS: markup formats (wild files, 11-06)

Found by running real files through the pseudo round trip. In each case LocOS's output is broken or loses text, so
Bilingua departs from it in every profile; each golden test skips (or relaxes) exactly the cases named here, through a
commented list in the test.

- **AsciiDoc, two-line titles.** A title written as `Title` over `=====` is a title only while the underline is
  within one character of the title. When a translation changes the length, Bilingua redraws the underline; LocOS
  keeps the old one, so the title becomes a paragraph and the next block runs into it. Golden list `SETEXT`
  (56 cases: units, styles, maps and the echo build still compared).
- **AsciiDoc, glued macros.** `link:`, `xref:`, `mailto:`, `image:`, `footnote:`, `kbd:` and `btn:` are macros even
  right after a letter (`wordfootnote:[A note.]`), as Asciidoctor reads them; LocOS offered the macro syntax as text.
  A macro still needs its `[..]`, so `footnote:` in prose and URLs in text do not change. Golden list `GLUED_MACRO`.
- **LaTeX, text after a control word.** When a translation that starts with a letter follows a control word of the
  skeleton, Bilingua writes `\chapter{}Text`; LocOS writes `\chapterText`, another (undefined) control word. A text
  after a control space at a line end gets a blank for the same reason. Golden list `GLUED_WORD` (49 cases, each
  build compared with the `{}` taken out). An unchanged merge stays byte-identical.
- **LaTeX, `\begin{document}` in an example.** The document starts at the first `\begin{document}` that is not
  inside another environment or a verbatim body (a manual's examples, a chapter file with no preamble). An
  environment that is never closed does not count, so a broken preamble still reads as LocOS reads it.
- **MediaWiki, file links.** The localized File/Image/Media namespaces of the larger wikis (`Datei:`, `Файл:`,
  `ファイル:`) are file links, and their localized options (`mini`, `мини`, `250بك`) are not text. Local option words
  are matched as written (case sensitive, as in MediaWiki), so a one-word caption such as `Marco` stays a caption.
  The caption is the last part that is not an option (`[[File:a.png|thumb|Caption|upright]]`); LocOS looks only at
  the last part and loses a caption that comes before options. Golden list `CAPTION_BEFORE_OPTIONS`.
- **reStructuredText, blocks under a list item.** After the first blank line, a list item's body is dedented to the
  column of the item's text, so a literal block under an item stays literal (LocOS dedented it to column 0 and its
  lines leaked as text). Lines that directly continue the item's first paragraph are read as LocOS reads them. Golden list
  `LITERAL_UNDER_ITEM`.
- **reStructuredText, a literal that ends in a backtick.** A run of exactly three backticks ends the literal
  (``` ``:role:`x``` ``` is one literal, as docutils reads it); LocOS ends it at the first pair and offers the last
  backtick as text. Any other run still closes at its first pair. Golden list `LITERAL_BACKTICK`.
- **Server pages, code between attributes (11-07).** `<div <%= attrs %> class="a">`, `<input checked <%= x %> />`,
  `<option<?php if ($s): ?> selected="selected"<?php endif ?>>` and JSP code around a bare attribute are opened; LocOS
  refuses them ("code is used as a tag or attribute name"). The code is an opaque code of the tag, attribute values
  keep their normal handling, and the merge writes the code back byte for byte. Still refused, as in LocOS: code that
  is the tag name or builds an attribute name (`<<?= $t ?>>`, `<div<?= $a ?> ..>`, `data-<?= $k ?>="v"`), and a tag
  nested in a quoted attribute value that would make the tokenizer read junk (`href="..<x:url value="${v}"/>.."`).
  JSP custom tags in a tag: inside a quoted value (`title="<fmt:message key='x'/>"`) the value is not offered (LocOS
  offers the tag as text); between attributes (`<input <c:if ..>checked</c:if>>`) the page is refused (LocOS and 0.9.0
  opened it and offered the rest of the outer tag as text).
  Golden test `matches_locos_on_random_pages_and_fixtures` lets exactly this refusal differ (the opened page must
  merge back byte for byte); `refusals` and `html_with_server_code_is_protected` in `tests/locos_suite.rs` say so in
  comments.

## Differences from LocOS: xcstrings (11-06)

- **`stringSet` (App Shortcuts phrases).** LocOS does not read it, so such a catalog had no units. Bilingua gives
  one unit per source string. The target set always has the source's count and order. A phrase without a new
  translation keeps the target's old string at that position, else the source string, and the set is then
  `needs_review`; a phrase with no letter or digit is copied. Extra old target phrases beyond the source count are
  dropped once any phrase of the set is translated.
- **`${name}` parameters** (App Intents and App Shortcuts, `${applicationName}`) are free inline codes in every value;
  LocOS offered them as text. A value that is only parameters is not a unit.

## CAT OS core (note for the lead)

- `catcore` re-exported `bilingua_sdl::ZipLimits`; it now re-exports `bilingua_model::Limits as PackageLimits`
  (`catcore/src/lib.rs`), and `catcore/tests/packages.rs` builds it with `PackageLimits::new().with_max_zip_*`.
  `SdlPackage::from_bytes` / `MqPackage::from_bytes` keep LocOS's numbers (10 000 entries, 256 MB); the desktop
  open-package path should call `from_bytes_with_limits` with larger limits of its own (not done here: no call
  site in core passes limits today).
- `catos-server/src/workspace.rs` builds its `ExtractOptions` with the builders; `catcore/src/inline.rs` maps a
  future `SegState` by `is_accepted()`.

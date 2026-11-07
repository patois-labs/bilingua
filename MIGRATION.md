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
  `merge(doc, profile="locos")`). The profile means UTF-8 out except ini, strings, dxf, txt, properties, xaml and resjson (kept
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
  unit text is `Title`; LocOS lists the mark and `# Title` as text. ARB, resx and XAML follow the file's CRLF for what they
  add or translate; LocOS wrote bare LF. The CJK autolink repair skips a URL that the source already had glued to text.
  Token-like text in a source (`⟦x2⟧`) stays text: Bilingua's real codes skip those numbers (so a unit's tokens can
  be numbered with gaps) and an unknown token in a translation is kept; LocOS drops or moves it.
  DOCX, PPTX and XLSX text boxes under the profile do the same when they write: plain text that looks like a token
  (`⟦x2⟧`, a token for a code the unit does not have) stays text in the file; only the unit's real codes are tokens.
  The XLIFF family (xliff, sdlxliff, mqxliff, mxliff), XLIFF packages (xlz, mqxlz), Trados packages (sdlppx,
  sdlrpx), MDX and SubRip / WebVTT do the same: look-alike text in a source or a translation is written as text, even when it names a
  real code of the unit (`⟦g2⟧` typed beside the code `g2`). LocOS reads it as that code, so it moves or doubles
  the code and loses the text. Where a translation arrives as LocOS text (the adapter), a typed token of a real code
  is that code by definition and nothing changes. The public `bilingua_xliff::locos::target_text`, `written_texts`
  and `build_cat` still read plain LocOS text, every token a code, as in 0.9.0. MDX also skips the emphasis repair
  of a line when the repair would change a token (LocOS's CJK repair can move a mark into one, `⟦*x1⟧`, and then
  writes the broken token as text, losing its code).
- XML, DITA and DocBook (11-06): Bilingua opens a file whose DOCTYPE has an internal subset (entity declarations,
  as in most DocBook 4 manuals); LocOS refuses it. The subset is kept byte for byte and never read: no entity
  (internal, external or parameter) is expanded or fetched, every `&name;` stays a code, and the depth and element
  limits still apply. Every other XML format (ITS rules files, XLIFF, Office parts, resx, TTML and the rest)
  still refuses entity declarations. SVG (11-07) accepts plain internal ones only (`<!ENTITY name "literal">`, no `&` in
  the literal, at most 1024 bytes, at most 64, as Illustrator and Inkscape write for namespace URIs; external, parameter and nested
  entities are still refused); a text node that uses a declared entity is not offered as a unit.
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

## Differences from LocOS: PDF text cleaning (#808)

Unit text from `bilingua-pdf` is cleaned after PDFium's page text: every U+FFFE is dropped (the line-end hyphen
marker, so the hyphenated word is joined, and the marker for a glyph mapped to U+0000), and C0 controls other than
tab and line breaks are removed. The OCR threshold counts only printable characters. LocOS gets the same change next
(needs a new extraction version there); until then units differ from LocOS on those pages. A compound hyphen that
PDFium takes for a line-end break (`well-` at a line end, `known` on the next) is joined too (`wellknown`), as
PDFium's own heuristic decides. There is no separate `locos` profile for PDF: both modes behave the same.
`page_texts` text is still byte for byte PDFium's.

## Differences from LocOS: subtitles and Salesforce (issues 798, 803, 804, 11-07)

LocOS is getting the same fixes, so these differences are temporary.

- **STL, code table switch (now the same in LocOS).** When a translation needs another EBU table, comment subtitles and text-less subtitles
  are re-encoded with the rest of the file, and the switch is refused when one cannot move (LocOS leaves their bytes
  in the old table). A byte kept raw (a control code, a byte the old table has no character for) moves only when the
  new table reads it the same way: 0xC0 (none in table 00, a letter in table 01) makes the switch refuse with a
  message naming the subtitles (LocOS copies it raw, and it reads as a letter of the new table); 0x7F (none in any
  table) moves. Tests: `a_code_table_switch_*` in `bilingua-stl/tests/behaviour.rs`. The other parts of issue 798
  (letters with a floating mark, quadratic paths) were already fixed in Bilingua. LocOS build/issues-1107 now makes the
  same refusals and builds, so the STL golden test has no exemption left.
- **STL, base and mark pairs.** The locos profile writes a base letter and a floating mark with no single code as the
  two-byte pair, as LocOS now does (it refused them before). A pair takes one cell when rows are measured (the mark
  byte takes none), in both profiles. Only the wider-row allowance (a row may be as wide as the widest source row)
  stays native. Test: `a_base_and_mark_pair_takes_one_cell_in_both_profiles`.
- **STF, escapes a translator types.** The escapes for backslash, line feed, carriage return and tab in a loaded TRANSLATION cell
  that the source row does not have are decoded to their character (they stayed raw text and were escaped a second time on merge). Other
  placeholders the source lacks still stay text. Test: `an_escape_a_translator_typed_is_decoded_and_escaped_once_on_merge`.
- **HTML Help, `.hhp` over a limit.** `hhp_code_page` returns `None` for a project over the size, `<` count or line
  limit, as LocOS; only a code page the decode cannot read still falls back to the ASCII `Language=` line.
- **SubViewer/MicroDVD, two kept differences.** LocOS now refuses a translation line that reads as a timecode or a MicroDVD
  cue; Bilingua already did. LocOS still writes text that reads as a MicroDVD control code (`{y:i}`), where Bilingua
  refuses (3 of the golden builds). A token that names no code of the unit (`⟦x99⟧`) stays text in SBV and SubViewer
  in both (LocOS issue 814), but the LocOS MicroDVD build still drops it; the golden test names those builds (LocOS
  to change, as 814).
- **SCC, unknown inline code.** A translation with a code the caption does not have is refused, where LocOS drops it
  silently. Literal `⟦xN⟧` text was already refused (the characters are not CEA-608). The channel-2 loss and
  the quadratic character scan were already fixed in Bilingua.
- **STF, backslash.** A literal backslash in a translation is written as `\\` so Salesforce reads it back as a
  backslash. Build bytes change for such translations. Test: `literal_backslash_in_a_translation_is_escaped`.

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
- **AsciiDoc, translated table cells (security).** In a nested `!===` table a translated `|` is escaped (`\|`) as well
  as `!`, so it cannot split the outer `a|` cell. A cell that starts its line gets the `{empty}` guard when the
  translation opens with block syntax or a directive (`ifdef::`, `include::`, `//`), as paragraphs do. LocOS does
  neither (issue 788); the fix is going into LocOS too.
- **rc, a short escape before a translated digit.** `\x4` followed by `1`, or `\1` followed by `2`, would be read by
  rc.exe as `\x41` and `\12`. Bilingua pads the escape with leading zeros to the most digits rc.exe reads (2 hex or
  3 octal in a narrow string, 4 hex or 7 octal in an `L"..."` one): `\x041`, `\0012`. LocOS writes the merged escape
  (issue 791). Bilingua also reads octal escapes in wide strings up to 7 digits, as rc.exe does; LocOS stops at 3,
  so a wide string with 4 or more octal digits in a row extracts differently.
- **Fluent, multi-line indent.** A value that starts on its own line writes its continuation lines at that line's
  indent. LocOS used the first deeper-indented line, so the value gained two spaces per merge (issue 811, item 1).
  The golden file was regenerated with that one LocOS function patched in `tests/tools/gen_golden.py` (every golden
  case says so in its `locos_patch` field); drop the patch when LocOS has the fix.
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

## Differences from LocOS: structure fuzz (issues 788 and 790, 11-07)

A round-trip structure fuzz on the LocOS side found four ways a translation changes a file's structure; LocOS
gets the same fixes (its gate commits for #790 and #788), so the first four items are not differences once both
have shipped. Bilingua's own fuzz (`bilingua/tests/structure_fuzz.rs`) then found the rest, which go beyond LocOS.
Each golden test relaxes exactly the cases named here, through a commented rule or list in the test.

- **MDX, code spans in a translation (security).** The `{`, `}` and `<` a translation adds are escaped outside code
  spans, and the spans are now found the CommonMark way: a whole backtick run opens one (never a suffix of a
  longer run, `` w ``<z` ``), a backslash-escaped backtick is a literal (`` \`{x}` ``), and in a table row no span
  crosses a cell `|`. LocOS's old regex left the `{x}` or `<z` live (a JavaScript expression or JSX).
- **MDX and Markdown (LocOS profile), a translated code fence.** A translated line start that opens a fence
  (```` ``` ```` or `~~~`) gets a backslash, as the other block markers do (#704): a fence swallowed the rest of
  the file.
- **MDX, a translated ESM line.** A translated line that starts with `import ` or `export ` writes its first letter
  as a character reference (`&#105;mport`), which prints the same; it would have been compiled as JavaScript.
- **AsciiDoc, description list delimiters.** `x:: w` (also `:::`, `::::`, `;;`) in a translation is written with
  `{two-colons}` / `{two-semicolons}`, so a paragraph, admonition or term does not become a list item. Golden: the
  `block` builds (`term:: `) compare with the references read back as `::`.
- **Markdown (native), block starts, cells and links.** A translated unit that would open a block its source did
  not (a fence, `# `, `- `, `1. `, `> `, a rule or setext underline) at a block start gets a backslash before the
  marker; a translated cell escapes `|`; a `[` or `]` inside a link's text is escaped; an ATX heading that ends in
  ` #` escapes it (the reader drops a closing sequence). Unit text stays Markdown source otherwise.
- **MDX and Markdown (LocOS profile), heading text.** The block-start escape also runs on heading text: LocOS's
  prefix takes a translated `# `, `- ` or `1. ` after the heading's `#` (`## 1. Zwei` reads back as `Zwei`).
  Escaped, it prints the same. `locos_suite::block_start` expects `## 1\. Überschrift zwei`. A translated line
  start with four or more columns of indent (an indented code block) loses the blanks, in native Markdown too.
- **MDX, backticks and backslashes around codes.** In a changed line with codes, a backtick run that closes no span
  of its own text run is escaped: two backticks on either side of `{count}` paired into a code span and the
  expression was read back as text. A text run that ends in an odd run of backslashes before a code gets one more
  backslash, so the code's first character (`<Badge>`) is not escaped into text. Golden: builds that differ only by
  `` \` `` are counted (fewer than 40 allowed).
- **AsciiDoc, marks next to codes, terms and titles.** A translated mark character next to a code's mark of the
  same character (`*p*` + `*`), and a translated `[...]` right before a mark (it would be the mark's attribute
  list), is written as a character reference (`&#42;`, `&#93;`). A term that ends in `:` or `;` gets `{empty}`
  before its delimiter (`w:` + `::` read as a `:::` delimiter). A section title that ends in its own marker
  (`== w ==`) gets `{empty}` after it. Golden list `MARK_REF` (2 `zh` builds).

## Differences from LocOS: xcstrings (11-06)

- **`stringSet` (App Shortcuts phrases).** LocOS does not read it, so such a catalog had no units. Bilingua gives
  one unit per source string. The target set always has the source's count and order. A phrase without a new
  translation keeps the target's old string at that position, else the source string, and the set is then
  `needs_review`; a phrase with no letter or digit is copied. Extra old target phrases beyond the source count are
  dropped once any phrase of the set is translated.
- **`${name}` parameters** (App Intents and App Shortcuts, `${applicationName}`) are free inline codes in every value;
  LocOS offered them as text. A value that is only parameters is not a unit.

## Differences from LocOS: issue fixes (cadzip group, 11-07)

Bilingua fixes these first; LocOS gets the same changes next and its goldens are regenerated. The golden tests in
`bilingua-dxf` and `bilingua-htmlhelp` list the cases that differ on purpose (`FIXED_ON_PURPOSE`).

- **dxf, issue 805**: a single `%` at the end of a translated text piece before a `%%d`, `%%c`, `%%p` or `%%nnn` code is
  written `%%%`, so the code survives (LocOS wrote `%%%d`, which reads back as a literal `%` and a stray `d`). The
  line budget counts a CRLF line end once (LocOS counted it twice and refused CRLF files at half the group codes). A
  DOS864 drawing reads byte 0x25 as `%` (not U+066A), so `%%d` stays a code, and writes `%` back as byte 0x25 (the
  code page has no other byte for it; LocOS's codec has none for `%` at all and wrote `\U+0025`). A translator-typed
  Arabic percent sign is still written as byte 0x25 in that code page.
- **dxf, issue 811 item 3**: a code that ends where the value ended (`\` alone, an unterminated `\A1`) is closed
  when anything follows it, white space included (LocOS closed it only before other characters).
- **htmlhelp, issue 796**: a declared `gb2312`, `shift_jis`, `euc-kr` or `ks_c_5601-1987`, or `big5` label is read and
  written as GBK, Windows 932, Windows 949 or Windows 950, the way browsers and HTML Help Workshop read them. A
  localized CJK export now reads back (LocOS fell back to Windows-1252 for any character outside the strict subset).
  The EUC-KR make-up sequence is no longer special (it reads as the separate jamo, as browsers show it). A file
  keeps its declared label: these are the labels HTML Help Workshop itself writes for code pages 936, 932, 949 and
  950, so the superset bytes under them are what every reader expects. A `euc-kr` label is no longer rewritten to
  `ks_c_5601-1987` in a localized Korean export.
- **zip batch, issue 812 item 2**: a member of a ZIP batch (`bilingua::batch`) whose path has a component that is a
  Windows device name (`CON`, `PRN`, `AUX`, `NUL`, `COM1` to `COM9`, `LPT1` to `LPT9`, the superscript `COM¹` to
  `LPT³`, any case, any extension) refuses the whole archive, as the other unsafe paths do. The batch hands its
  members back as files; the package formats (DOCX, EPUB, ...) never write a part to disk, so a part named
  `aux.xhtml` there still opens. A colon was refused already.

## Differences from LocOS: XML fixes made in Bilingua first (issues 794, 809, 813, 792)

Both modes (native and the `locos` profile) carry these fixes; LocOS gets the same change next, so the goldens in
`bilingua-dita/tests/locos_golden.rs` list them as known differences.

- 794: a comment or processing instruction inside an XLIFF `<source>` is not text and takes no token number
  (LocOS `cat_xliff.py` makes an empty `g` pair out of the comment text).
- 809.2: nested `xml:lang` (DocBook 4.x `lang`) equal to the source language is restamped on a target build when the
  element (not an inline phrase) holds at least one unit whose target differs from its source; an untranslated unit
  or a target equal to the source does not count. This is the rule LocOS's fix copies. The source language is the document's `source_lang`, else the first language the
  file declares. Same language: the primary subtag matches, and the script and region too when both tags have one
  (`en` matches `en-US` either way, `en-GB` does not match `en-US`); case and `_` against `-` are ignored.
- 809.3: no attribute unit under an explicit `translate="no"` (own attribute, ancestor, ITS rule) or inside an element
  with `conref` / `conkeyref`. Elements the dialect lists by name (DITA `codeph`) still give their attributes, unless
  `translate="no"` is set on them too. The conref element's own attributes stay translatable: by DITA 1.3 part 3,
  2.4.2.3 "Processing attributes when resolving conrefs", the referencing element's attributes win over the
  referenced element's, except those set to `-dita-use-conref-target`. So a `navtitle` (or image `alt`) attribute on a
  conref element is a unit; its content (a `topicmeta` navtitle) and the attributes of its descendants are replaced
  and are not; a value `-dita-use-conref-target` is never a unit.
- 813 item 6, wxl: a changed element text value gets the file's line break. An unchanged value keeps its bytes.
  Attribute values keep `&#10;`.
- 792: xmltree, xml, xmlres and wxl scan thousands of attributes or namespace declarations in one tag in linear time.
  xmltree refuses more than 1,024 declarations in scope at one element, and a file whose per-element scope copies
  would hold more than 4,194,304 entries in all (a memory denial of service); real files hold far fewer.

## Differences from LocOS: Markdown and PO (wave 1 follow-up, 11-07)

Found by running the LocOS suites with Bilingua switched on. Bilingua now gives what LocOS gives.

- **Markdown front matter, multi-line plain values.** A plain YAML value that continues on the next indented line is
  not offered (LocOS: the whole block reads it as the folded text, not the first line). Native Markdown does the same.
- **PO, CLDR-only targets.** The plural table holds every locale LocOS resolves through babel, aliases included.
  Known remaining difference: LocOS leaves a header alone for a language and region that babel does not know
  (`sr-XX`, `zh-US`); Bilingua takes the language's rule and ignores the region.
- **`source_encoding` for md, po and srt** under the profile: the Python codec name is the adapter's to derive from the
  Bilingua name (`windows-1252` is LocOS's `cp1252`).
- **Markdown read from Windows-1252 is written as UTF-8** when something is translated, as LocOS's `build` does (its
  stored-Windows-1252 test says so). LocOS returns the upload byte for byte when nothing is written; that is its
  `Filter.build` guard and `export._nothing_written`, so the adapter keeps it (Bilingua's own merge of an untouched
  document under the profile writes UTF-8 too).

## Wave 2 B-side: resource formats (W2, 11-07)

Found by the W2 parity and diff runs with the adapter (`coder-comparison/2026-11-07-locos-issues/bili-core/W2/B-SIDE.md`).

- **Error codes the adapter can branch on.** Size, budget and nesting refusals are code `limit` and byte-level decode
  refusals code `encoding` (both codes existed already) in arb, dtd, ftl, ini, jsmodule, json5, properties, resjson,
  strings, toml and yaml; they were `unsupported` or `parse`, so the adapter matched message text. The sentence is
  LocOS's where LocOS has one (properties binary file, ftl, dtd and toml decode refusals, ftl placeables, jsmodule and
  json5 budgets). `BilinguaError.detail` (Python) is the parser's own sentence without the `not a <format> file:`
  prefix a `parse` error carries. A js file that is not a static module stays `parse` (LocOS: InputLimitError).
- **Budgets follow the call's limits.** arb, jsmodule, resjson, toml and yaml took their crate defaults in
  `Filter::extract`, so a lowered `MAX_DOCUMENT_SEGMENTS` never reached them; json5 lowers its own limits to the call's
  when they are set. New pre-parse budgets: arb (`check_json_budget`), toml (`_budget`: quotes over 40 x, `= , [ {`
  over 20 x), resx, wxl, Android strings, Java XML properties and Qt ts (`check_xml_budget`: `<` over 40 x). Native
  mode too. arb's `Filter::extract` now caps at 50,000 units (LocOS) where its own `Limits` allow 1,000,000;
  `Arb.extract_with` keeps the crate's numbers.
- **Wrong-kind tokens (toml, js, #814.1).** A pair token naming a point code (`⟦g1⟧` for
  `{{ .Count }}`) is dropped, as LocOS drops it; a token naming no code stays text. Other formats keep such a token
  as text, as before (`profile::tokens::parse_locos_text_for`).
- **Encodings kept under the profile.** xaml and resjson are written in their own encoding: a UTF-16 file keeps its
  BOM and UTF-16, a declared legacy charset stays with a character reference for what it cannot hold.
- **yaml under the profile.** Values are plain text (Rails `%{name}`, `{name}`, HTML and `%d` are no longer codes) and
  the style is empty: LocOS's yaml filter has no inline map and no style. Native mode is unchanged.
- **xcloc under the profile.** Members go through the LocOS XLIFF reader and writer: trimmed unit text, `p_index`
  offset by the `<source>` elements of the earlier members (skipped sources leave gaps, so segment ids match LocOS),
  a new `<target>` without a state, the declaration line break kept, `_mark_translated` and `targetLocale`. A repeated
  token is written once, as LocOS does. The profile document carries an empty skeleton entry naming this writer; a
  native document merges as before.
- **xaml DOCTYPE wording** is LocOS's, UTF-16 files included.
- **xcstrings under the profile.** LocOS writes a changed catalog with `json.dumps` (`_dump`), which spells an empty
  object `{}` where Xcode writes an opening brace, a blank line and the closing brace. A profile merge that changes
  the file now writes empty objects and arrays the same way; Bilingua still splices everything else (an Xcode file is otherwise already in `json.dumps`
  style). Native mode keeps every byte. A catalog that LocOS's `_dump` would re-escape (`ensure_ascii`, `\/`) is
  not rewritten that way.
- No profile version bump: no LocOS document was ever stored by Bilingua in production.

## Wave 1 switch blockers: Markdown, JSON and PO (W1, 11-07)

Found by the LocOS suites and the differential test with Bilingua switched on. Bilingua now does what LocOS does.

- **`markdown.fm_keys` is applied** (it was accepted and ignored). It is LocOS's `LOC_TMS_FRONT_MATTER_KEYS`: comma
  separated, blanks around a name dropped, case-insensitive, empty = no front matter key is translated. Under the
  profile it replaces LocOS's default list; in native mode it replaces `title` and `description` (unset: as before).
  The profile skeleton tag carries the list (`;fm=a:b`), so the merge walks the file with the keys it was extracted
  with. A skeleton tag may now be up to 4096 bytes (was 200).
- **Markdown image inventory (LocOS `a11y_images`).** Under the profile each unit carries LocOS's image scan as unit
  extras: `locos.a11y_images` (how many images `a11y.anchor_for` puts on it) and `locos.a11y_empty_alt` (the file
  names of those with an empty alt, one per line). The adapter sums them into `skeleton["a11y_images"]`. Units and
  their text do not change.
- **Python charset names in a declaration** (`enc::label_encoding`). A file that declares `charset=CP932` (or `cp936`,
  `cp949`, `uhc`, `ujis`, `sjis`, `ms_kanji`, `iso8859_2`, ...) is read in that charset, as LocOS's `codecs.lookup`
  reads it: the WHATWG label first, then Python's codec names and aliases of the legacy charsets LocOS reads from a
  declaration. A PO file declaring CP932 is read as Shift_JIS whatever the source language and written back in it
  under its own label (it was read as Windows-1252 and refused, or written as UTF-8). This applies to every format
  that honours a declaration (HTML, XML, PO).
- **Flood budgets (LocOS #571 gate).** A JSON file with more than 40 x `max_units` quotes, or 20 x `max_units` commas
  and brackets, and a PO file with more than 40 x `max_units` backslashes, are refused as a `limit` error before they
  are read (they were read, found to hold no text, and refused as empty). Native mode too.
- **JSON nesting under the profile.** LocOS reads JSON as deep as `json.loads` does (Python's recursion limit, about
  990 levels); the profile now reads 400 levels (was 200; the scanner recurses, so the cap stays safe on a 2 MB thread stack) and
  refuses past that. Native mode keeps 200.
- **No profile version bump.** These change what the locos profile reads, but no LocOS document was ever stored by
  Bilingua in production, so the profile stays at its version.

## Differences from LocOS: unknown tokens in rst, ARB and JSON (#814 item 1, 11-07)

Both engines now follow one rule: a token in a translation that names no code of the unit is typed text. It is kept,
with the format's escaping (backslashes in rst, JSON string escapes in ARB and JSON), and never replaces the
translation with the source.

- **rst.** The writer cut every token-shaped piece out of a text (`Ein Wort ⟦x9⟧.` became `Ein Wort .`). It writes the
  text as it is. The text around a token is now one escaping context, not two.
- **ARB, Chrome, go-i18n, FormatJS and the other JSON dialects (LocOS profile).** A level whose translation held a
  `⟦xN⟧` its unit did not have was written as the source (LocOS `_tokens_ok`). The translation is written, token
  included. The native profile already did this.
- The adapter needs no change. A repeated or wrong-kind real code is handled as before.

## Differences from LocOS: web group (wild files round 2, 11-07)

- PHP: the block guard `if (!defined('X') || !X) { die('...'); }` (with or without braces, the second test and a
  plain string message) is skipped like `defined('X') OR die(...)`; LocOS refuses the file. Anything else in the block,
  any call, interpolation or concatenation, and any statement after it still refuses the file.
- HTML and server pages: a semicolon-less legacy reference that the unit's source wrote (`&nbsp`, `&copy`) is
  written as it is in a target; LocOS `_escape_text` writes `&amp;nbsp`. A semicolon-less name the source did not
  have is escaped as before.
- Declared charsets (HTML, XML family, PO and the rest of the formats that read one): only the declarations in the
  first 4096 characters are the file's own, so a later one is content and comes back as written; LocOS rewrites every
  one it finds. A legacy file whose only declaration is further down is read by the default path (HTML used to refuse
  it as "cannot be written back unchanged") with a
  `text_encoding` warning that names the later charset.

## CAT OS core (note for the lead)

- `catcore` re-exported `bilingua_sdl::ZipLimits`; it now re-exports `bilingua_model::Limits as PackageLimits`
  (`catcore/src/lib.rs`), and `catcore/tests/packages.rs` builds it with `PackageLimits::new().with_max_zip_*`.
  `SdlPackage::from_bytes` / `MqPackage::from_bytes` keep LocOS's numbers (10 000 entries, 256 MB); the desktop
  open-package path should call `from_bytes_with_limits` with larger limits of its own (not done here: no call
  site in core passes limits today).
- `catos-server/src/workspace.rs` builds its `ExtractOptions` with the builders; `catcore/src/inline.rs` maps a
  future `SegState` by `is_accepted()`.
- Wild round 2 (xml group): ICML now reads a file whose root is a bare `<Story>` (LocOS and the old Bilingua refused it;
  Okapi reads it). XSLT keeps a named entity reference as a protected token when the DOCTYPE has an external subset
  (the subset is still never read). XSLT and WXL write the carriage returns of a translation as `&#13;` when the source
  text had one (it can only have come from a character reference); LocOS turns them into LF.

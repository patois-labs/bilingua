# Migrating to Bilingua 1.0 (model round 2)

What changed in the public API before 1.0, for format-crate builders and for the Localization OS adapter. Every
item below breaks code written against 0.1; after 1.0 these types grow without breaking callers.

## Everyone

- **Build options and model values with constructors, not struct literals.** `ExtractOptions`, `MergeOptions`,
  `Limits`, `Document`, `Unit`, `Segment` and `LimitError` are `#[non_exhaustive]`; outside `bilingua-model` a
  struct literal (even with `..Default::default()`) no longer compiles.

  ```rust
  // before
  let o = ExtractOptions { target_lang: Some("de-DE".into()), limits: Some(Limits { max_units: 500, ..Limits::default() }), ..Default::default() };
  // after
  let o = ExtractOptions::new().with_target_lang("de-DE").with_limits(Limits::new().with_max_units(500));
  let m = MergeOptions::new().with_output_encoding(OutputEncoding::Utf8);
  let d = Document::new("txt", units, skeleton).with_options(&o);
  let s = Segment::new("u1", source).with_target(target).with_state(SegState::Translated);
  let u = Unit::from_segments("u1", segments).with_translate(false);
  ```

  Optional text fields take `"de"`, a `String` or an `Option` of either (`with_target_lang(opts.target_lang.clone())`).
  Fields stay public for reading and assigning. `MergeOptions` is no longer `Copy` (`Limits` still is).
- **Match with a wildcard arm.** `Error`, `LimitKind`, `OutputEncoding`, `Granularity`, `SegState`, `Skeleton`,
  `OptionKind`, `bilingua_xliff::Error` and `bilingua_seg::SrxError` are `#[non_exhaustive]`.
- **`format_options` are typed.** `bilingua::extract` refuses a key no filter declares, or a value of the wrong kind,
  with `Error::FormatOption { key, msg }`. `bilingua::format_option_keys()` (CLI: `bilingua options`) lists them. A
  key for another format than the file's is accepted.
- **`Skeleton::Binary`** is new (mo uses it); **`Unit::hidden`** holds parser-private values that used to be `~`
  codes in `Unit::codes`. Old JSON documents still read (the `~` codes move into `hidden`).
- **Empty translations.** An empty `target` still means untranslated (the source is written).
  `Document::set_target_empty(id)` marks a segment as translated to nothing (`Segment::empty_target`); it is written
  empty. `Segment::has_target()` / `Unit::is_translated()` are the tests to use.
- **Profiles.** `bilingua_model::locos` (the token conversion) is now `bilingua_model::profile::tokens`;
  `MergeOptions::locos()` is gone: use `profile::locos().merge_options(format)`.
- **`Error::Unwritable { unit, reason }`** is new: the translation cannot be written in this format (does not
  fit, a character has no code in the code table, it would read back as structure). `Error::Write` now only means
  a broken or foreign skeleton. rc, properties and strings use it; `Error::UnknownProfile` and
  `Error::FormatOption` are new too.
- **A declared encoding is binding.** `enc::decode_text` refuses bytes that are not valid in the encoding a file
  declares (XML declaration, meta charset, or a BOM in a declared format) with `Error::Encoding`, where it used to
  fall back to a code page. Undeclared formats still fall back, as LocOS's `read_text_upload` does; plain text and
  subtitles read a UTF-8 BOM plus invalid bytes as UTF-8 with U+FFFD (LocOS). `enc::DecodeOptions` is
  `#[non_exhaustive]`: `DecodeOptions::new(&opts)` or `DecodeOptions::with_limits(l).with_source_lang(..).declared()`.
- **bilingua-xml / bilingua-idml.** `MlConfig`, `MlInfo`, `Inspection`, `LangStamp`, `Detect` and `MlMode` are
  `#[non_exhaustive]` (`MlConfig::new().with_source("en")`, `LangStamp::new().with_update(["xml:lang"])`,
  `Detect::new().with_root_names(["recipe"])`); idml's `story::Where`, `Point` and `Part` are private.
- **`sdl` limits.** `bilingua_sdl::ZipLimits` is gone: `SdlPackage::from_bytes_with_limits` and
  `MqPackage::from_bytes_with_limits` take `bilingua_model::Limits` (LocOS's numbers by default: 10 000 entries,
  256 MB unpacked; raise them for desktop use with `Limits::new().with_max_zip_bytes(..).with_max_zip_entries(..)`).

## Format-crate builders

- **Binary files use `Skeleton::Binary`.** A crate that keeps the original bytes of a non-zip file (mo did; stl
  does on its branch) stores them as `Skeleton::binary(bytes, parser_info!())`, not as a one-entry
  `Skeleton::Zip`; bump `SKELETON_SCHEMA` and keep reading the old zip form in `merge` (see bilingua-mo's
  `original_bytes`).
- **`Error::Unwritable`** for a translation the format cannot hold (keep LocOS's message as `reason`); scc, stl and
  subtext move over when they rebase.

- **`Filter::merge_with(doc, &MergeOptions)` is the required method**; `merge(doc)` is provided. Implement
  `merge_with` only, and honour both options:
  - `output_encoding`: `Keep` writes the file's own encoding; `Utf8` writes UTF-8 literally (a UTF-8 BOM is kept,
    UTF-16 and code pages are not). Use `enc::encode_text_with`, or `enc::utf8_output` when the crate has its own
    codec.
  - stamping: ask `opts.stamp_lang(doc, translated)` (or `opts.should_stamp(doc)`), never `stamp_target_always`
    directly. It returns the target language only when it is not blank and something was translated or
    "always" is set.
- **Declare your `format_options` keys** in `Filter::format_option_keys()` as `FormatOptionKey::bool/int/text`,
  with the `<format>.` prefix; the facade refuses undeclared keys.
- **Use `Unit::hidden`** (`set_hidden`, `hidden`, `has_hidden`) for values only your writer needs, not `codes`.
- **Translated = `Segment::has_target()`**, not `!target.is_empty()`, so empty translations are written.

## The Localization OS adapter

- Extract with `profile="locos"` (Rust: `ExtractOptions::new().with_profile("locos")` or
  `profile::locos().extract_options()`); merge with `profile::locos().merge_options(&doc.format)` (Python:
  `merge(doc, profile="locos")`). The profile means UTF-8 out except ini and strings (kept in their own
  encoding, as LocOS does) and the target language stamped even when nothing is translated.
- Under the profile, units carry `context.extra["locos.p_index"]` where the format knows LocOS's number (odf, Qt ts,
  wxl, icml, idml today) and `["locos.style"]` where it knows LocOS's style (none yet); unset means "fall back to
  position".
- `Document::source_encoding` names the encoding Bilingua decoded a non-UTF-8 text file from (`utf-16le`,
  `windows-1252`, ...).
- An empty LocOS target is a translation to nothing: `profile::locos_set_target` (Python: set
  `"empty_target": true` on the segment).
- Python: `merge(document, *, profile=None, output_encoding=None, stamp_target_always=None, format_options=None)`
  is keyword-only (the positional options dict is gone); `extract` takes `profile=`; `format_options` values may be
  str, bool or int; `bilingua.profiles()` returns `{"locos": 1}`. A bad option key or profile raises
  `BilinguaError` with `code == "invalid_argument"`.
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

# bilingua-model

The document model every Bilingua parser fills and every writer reads back.

- `Filter`: the trait a format crate implements (detect, extract to a `Document`, merge back).
- `Document`: the file's translatable `Unit`s plus a `Skeleton` holding everything else byte for byte.
- `Unit`: one paragraph, cell or string, with one or more `Segment`s (sentences after `bilingua-seg`), its context
  (where it is in the file) and the original markup of its inline codes.
- `Inline`: text or a `Tag` (open, close or standalone). The JSON shape matches the CAT OS editor's own model, so one
  editor shows bilingual files and source files the same way.
- `ExtractOptions`, `Granularity` (sentence, paragraph, document) and `Error`.

No format parsing happens here; this crate is types, the safety guards and the text-encoding rules (below).

## Bilingual units, tag kinds, parser stamp (D27)

- A `Segment` may carry a target, a `state` (new, draft, translated, confirmed, reviewed, signed-off, rejected,
  locked) and an `origin` (kind tm, mt, ai, human or other, a name and a match percentage). A `Unit` may carry
  `notes` and a `lock_reason`. All are left out of the JSON when empty.
- A standalone tag has a `free` flag: true when it names its argument (`%1$s`, `{name}`) so a translation may move
  it. `locos::to_locos_text` and `from_locos_text` turn tags into the `g`, `x` and `p` text tokens Localization OS
  uses, one token per tag, and back.
- Every skeleton records the producing crate and version (`parser`). `merge` refuses a skeleton from another
  crate or an incompatible version (`Error::ParserVersion`); a skeleton with no record is accepted.
- Raw zip bytes in a skeleton are a base64 string in JSON; the old array form still reads.

Not done: the LocOS adapter itself, and `free` for XLIFF codes read from files (that belongs to the XLIFF crate).

## Limits, guards and encodings (D27 step 5)

Safety belongs to the engine, not to each app. The rules and the default numbers are Localization OS's.

**Limits** (`guard::Limits`, also `bilingua_model::Limits`): one `Copy` struct, `Default` = LocOS. A caller sets it per
call with `ExtractOptions::new().with_limits(Limits::new().with_max_units(10_000))` (`None` = defaults;
`ExtractOptions::limits()` gives the one in force; the struct is serde with `#[serde(default)]`, so a JSON or Python
dict may list only the fields it changes).

| Field | Default | LocOS name |
|---|---|---|
| `max_input_bytes` | 100 MB | `MAX_UPLOAD_BYTES` |
| `max_units` (units and segments) | 50 000 | `MAX_DOCUMENT_SEGMENTS` |
| `max_unit_chars` | 100 000 | `MAX_SEGMENT_CHARS` |
| `max_lines` (text files) | 1 000 000 | 20 x segments |
| `max_xml_depth` / `max_xml_elements` | 256 / 4 000 000 | libxml2 depth |
| `max_zip_entries` / `max_zip_bytes` / `max_zip_ratio` | 10 000 / 256 MB / 200 | `ZIP_MAX_*` |
| `zip_ratio_floor_bytes` | 1 MB | `ZIP_RATIO_MIN_BYTES` |
| `max_paragraphs` | 50 000 | `DOCX_MAX_PARAGRAPHS` |
| `xlsx_max_cells` / `_rows` / `_cell_chars` / `_total_text_chars` | 500 000 / 100 000 / 32 767 / 8 MB | `XLSX_MAX_*` |

A refusal is `Error::Limit(LimitError { kind: LimitKind, msg })`: `kind` is a stable value to branch on
(`input_bytes`, `lines`, `units`, `unit_chars`, `xml_depth`, `xml_elements`, `xml_entities`, `paragraphs`,
`spreadsheet`, `zip_entries`, `zip_bytes`, `zip_ratio`, `zip_malformed`), `msg` is short and safe to show the person
who uploaded the file. Unreadable text is `Error::Encoding(msg)`.

**What a new format crate calls** (all in `guard`):
1. `check_input(bytes, &limits)` first.
2. XML: `check_xml(bytes, &limits)` on every XML part before parsing (refuses `<!ENTITY` declarations: entity bombs
   and external entities; bounds depth and element count; looks inside UTF-16). HTML and other markup:
   `check_markup(text, &limits)`. Zip packages: `bilingua_zip::read_zip` applies the zip limits and proves every
   entry's real size and checksum against the directory; `bilingua_zip::check_xml_parts(&entries, &limits)` runs the
   XML guard over every XML part.
3. Text files: `enc::decode_text(bytes, &DecodeOptions::new(opts))` instead of `from_utf8`, and
   `enc::encode_text(&merged, &encoding_tag)` on merge (see below). It also applies `max_lines`.
4. While walking: `check_unit_count(n, &limits)`; at the end return `guard::finish(doc, warnings, &limits)` (checks
   unit, segment and unit-length limits and puts the warnings on the document).

**Encodings** (`enc`): `decode_text` tries UTF-8 (a BOM is kept), a UTF-16 BOM, then (formats that declare a charset:
`.declared()`) the declared legacy charset, a binary-file check, then the Windows code page of
`ExtractOptions::source_lang` (`codepage`: Windows-125x, 874, Shift_JIS, GBK/GB18030, Big5, EUC-KR as LocOS), with
Windows-1252 the default. The result's `encoding` string goes into `Skeleton::Text::encoding`; `encode_text` writes it
back in the same encoding, BOM and line endings (when the file used one style), and in UTF-8 when a translation does
not fit the code page. Untouched merges are byte-identical. When the decoded letters look wrong (`garble`: scripts
mixed inside words, accented-letter soup, UTF-8 read as a single-byte page) the file is still read and the
`Document` gets a `Warning { kind: "text_encoding", part, message }` in `Document::warnings` (left out of the JSON when
empty). `enc::warn_if_garbled` is there for a reader with its own legacy decode.

Not ported: LocOS's `html_guard` (a WHATWG tokenizer that re-parses the built HTML to catch attribute injection);
Bilingua's HTML writer escapes quotes, `&`, `<` and `>` in attributes as before.

## Python-exact character tests (`pytext`)

Localization OS decides "has a letter or digit" with Python's `str.isalnum` and "is whitespace" with `str.isspace`.
Rust's `char::is_alphanumeric` is wider (combining marks such as U+0903, newer Unicode letters) and `is_whitespace`
misses U+001C..U+001F. Format crates that mirror those LocOS rules call `py_isalnum`, `py_isalpha`, `py_isspace`
(whole string), `py_has_alnum`, `py_has_alpha`, `py_strip` (any character) or the `_char` forms, plus
`py_isdecimal_char` and `py_is_word_char` / `py_is_word_start_char` for what `\d` and `\w` match in a Python `re`
pattern. The tables are for Python 3.13 (Unicode 15.1) and are generated by `tools/gen_pytables.py`
(`python tools/gen_pytables.py > src/pytables.rs`); regenerate with the Python version LocOS runs.

## Merge options and format options

`MergeOptions` (one place, `bilingua_model::MergeOptions`) changes how a merge writes; the default is what `merge`
always did. Pass it through `bilingua::merge_with(&doc, &opts)` (`merge(&doc)` stays), or `Filter::merge_with`;
bilingua-py has `merge(document, options={...})`.

| Option | Default | Effect |
|---|---|---|
| `output_encoding` | `"keep"` | `"keep"`: the file's own encoding, BOM and line endings (an untouched merge is byte-identical). `"utf-8"`: always UTF-8 (a UTF-8 BOM stays, a UTF-16 BOM goes); an XML, HTML or PO charset declaration is rewritten to say UTF-8 (`enc::encode_text_with`, so every crate on B6 encoding has it; HTML, XAML and the XLIFF family too). |
| `stamp_target_always` | `false` | By default a merge with nothing translated returns the original bytes. `true`: stamp the `target_lang` and fill plurals even then (Localization OS): XLIFF `target-language`/`trgLang`, YAML root key, ARB `@@locale`, WiX `Culture`, PO header, HTML `lang`/`dir`, Fluent plural variants, xcloc target locale. Needs `Document::target_lang`. |

`profile::locos().merge_options(format)` turns both on (UTF-8 except ini and strings), for the LocOS adapter. A format that ignores an option says so in its README.

`ExtractOptions::format_options` is a `BTreeMap<String, String>` of options only one format knows, keyed
`"<format>.<name>"` (read with `format_option` / `format_flag`; unknown keys are ignored). Known keys:
`ftl.translate_terms` (`"true"`: translate Fluent terms; `bilingua_ftl::Limits::translate_terms` still works).

`LimitKind::Depth` is the refusal for nesting that is too deep in a format that is not XML (HTML, JSON, YAML, ...);
the cap is `Limits::max_xml_depth`, the nesting cap of every format.

`Document::set_targets` / `set_states` set many segments in one pass (one id index per call; `set_target` scans).

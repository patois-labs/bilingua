# bilingua-xlsx

The Excel parser for Bilingua. It opens an `.xlsx` (also `.xlsm`, `.xltx`, `.xltm`), gives you one unit per
piece of cell text, and writes the translation back into a valid workbook.

```rust
use bilingua_xlsx::{Xlsx, XlsxOptions};
use bilingua_model::*;

let f = Xlsx::default();                       // or Xlsx::with_options(XlsxOptions { .. })
let mut doc = f.extract(&bytes, &ExtractOptions { target_lang: Some("es-ES".into()), ..Default::default() })?;
doc.set_target("ss3", vec![Inline::text("Hola.")])?;
let (out, report) = bilingua_xlsx::merge_with_report(&doc)?;   // or f.merge(&doc) for just the bytes
```

For the facade (`bilingua/src/lib.rs`, `filters()`): `Box::new(bilingua_xlsx::Xlsx::default())`.

`ExtractOptions` has no room for format options, so the three Excel options live in `XlsxOptions`
(`extract_with(bytes, &opts, &xlsx_options)` or `Xlsx::with_options`). All three are off by default.

| Option | Effect |
|---|---|
| `translate_sheet_names` | Sheet names become units (`sheetname-N`, kind `sheet-name`, `max_len` 31). Formulas and defined names that mention a sheet are rewritten to the new name. |
| `skip_hidden` | Text that is only used in hidden sheets, rows or columns is not extracted. |
| `translate_comments` | Cell comments become units (`comment<sheet>-<cell>`, `threaded<sheet>-<cell>-<n>`, kind `comment`). |

## How it works

- The file is read in memory with the same zip safety limits as `bilingua-docx` (no `..`, absolute paths, drive
  letters or control characters in names, no encrypted entries or links, caps on entry count, size and expansion
  ratio). Nothing is written to disk.
- Every XML part that holds text is parsed into a lossless tree. An unchanged workbook comes back with every
  XML part **byte for byte identical** (the zip container itself may differ; entry order, compression method and
  time stamps are kept). A unit that is not changed is written from its stored original XML.
- Unit ids: `ss<N>` for shared string N (zero based, as in `sharedStrings.xml`), `is<sheet>-<cell>` for inline
  strings (`is1-B13`), `sh<sheet>-<element>` for headers and footers (`sh1-oddHeader`). Sheet numbers count
  the `<sheet>` entries of the workbook from 1.
- One segment per unit. Sentence splitting happens later in `bilingua-seg`.

## Supported

| Feature | What happens |
|---|---|
| Shared strings used by cells | One unit per string. A string used by several cells is still one unit: `context.location` is the first cell (`Sheet1!B4`) and `context.extra["cells"]` says how many cells use it. `context.kind` is `cell` |
| Inline strings (`<is>`) | One unit per cell |
| Cells and rows without `r` | The attribute is optional in the file format. A row without it follows the previous row, a cell without it follows the previous cell |
| Rich text runs | The most common formatting of a string needs no tag. Other runs become paired tags: `<b>`, `<i>`, `<u>`, `<sup>`, `<sub>`, or `<fmt>`. The raw run properties are kept in the tag's code |
| Phonetic guides (`rPh`) | Kept as they are while the string is unchanged. A translated string loses its guides (their positions refer to the old text) and `merge_with_report` returns a warning |
| Numbers, dates, formulas, formula results | Never extracted. A cell with a formula is skipped even if its cached result is text. Strings with no letters at all ("2024") are not units |
| Hidden sheets, rows, columns | Extracted with the note `hidden` and `context.extra["hidden"]` = `sheet`, `row` or `column`. A shared string is marked hidden only when every cell that uses it is hidden |
| Text length limits | A data validation of type "text length" puts the limit in `context.max_len` (the smallest limit of the cells that use the string). Header and footer units get 255 (Excel's limit) |
| Headers and footers | One unit per `oddHeader`, `oddFooter`, `evenHeader`, ... (kind `header` or `footer`). Codes such as `&L`, `&C`, `&R`, `&P`, `&N`, `&D`, `&"Arial,Bold"`, `&12` are standalone tags; `&&` is a literal ampersand and is written back as `&&` |
| Comments | Legacy notes and threaded comments are notes on the unit of the cell they belong to ("Comment (Ana): text"). With `translate_comments` they are also units. The "[Threaded comment]" placeholder that Excel writes in the legacy part is ignored |
| Sheet names | Off by default (see above). Names are cleaned for Excel: `: \ / ? * [ ]` become `-`, at most 31 characters. A name that is empty or already used keeps the source name, and the report says so |
| Table headers | A table column name must equal its header cell. When a header cell that belongs to a table is translated, the column name in the table part is changed too, and the report has a warning because formulas written as `Table1[Old name]` are not updated |
| Target language | Excel runs carry no language, so nothing is set |
| Other namespace prefixes | The prefix of the spreadsheet namespace is detected from the root element |

## Not supported (and what happens instead)

| Not supported | What happens |
|---|---|
| Chart titles and chart text | Left out on purpose; charts are not touched. A chart that points at a renamed sheet keeps the old name (use `translate_sheet_names` only for workbooks without such charts) |
| Pivot tables, slicers, external links, VBA, form controls, text boxes and shapes in drawings | Left untouched, not extracted |
| Sheet names inside formulas stored in conditional formats of the x14 extension, pivot caches, charts | Not rewritten when a sheet is renamed |
| Cached results and number formats containing text (`0 "units"`) | Not extracted |
| `_xHHHH_` escapes inside strings | Shown as they are |
| `ExtractOptions::include_locked` | Ignored; formula results are never extracted |
| Preview | No HTML preview for spreadsheets (`preview()` returns `None`) |
| Workbooks that contain U+E000 to U+E005 in the workbook, sheet or shared strings part | Refused with an "unsupported" error (used as internal markers) |
| DOCTYPE or entity declarations | The workbook, sheets and shared strings are refused; side parts are left untouched |
| Side parts that are not well-formed XML or not UTF-8 | Left untouched |
| Digital signatures, encrypted workbooks | A signature does not stay valid after any change; encrypted files are refused |

## Zip details

The skeleton is `Skeleton::Zip`: untouched entries are `ZipData::Raw`, rewritten XML parts are `ZipData::Parts`.
`bilingua-model`'s `ZipEntry` has no field for the compression method or the time stamp, so the filter adds one
extra entry named `"\0bilingua-xlsx-meta"` (a NUL in the name, impossible in a real file) as the last skeleton
entry. It is never written to the output. Do not drop it when you store or copy a `Document`.

Skeleton `Raw` strings may contain internal markers (U+E000 to U+E005). Only this crate's `merge` resolves them.
Unit codes whose names start with `_` belong to the writer; do not edit them.

## Tests

`cargo test -p bilingua-xlsx`. The fixture workbook is built in code from made-up XML (no binary files). The
LibreOffice test converts a merged workbook to PDF when `soffice` is installed in a standard location and is
skipped otherwise.

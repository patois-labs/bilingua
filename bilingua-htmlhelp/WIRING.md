# Wiring bilingua-htmlhelp (the lead replaces the throwaway wire commit at merge)

- Three format ids from one crate: `hhc` (contents), `hhk` (index), `hhp` (project). Extensions: `hhc`, `hhk`, `hhp`.
  Filter types: `bilingua_htmlhelp::HhcFilter`, `HhkFilter`, `HhpFilter` (unit structs, `Filter` impls).
- Facade `filters()`: add the three boxes **before `bilingua_ini::IniFilter`** (an `.hhp` is INI-like) and, of course,
  before HTML and text. Sniffing is strict, so the exact place does not matter beyond that:
  - `hhc`: the first 4 KB hold `text/sitemap` and no `Keyword` / `See Also` param;
  - `hhk`: the same, with a `Keyword` or `See Also` param (a plain hhk with only `Name` params is the same markup as an
    hhc, so only the extension tells them apart);
  - `hhp`: an `[OPTIONS]` line and one of `Contents file=`, `Compiled file=`, `Index file=`, `Default topic=`,
    `Language=0x`.
  UTF-16 files are sniffed too.
- Facade `Cargo.toml`: `bilingua-htmlhelp = { workspace = true }`. Workspace: member `bilingua-htmlhelp` and
  `bilingua-htmlhelp = { path = "bilingua-htmlhelp" }`. Dev-dependency `proptest = "1"` is already in the lockfile.
- `bilingua/tests/contract.rs` `crate_of`: `"hhc" | "hhk" | "hhp" => "bilingua-htmlhelp"`.
- `tools/check-portable.sh` and `.ps1`: add `htmlhelp` to `CRATES`.
- Oracle corpus: `oracle/corpus/htmlhelp/` (38 files, made by `tests/tools/gen_corpus.py`).
- Options, none in `ExtractOptions` beyond the usual:
  - `source_lang` picks the code page of an undeclared non-UTF-8 file (Japanese: Shift_JIS) when no project hint is given;
  - `HhcFilter::extract_with(bytes, opts, hhp_code_page)` / `HhkFilter::extract_with` / `HhpFilter::extract_with`
    take the ANSI code page of the project's `Language=` (`bilingua_htmlhelp::hhp_code_page(hhp_bytes)`), the LocOS
    "decision 21" hint for a file that declares no charset;
  - format option `htmlhelp.utf8_fallback` (`ExtractOptions::format_options`, `"true"`; the constant
    `bilingua_htmlhelp::UTF8_FALLBACK`) is `HTMLHELP_UTF8_FALLBACK=1`: a text the ANSI code page cannot hold is written
    as UTF-8 instead of refusing the merge. Default off (as LocOS). The extract records it in the skeleton.
- Merge options (`bilingua_model::MergeOptions`, through `Filter::merge_with`):
  - default: with `doc.target_lang` set and at least one translated unit, the file is re-encoded in the target
    language's ANSI code page (LocOS `build_localized`); with nothing translated the original bytes come back.
  - `stamp_target_always`: localize (target code page, meta and Font charsets, hhp `Language=`) even when nothing is
    translated, as LocOS `build_localized` does.
  - `output_encoding: Utf8`: write UTF-8 (a UTF-8 BOM stays, a UTF-16 BOM goes) and rewrite a declared meta charset
    to `UTF-8`. **This is not what LocOS does for HTML Help**: LocOS writes the target ANSI code page. The LocOS adapter
    must pass `MergeOptions { output_encoding: Keep, stamp_target_always: true }` for hhc/hhk/hhp, not
    `MergeOptions::locos()`.
- Merges re-parse the built sitemap the way a browser does (LocOS `html_guard`, #676, ported in `src/browser.rs`):
  a value a browser would not read as that attribute value is written opaque, and a build whose markup a browser
  would read differently from the source is refused (`Error::Write`).
- Python wheel: nothing format-specific; it goes through the facade.

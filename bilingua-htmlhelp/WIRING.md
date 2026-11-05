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
  - `XxxFilter::merge_with(doc, &MergeOptions { utf8_fallback })` is `HTMLHELP_UTF8_FALLBACK=1`: a text the ANSI code
    page cannot hold is written as UTF-8 instead of refusing the merge. Default off (as LocOS). `Filter::merge` uses the
    default.
- Merge behaviour the adapter must know: with `doc.target_lang` set and at least one translated unit, the file is
  re-encoded in the target language's ANSI code page (LocOS `build_localized`); otherwise it is written in its own
  encoding. The LocOS behaviour "stamp the language even when nothing is translated" is NOT done (lead decision 11-04);
  when the adapter needs it, it needs the model's coming "stamp anyway" merge option.
- Python wheel: nothing format-specific; it goes through the facade.

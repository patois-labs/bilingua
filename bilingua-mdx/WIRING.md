# Wiring bilingua-mdx (the lead replaces the throwaway wire commit at merge)

- Format id: `mdx`. Extension: `mdx`. Filter type: `bilingua_mdx::Mdx` (unit struct, `Filter` impl).
- Facade `filters()`: `Box::new(bilingua_mdx::Mdx)` right after the Markdown filter. **`sniff` is always false** (MDX is
  plain text; only the extension claims it), so its position does not matter.
- Facade `Cargo.toml`: `bilingua-mdx = { workspace = true }`. Workspace: member `bilingua-mdx` and
  `bilingua-mdx = { path = "bilingua-mdx" }`.
- `bilingua/tests/contract.rs` `crate_of`: `"mdx" => "bilingua-mdx"`.
- `tools/check-portable.sh` and `.ps1`: `mdx` added to `CRATES`.
- Dependencies: `bilingua-model`, `regex` (workspace) and `toml = { version = "0.8", default-features = false,
  features = ["parse"] }` (already in `Cargo.lock` as a dev-dependency of `bilingua-toml`; here it checks and decodes
  TOML front matter). `proptest` and `serde_json` are crate-local dev-dependencies.
- Options: `Mdx::extract_with(bytes, &ExtractOptions, &MdxOptions)` takes `MdxOptions { attrs, front_matter_keys }`
  (the LocOS settings `LOC_TMS_MDX_ATTRS` and `LOC_TMS_FRONT_MATTER_KEYS`); `extract` uses the defaults. Size limits
  come from `ExtractOptions::limits`.
- LocOS adapter notes:
  - Tag ids are the LocOS token numbers ("1", "2", ...). A paired component is `Open`/`Close` with one id; a
    self-closing component, an unpaired or orphan tag and an `{expression}` are `Standalone`. No tag is `free`.
    Tag `label` is `jsx` or `expression`; `display` and the code strings are the exact source text. Code spans and
    backslash escapes are plain text in LocOS, so they are plain text here.
  - A unit whose target is set always goes through LocOS's re-materialisation (`{ } <` escaped, tokens put back,
    unknown tokens dropped), even when the target equals the source, exactly as LocOS builds. A unit with no target
    is written as the original bytes.
  - String attributes are units after all line units, `context.style = "a11y:mdx-attr"`, `context.key` the attribute
    name. They are not in the skeleton parts when they sit inside a line unit; the line unit carries them (hidden
    codes `$a<n>`, `$t<id>`), so the adapter must keep a document whole and not drop units.
  - Missing-alt report: `Document::warnings` entries with `kind = "missing_alt"`, `part = "empty" | "missing"`.
- Oracle corpus: `oracle/corpus/mdx/` (guide.mdx is the LocOS fixture; handbook.mdx and edge.mdx are made up). Nothing
  else may sit in that folder. `oracle/run.py` lists its FORMATS by extension: add `mdx` there (not done here: the
  oracle code is the lead's). `oracle/corpus/SOURCES.md` has no row for these files (shared file, left alone).
- Python wheel: nothing format-specific; it goes through the facade.
- Model needs: none. A merge option "stamp even when nothing is translated" does not apply (MDX has no language
  declaration); "always write UTF-8" is the LocOS behaviour (`encode_text_lossless`), here the file's own encoding is
  kept (B6), as for the other crates.

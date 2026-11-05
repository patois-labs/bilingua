# Wiring bilingua-dxf (the lead replaces the throwaway wire commit at merge)

- Format id `dxf`, extension `dxf` (LocOS registers none; the id is its lookup). Filter type `bilingua_dxf::Dxf` (unit
  struct, `Filter` impl).
- Workspace: member `bilingua-dxf` and `bilingua-dxf = { path = "bilingua-dxf" }`. Dev-dependencies `proptest` and
  `serde_json` are already in use. The crate needs only `bilingua-model` (no `regex`, no `encoding_rs`).
- Facade `filters()`: `Box::new(bilingua_dxf::Dxf)` right after rc. `sniff` is strict: the first group must be code 0 with
  value `SECTION` (comment groups 999 may come first) followed by a code 2 group, or the file starts with the binary
  DXF sentinel (so the extract can refuse it with a reason). Its place does not matter beyond being before text.
- Facade `Cargo.toml`: `bilingua-dxf = { workspace = true }`. `bilingua/tests/contract.rs` `crate_of`: `"dxf" => "bilingua-dxf"`.
- `tools/check-portable.sh` and `.ps1`: add `dxf` to `CRATES`.
- Oracle corpus: `oracle/corpus/dxf/` (34 files, a copy of `bilingua-dxf/tests/fixtures/corpus`) and the row in
  `oracle/corpus/SOURCES.md`.
- Options: none beyond `source_lang` (unused), `limits`. `include_locked` has no effect (LocOS has no locked DXF text).
- Merge options (`Filter::merge_with`): **no effect**. DXF has no language stamp, and LocOS keeps the drawing's code page
  on build, so the LocOS adapter must not turn `output_encoding: Utf8` on for dxf; the filter ignores it either way.
- Skeleton: `Text` with `encoding` = `dxf-bytes;codec=<Python codec>` (for example `dxf-bytes;codec=cp1252`); raw parts
  hold one character per file byte (U+0000..U+00FF). A consumer that reads the parts as text would see mojibake for
  non-ASCII bytes: only this crate's `merge` should read them.
- Inline codes: no placeholder is a plain-text token in LocOS, so none sets the `placeholder` label (labels are
  `mtext code`, `symbol`, `character`, `measurement`, `control`, `group`).
- Known difference: `Unit::context.location` is `line N` (the unit's first code line), which LocOS does not have.
- Python wheel: nothing format-specific; it goes through the facade.

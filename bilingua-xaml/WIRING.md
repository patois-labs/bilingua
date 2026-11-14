# Wiring for the lead

- Workspace: `"bilingua-xaml"` in `members`, `bilingua-xaml = { path = "bilingua-xaml" }` in
  `[workspace.dependencies]` (done in the wire commit). The crate uses `bilingua_model::{enc, guard}` (B6).
- Facade filter list: `Box::new(bilingua_xaml::Xaml::default())`, before `Html`/`TextFilter`. Format id `xaml`,
  extension `xaml`.
- `sniff`: XML whose first 4 KB names a winfx, dotnet (MAUI), Avalonia or xaml namespace, or `<ResourceDictionary`.
  Reasonably tight; it must come before any generic XML filter (wave 3 `xml`) and before `Html`.
- Options: `Xaml { view_attributes: bool }` (default false). Format option `xaml.view_attributes` (bool) sets it
  per extract (LocOS's `XAML_VIEW_ATTRIBUTES`); unset keeps the filter's own setting.
- Skeleton `encoding` values: `utf-8`, `utf-8-bom`, `utf-16le`, `utf-16be`, or an `encoding_rs` name
  (`windows-1252`, `shift_jis`, `gbk`, `euc-kr`, ...).
- Oracle: `oracle/corpus/xaml/` is the made-up corpus plus the LocOS suite fixtures; add `xaml` to `FORMATS` in
  `run.py`. LocOS needs `config.XAML_VIEW_ATTRIBUTES` for the view-attribute files.

# Wiring for the lead

- Workspace: add `"bilingua-rc"` to `members` and `bilingua-rc = { path = "bilingua-rc" }` to
  `[workspace.dependencies]`. Dev-dependencies are `proptest = "1"` and `serde_json` (both already in use).
  The crate needs `bilingua-model` and `regex`. Encodings come from the `encoding_rs` that `bilingua-model` re-exports.
- Facade: dependency `bilingua-rc = { workspace = true }`; filter `Box::new(bilingua_rc::Rc)` (a unit struct).
- Format id: `rc`. Extensions: `rc`. LocOS registers no extension for it either (`exts = ()`), so the id is the lookup.
- `sniff`: strict. It needs a resource-script line start: `STRINGTABLE`, `<id> DIALOG|DIALOGEX|MENU|MENUEX|VERSIONINFO|
  ACCELERATORS|DESIGNINFO`, `#include <winres.h|afxres.h|windows.h>` or `#pragma code_page`, in the first 8 KB (UTF-16 too).
  It can sit early in `filters()` (the wire commit puts it right after DTD).
- Options used: `source_lang` (only for the garbled-letters warning of a legacy code page), `target_lang`
  (LANGUAGE statements and VERSIONINFO language fields on merge, only when a unit is translated),
  `include_locked` (strings with no letter or digit outside their codes become locked units `k1`, `k2`, ...), `limits`.
- Skeleton `encoding` values: `utf-8`, `utf-8-bom`, `utf-16le` / `utf-16be` (with a BOM), `utf-16le-nobom`, or a code page
  label: `windows-1250`..`windows-1258`, `windows-874`, `shift_jis`, `gbk`, `euc-kr`, `big5`, `gb18030`, `euc-jp`,
  `ibm437`, `ibm850`, `ibm866`, `koi8-r`, `koi8-u`, `iso-8859-1`, `-2`, `-5`, `-7`, `-9`, `-15`.
  `utf-16le-nobom` and the single-byte pages are this crate's own tags (not in `bilingua_model::enc`).
- Inline codes: every escape (`\t`, `\n`, `\x41`, `\101`, `\q`) and every printf / FormatMessage placeholder is a
  standalone tag whose display and `standalone` code are the written text. `%1` and `%2!s!` (numbered) are `free`
  tags (LocOS `p` tokens); the rest keep their order (`x`). Labels are `escape` and `format`, never `placeholder`,
  because LocOS tokenises all of them (the adapter must not flatten them).
- Hidden codes on a unit: `~raw` (the literal as written) and `~acc` (menu items and controls: keep one `&`).
  The unit's `context.style` is LocOS's style (`rc:string`, `rc:menu`, `rc:control`, `rc:caption`, `rc:version`).
- Oracle: `oracle/corpus/rc/` holds the made-up corpus (12 files); add `rc` to `FORMATS` in `run.py`.
- Merge options still wanted from the model: "stamp the target language even when nothing is translated" (LocOS
  does) and "always write UTF-8" (LocOS keeps the file's encoding here, so not needed for rc).

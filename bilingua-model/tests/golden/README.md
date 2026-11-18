# Golden JSON for the unit model change

Made by main `626e6f04` on the M5 (2026-11-17), before any change to the unit model, so `tests/golden_json.rs`
can check that new code reads them and writes the same bytes back. Do not regenerate them from a later commit.

| File | Source |
|---|---|
| `docx__features.docx.skl.json`, `xlsx__menu.xlsx.skl.json`, `pptx__launch.pptx.skl.json` | `bilingua extract <file> -t fr -o x.xlf` (the memb bench command) on `oracle/corpus/docx/features.docx`, `xlsx/menu.xlsx`, `pptx/launch.pptx`; the skeleton the CLI writes next to `x.xlf` |
| `po__lumen.po.skl.json`, `arb__app_en.arb.skl.json`, `xcstrings__bakery.xcstrings.skl.json`, `json__strings.json.skl.json` | the same command on `oracle/corpus/po/lumen.po`, `arb/app_en.arb`, `xcstrings/bakery.xcstrings`, `json/strings.json` |
| `chattercat_docx.state.json` | `bilingua::extract` of `oracle/corpus/docx/features.docx` with the ChatterCAT server's options (en to fr, sentence, locked units included), the first four segments given their source as target and the states translated, confirmed, draft, reviewed, then `serde_json::to_vec` as the server's `persist` writes `state.json` |
| `old_docx.state.json`, `old_po.state.json` | copies of `core/catos-server/tests/fixtures/old-skeletons/data/files/{f1a1db279dad005,f1a1db279daf007}/state.json`, stored by the server of image 5620cb3e (Bilingua before 0.9) |
| `old_docx.state.resaved.json`, `old_po.state.resaved.json` | those two read by main into a `Document` and written again with `serde_json::to_vec` |

All the source files are invented text (see `oracle/corpus/SOURCES.md`). The generator was a throwaway test on
branch `tmp/bili-model-gen` (`bilingua-cli/tests/zz_golden_gen.rs`), run with `m5-test.sh`.

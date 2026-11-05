# Wiring for the lead

- Workspace: add `"bilingua-latex"` to `members` and `bilingua-latex = { path = "bilingua-latex" }` to
  `[workspace.dependencies]`. Dev-dependencies are `proptest = "1"` and `serde_json` (both already in use). The crate
  needs `bilingua-model` and `regex`.
- Facade: dependency `bilingua-latex = { workspace = true }`; filter `Box::new(bilingua_latex::Latex::default())`.
  `Latex` has one public field, `extra_inline: Vec<String>` (LocOS `LATEX_TEXT_COMMANDS`: extra inline text commands
  by name); build it with `Latex::with_inline_commands(["keyword"])`. The crate reads no environment variable (it must
  stay WASM portable), so the adapter passes the list.
- Format id: `tex` (LocOS's id). Extensions: `tex`, `latex`, `ltx`.
- `sniff`: strict. A `\documentclass`, `\usepackage`, `\RequirePackage` or `\begin{document}` at a line start in the
  first 8 KB. A chapter file that has none of them is found by extension only. It can sit anywhere in `filters()`.
- Options used: `include_locked` (units with no letter, for example a paragraph of only `\label`/`\ref`, become locked
  units `k1`, `k2`, ...), `limits`, `source_lang` (the legacy code page of an undeclared non-UTF-8 file, as the other
  text formats), `target_lang` (read nothing: see below).
- Skeleton `encoding`: the shared envelope grammar of `bilingua_model::enc` (`utf-8`, `utf-8-bom`, `utf-16le`,
  `utf-16be`, `windows-1252`, ... with `;nl=crlf`).
- Inline codes: a text command (`\emph{..}`, `\textbf`, `\footnote`, `\href{url}{..}` with its first argument kept,
  `\textcolor{c}{..}`, a bare `{..}` group) is an `Open`/`Close` pair; every other command with its arguments, every
  comment, every piece of math and every `\verb` is one standalone tag (LocOS `x` token). Tag ids are the LocOS
  token numbers ("1", "2", ...). Labels are the command name (`emph`, `group`), `comment`, `math` or `markup`; none
  is `placeholder` because LocOS tokenises all of them.
- Hidden code on a unit: `~raw`, the slot's source text as written (the merge writes it back for an unchanged unit).
  The unit's `context.kind` is `paragraph`, `heading`, `title`, `caption`, `label` (an `\item[..]`) or `cell`;
  `context.style` is `tex`.
- Oracle: `oracle/corpus/latex/` holds the made-up corpus (22 files; extension `tex`, so the oracle's `FORMATS`
  already needs `tex` -> `tex` if it is not there).
- Merge differences from LocOS (both deliberate, B6 and the lead decisions of 11-04):
  1. A Windows-1252 (or other legacy) file goes back in its own encoding when every character fits, and the
     `inputenc` option stays. When a translation does not fit, the file is written as UTF-8 and an `inputenc` option
     naming a legacy code page (`latin1`, `latin9`, `cp1252`, `ansinew`, `latin2`, `cp1251`, ...) is rewritten to
     `utf8`. LocOS always writes UTF-8 and rewrites it. **The adapter needs the model's "always write UTF-8" merge
     option for this**: with it on, the crate must also rewrite `inputenc` (it does not yet, because the option does
     not exist). Until then the adapter's LocOS suite will see Windows-1252 bytes where LocOS writes UTF-8.
  2. An untouched or echo merge never rewrites `inputenc` (LocOS rewrites it for a legacy file even when nothing
     changes).
- `target_lang` is read for nothing, as LocOS (`build_localized` is `build`): babel and polyglossia names differ per
  package (`ngerman` or `german`), so the target language is not guessed. If Brent wants `\usepackage[english]{babel}`
  switched to the target, it needs a language-name table and a decision; it is a question in the report.
- Model needs: none. The model's tag validation (`set_target`) refuses a translation that repeats a tag
  (`DuplicatedTag`), which LocOS accepts (the #742 case repeats a `\TeX` token); the adapter should write targets
  the way the other LocOS-parity crates do.

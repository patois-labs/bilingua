# bilingua-latex

LaTeX (`.tex`, `.latex`, `.ltx`) for Bilingua. A scanner over the source: no TeX engine runs, nothing is expanded or
followed (`\input`, `\include`, `\bibliography` stay as they are). The spec is the Localization OS `latex_filter.py`
and its `verify_latex_filter.py` suite; Okapi `okf_tex` is the second baseline.

## What becomes a unit

The prose of the document body (after `\begin{document}`; a file without one, such as a chapter pulled in with
`\input`, is all body). One unit each for a paragraph (split at blank lines, block commands, environment boundaries,
display math, `\item`, comments and, in tabular environments, at `&` and `\\`), the arguments of the block text
commands (`\section` .. `\subparagraph`, `\chapter`, `\caption`, `\title`, ...) including the optional short title,
and `\item[label]`. In the preamble only `\title` and `\subtitle` arguments.

Inline: a text command (`\emph`, `\textbf`, `\footnote`, `\href{url}{text}`, `\textcolor{c}{text}`, a bare `{..}`
group) is an `Open`/`Close` pair so its words are translated and the command is not. Every other command with its
arguments (`\label`, `\ref`, `\cite`, `\url`, `\includegraphics`, ...), every comment, every piece of inline math and
every `\verb` is one standalone tag. Never translated, kept byte for byte: the preamble, comments, math (`$..$`,
`$$..$$`, `\(..\)`, `\[..\]`, `equation`, `align`, ...), `verbatim`, `lstlisting`, `minted`, `comment`,
`filecontents`, `tikzpicture` and the like, environment names and arguments, tabular column specifications.

`\& \% \$ \# \_ \{ \}` read as the plain character and `~` as a no-break space; on merge every one of
`& % $ # _ { } ~ ^ \` in a translation is escaped again and a no-break space is written as `~`. A line break in a
translation becomes a space. A point tag that ends in a control word (`\TeX`) gets `{}` when letters or a blank follow
it in the translation. Braces always stay balanced: a close without its open is dropped, an open that is never closed
is closed.

An untouched merge is the original bytes (encoding, BOM and newlines kept). Only the slot of a changed unit is
rewritten.

## Tested against

* The LocOS suite: its 7 test functions, ported (`tests/locos_suite.rs`, 8 Rust tests; the config/encoding function
  is two tests, the byte differences below are the only changed assertions).
* The LocOS filter itself: `tests/locos_golden.rs`, 600 random and randomly broken documents in four encodings, the
  22-file corpus and the Okapi files; units identical in all 628 accepted files, 3 refused by both, 2685 build digests
  identical for UTF-8 inputs (`tests/tools/gen_golden.py` rebuilds the golden file with the LocOS venv).
* Okapi `okf_tex` on its three test resources (`tests/data/okapi`, `tests/tools/compare_okapi.py`): `Test02` 5 of 5
  units equal (codes removed); `Test01`/`Test03` 3 of 5 each. Differences, all LocOS decisions that win over Okapi:
  `\author{..}` is a unit in Okapi, a code in LocOS (it is not one of the text commands); TeX accent macros
  (`\v{S}\={\i}`) are decoded to Unicode letters by Okapi and kept as codes by LocOS, which splits the unit at the
  comment line before them; `\%` is read as `%` by LocOS and kept by Okapi; Okapi does not extract the table cells
  (`SMT`, `NMT`) or the `\caption`, LocOS does.
* Hostile input (`tests/hostile.rs`, proptest): random bytes, random text, LaTeX token soup, mutated fixtures, every
  truncation of every fixture, 100 000-deep braces, 100 000 unclosed `$`, `\[`, `\(`, `\verb`, `\begin{verbatim}`,
  `\item[` and so on in well under the time limit; typed errors for limits and encodings. The scanner is iterative;
  the group maps (`{`, `[`, `$`, blank lines) are one linear pass per text, so an unclosed delimiter repeated many
  times does not cost a rescan each. Pair nesting in a unit is capped at 40 levels and tokens at 1000 per unit (the
  rest becomes one standalone tag), as LocOS.
* Speed (`tests/speed.rs`, release): a 5.2 MB file with 80 471 units. Rust: extract 0.35 s, translate and merge
  0.39 s. LocOS filter (its venv): parse 7.3 s, build 13.7 s. About 29 times faster.

## Where it differs from LocOS

* Encoding on merge. LocOS exports a Windows-1252 file as UTF-8 and rewrites `\usepackage[latin1]{inputenc}` to `utf8`
  (and does so even when nothing changed). Bilingua keeps the file's encoding while every character fits and leaves
  `inputenc` alone; if a translation does not fit, the file becomes UTF-8 and a legacy `inputenc` option (`latin1`,
  `latin9`, `cp1252`, `ansinew`, and the other legacy names) is rewritten to `utf8`. The model's "always write UTF-8"
  option, when it exists, will need the same rewrite (see WIRING.md).
* `LATEX_TEXT_COMMANDS` (an environment variable in LocOS) is `Latex::with_inline_commands` here: the crate reads no
  environment.
* Unicode letters: where LocOS asks "is this a letter" it uses Python `isalpha`; this crate uses `\p{L}`, the same
  set, and Python's `isspace`, which also counts U+001C to U+001F. The known Rust `is_alphanumeric` difference for
  combining marks does not arise.
* Not guessed: babel and polyglossia language options are never changed on a target-locale merge (LocOS:
  `build_localized` is `build`).

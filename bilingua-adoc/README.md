# bilingua-adoc

AsciiDoc (`.adoc`, `.asciidoc`, `.asc`) for Bilingua. The spec and the baseline is the Localization OS filter
(`app/filters/asciidoc_filter.py`): `tests/locos_suite.rs` ports its 14 test functions (13 in full, the HTTP/CLI one
in part) and `tests/locos_golden.rs` checks 609 generated files, 5000+ units, their inline maps and 10000+ builds
against it byte for byte.

**Units** (in file order, then the header metadata): paragraphs (their lines joined), admonition paragraphs (the
`NOTE:` label stays outside), list items (`*`, `-`, `.`, `1.`, `a.`, `i)`, callouts, checklists), description list terms
and descriptions, section titles (`=` and `#`, a symmetric trailing marker stays), two-line titles, block titles,
table cells of `|===` and nested `!===` tables (cell specifiers stay), and the header's `:description:` and
`:keywords:` values (appended after every body unit, written back on one line).

**Never translated, kept byte for byte**: listing, literal, passthrough, comment and fenced blocks; `[source]`,
`[listing]`, `[literal]`, `[pass]`, `[comment]`, `[stem]` and diagram-styled blocks and paragraphs; indented literal
paragraphs; `//` comments; attribute entries (with their `\` continuations); author and revision lines; block
attribute lines; block macros (`image::`); `include::`, `ifdef::`, `ifndef::`, `ifeval::`, `endif::` (never followed);
breaks; CSV/DSV tables.

**Inline tags** (the LocOS tokens, one to one): paired for `*bold*`, `_it_`, `#mark#`, `**`, `__`, `##`, `^sup^`,
`~sub~`, `[.role]#x#` and the text of `link:`, `xref:`, `mailto:`, `footnote:`, `http(s):` macros and `<<ref,text>>`;
standalone for `{attribute}`, anchors, passthroughs, monospace, `image:`, `kbd:`, `btn:`, `menu:`, bare URLs and the
hard line break ` +`. On merge a constrained mark that a translation glues to a letter (Japanese, Chinese) is written
in its doubled form, and a URL glued to a letter becomes a `link:` macro.

**Merge**: only changed units are rewritten; every other byte is identical. A translation that opens with block
syntax gets an `{empty}` guard. With a target language and at least one translated unit the header gets (or has
rewritten) `:lang:`. The encoding (UTF-8, BOM, UTF-16, code pages), BOM and line endings are kept.

Known differences from LocOS: Unicode categories come from Rust (`isalpha`, `isalnum`, `\w` for non-ASCII); LocOS
writes UTF-8 for a file that was not UTF-8 and Bilingua keeps the file's encoding (B6); no `:lang:` stamp when nothing
is translated (lead decision 11-04); a header description that overlaps a body span (a malformed header) is dropped;
a locked unit (`include_locked`) is added for a span with no letter.

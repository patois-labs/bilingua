# bilingua-wiki

MediaWiki wikitext (`wiki`, `mediawiki`, `wikitext`) and DokuWiki (`dokuwiki`) for Bilingua: two filters, `MediaWiki` and
`DokuWiki`. The spec and the baseline is the Localization OS filter (`app/filters/wiki_filter.py`).
`tests/locos_suite.rs` ports its 14 test functions (13 in full; the 14th drives the LocOS HTTP API and CLI) and
`tests/locos_golden.rs` checks 350 generated pages (units and about 2500 builds) against LocOS byte for byte; a run
with 2500 pages gave 27 291 identical units and 17 578 identical builds.

**Units**: headings, paragraphs (their lines joined), list items (`* # : ;`, DokuWiki two-space `*` / `-`), `; term : definition`
(two units), table cells (MediaWiki `{| |+ ! !! | ||` with `attrs | content`; DokuWiki `^` / `|` rows, `:::` skipped), DokuWiki
`>` quotes, link labels, file and media captions, footnotes. **Never translated**: comments, `<nowiki>`, `<pre>`, `<code>`,
`<syntaxhighlight>`, math and the other extension tags, templates and parser functions, `{{{parameters}}}`, magic words,
signatures, categories, redirects, preformatted lines, `%%..%%`, `<code>`/`<file>`/`<html>`/`<php>` blocks, link targets,
URLs, HTML tags and entities.

**Template parameters** are protected unless the install lists them: `MediaWiki::with_template_params("Quote:text,Note:1")`
(LocOS `WIKI_TEMPLATE_PARAMS`), or per call `ExtractOptions::format_options["wiki.template_params"]` in the same spelling
(it adds to the filter's list). A template alone on its line gives each listed parameter its own unit; otherwise the value
is translated in place between two point tags.

**Tags** (the LocOS tokens one to one): paired for `'''bold'''`, `''italic''`, `'''''both'''''`, `**`, `//`, `__`, link labels
`[[target|label]]`, `[url label]`, file captions, `{{media|caption}}` and `((footnotes))`; standalone (`x`) for link targets,
templates, URLs, `<ref>` tags, entities, comments, `\` and `''mono''`.

**Merge**: only changed units are rewritten; every other byte is identical (encoding, BOM, line endings). `merge_with`:
`output_encoding: Utf8` writes UTF-8 whatever the file was (LocOS); `stamp_target_always` has nothing to stamp (neither wiki
declares a page language, so `target_lang` writes nothing). A translation cannot create markup: wiki syntax in the
translated words is written inside `<nowiki>`, a line break becomes a space, a table separator is escaped (`{{!}}`,
`%%|%%`), and a bare URL that a translation sets straight before more text (Chinese, Japanese) is ended with `<nowiki/>`.
Beyond LocOS (gate, 11-05; `tests/gate_injection.rs`, `tests/gate_render.rs`): markup that would form across a tag edge
(`{` before a template, `'` after an italic close, `[` before a bare URL, `/` before `//`) is broken; the line-start set
also covers table lines (`| ! } +`); a header cell escapes `!!` and a trailing `!`; a term escapes `:` and brackets; a
DokuWiki cell of `:::` stays literal; `]` in an external link label, `|` in a link or file caption and `{` `}` in a
DokuWiki media caption are inert; a template parameter translated in place is escaped like one on its own line, and single
braces in either are inert; URLs (`http://`, `mailto:`, DokuWiki `www.`) and `__switches__` (any case) typed in the words
are inert; a DokuWiki bare URL is ended before words that would extend it; and a separator inside our own `<nowiki>` is left
as it is. A LocOS build of a translation is the same as Bilingua's once these wrappers are taken out (`tests/locos_golden.rs`).

Known differences from LocOS: Unicode categories come from Rust (`isalpha`, `isalnum`, `\w` for non-ASCII); LocOS writes UTF-8
for a file that was not UTF-8 and Bilingua keeps the file's encoding (B6) unless `merge_with` asks for UTF-8; LocOS's
case-insensitive tag and namespace names also fold `ı` (U+0131), `ſ` (U+017F) and `K` (U+212A) onto ASCII (a Python `re`
quirk; MediaWiki and Bilingua compare ASCII only); an unclosed `<nowiki>` or `<pre>` in a page that is already broken can
be closed by the inert `<nowiki>` a translation needs, moving unit boundaries (as in LocOS). On purpose (see
MIGRATION.md): localized File namespaces and image options of the larger wikis are read, local option words case
sensitive, and a file link's caption is its last part that is not an option, where LocOS takes only the last part
(golden list `CAPTION_BEFORE_OPTIONS`).

**Okapi** (`okf_wiki`, its only test file `dokuwiki.txt`, gate run 11-05): Bilingua's 188 units are LocOS's exactly; 156 of
Okapi's 189 units with letters are identical in order. The rest are design differences where LocOS wins: Okapi makes
footnotes and media captions their own units (LocOS keeps them inside the paragraph as paired tags), keeps `''mono''`,
`%%..%%`, bare URLs and `<nowiki>` text as text (LocOS makes them codes), and splits a few lines LocOS joins.

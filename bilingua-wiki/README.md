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
(LocOS `WIKI_TEMPLATE_PARAMS`). A template alone on its line gives each listed parameter its own unit; otherwise the value is
translated in place between two point tags.

**Tags** (the LocOS tokens one to one): paired for `'''bold'''`, `''italic''`, `'''''both'''''`, `**`, `//`, `__`, link labels
`[[target|label]]`, `[url label]`, file captions, `{{media|caption}}` and `((footnotes))`; standalone (`x`) for link targets,
templates, URLs, `<ref>` tags, entities, comments, `\` and `''mono''`.

**Merge**: only changed units are rewritten; every other byte is identical (encoding, BOM, line endings). A translation
cannot create markup: wiki syntax in the translated words is written inside `<nowiki>`, a line break becomes a space, a
table separator is escaped (`{{!}}`, `%%|%%`), and a bare URL that a translation sets straight before more text (Chinese,
Japanese) is ended with `<nowiki/>`. Neither wiki declares a page language, so `target_lang` writes nothing.

Known differences from LocOS: Unicode categories come from Rust (`isalpha`, `isalnum`, `\w` for non-ASCII); LocOS writes UTF-8
for a file that was not UTF-8 and Bilingua keeps the file's encoding (B6); an unclosed `<nowiki>` or `<pre>` in a page that
is already broken can be closed by the inert `<nowiki>` a translation needs, moving unit boundaries (as in LocOS).

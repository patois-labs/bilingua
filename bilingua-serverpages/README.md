# bilingua-serverpages

Server pages for Bilingua: ASP.NET Web Forms (`.aspx`, `.ascx`), JSP (`.jsp`, `.jspf`), PHP templates (`.phtml`,
and a `.php` page that is not a language array), server-side includes (`.shtml`, `.shtm`, `.stm`) and ERB
(`.erb`). Five filters (`Phtml`, `Jsp`, `Aspx`, `Erb`, `Shtml`), one engine. The spec is the Localization OS filter
`app/filters/server_page_filter.py` and its suite `tests/verify_server_page_filter.py`; the HTML scanner under it is
a port of LocOS `html_filter.py` and `html_guard.py` that lives in `bilingua-htmlcore` (shared with
`bilingua-html`), so units and inline codes are LocOS's, token for token.

## What is a unit

* Block units of the page, as the LocOS HTML filter makes them: text of a `p`, `li`, `h1`, `td`, `div`, `title`,
  ... with its inline tags (`b`, `a`, `span`, `img`, `br`, ...) kept inside the unit as codes. Pairs are `Open` and
  `Close` tags, `img`, `br`, `wbr` and tags that are self closed are `Standalone` codes. A `translate="no"` or
  `class="notranslate"` run is one standalone code, written back verbatim.
* After the body units, the human-text attributes: `alt`, `title`, `aria-label` of `img`, `area` and `a`
  (style `a11y:img-alt`, `a11y:link-title`, ...), then the page metadata (`<meta>` description, keywords, og and
  twitter text; style `meta:description`, ...). An attribute whose value holds code is never a unit.
* Code never is. Every code island is found by a dialect lexer (PHP `<?php ?>`, `<?= ?>`, `<? ?>` with strings,
  comments and heredocs; `<% %>`, `<%= %>`, `<%@ %>`, `<%-- --%>`, `<%: %>`, `<%# %>`, `<%$ %>`; JSP `${ }`, `#{ }`
  and `<jsp:scriptlet>`; ERB `<%- -%>`, `<%== %>`, `<%% %>`; SSI `<!--#... -->`) and masked before the HTML scanner
  runs, so a `>` or a quote inside code cannot end a tag.
* An OUTPUT island (`<?= $n ?>`, `<%= x %>`, `${x}`, `<!--#echo -->`) is a `Standalone` code inside its sentence. A
  STATEMENT island (`<?php if ?>`, `<% } %>`, directives, comments) is a unit boundary like a block tag, so control
  flow never ends up inside a translated sentence.
* Text keeps its HTML escaping (`&amp;` stays `&amp;`); attribute units hold the unescaped text.

## Merge

The skeleton holds the decoded page. Merge re-derives the islands and the units from it, writes every island back
byte for byte, and puts a translated unit's text between its original codes. Text is escaped (`& < > " '`), a
dropped output code is put back at the end of its unit, a repeated one is written once, an unknown one is dropped,
and the JSP expression markers `${` and `#{` in a translation are escaped so they can never run. Translated
attribute values are escaped, and written as one opaque run (whitespace, `/`, `=` and backtick as character
references) on a page where the scanner and a browser may read the tag differently. A browser-equivalent re-parse of
the built page must find only the elements, attributes and script/style bodies of the source; otherwise the merge
fails with `Error::Write`.

With `target_lang` set and a unit translated, `<html lang>` (and `xml:lang`), `dir` (`rtl` for Arabic, Hebrew, ...) and
`<meta http-equiv="content-language">` are written, except where the value is code (`lang="<?= $l ?>"`).

## Code between attributes (differs from LocOS)

Server code between two attributes, after the tag name or before `>` / `/>` (`<div <%= attrs %> class="a">`,
`<input checked <?php echo $x ?> />`, `<option<?php if ($s): ?> selected="selected"<?php endif ?>>`) is an opaque code
of the tag. Attribute values keep their normal handling and the merge writes the code back byte for byte. LocOS refuses
these pages; Bilingua opens them (MIGRATION.md, "Differences from LocOS").

## Refused, never guessed

An unterminated island, an island used as a tag name or inside an attribute name (`<<?= $tag ?>>`,
`<div data-<?= $k ?>="v">`), a tag nested in a quoted attribute value around code, code that prints the start of an HTML tag the page finishes, a file that holds U+FDD0 or U+FDD1 (the masks) -> `Error::Parse`
with the LocOS message and line.

## API beyond `Filter`

* `Dialect` (`id`, `from_extension`, `code_islands`, `filter`).
* `server_page_for_html(text)`: the dialect for an `.html` page that holds a closed server island (LocOS #672).
* `php_has_page_text(bytes)`: whether a `.php` file is a template (page text outside its PHP islands) rather than PHP
  code; the facade routes `.php` between the language-array filter and `Phtml` with it.

## Differences from LocOS (all deliberate)

* The file's own encoding, byte order mark and line ends are kept on merge (B6); LocOS writes server pages as UTF-8.
  A UTF-8 BOM is not a text character of the first unit.
* A unit whose target equals its source is left as the original bytes (LocOS would re-escape its quotes).
* A JSP translation that ends in a backslash right before a `${` island writes the backslash as `&#92;`, so it cannot
  turn the expression into literal text.
* The text of a translated unit is escaped as one run up to the next code that writes something, so a dropped tag,
  an empty close or a repeated island cannot join `$` and `{` into a JSP expression (LocOS escapes each piece
  between tokens). A tag whose markup holds code (`<a href="<?php track(); ?>">`) is written at most once.
* The XML cross-check LocOS runs with lxml on well-formed pages (`html_guard._check_xml`) is not ported; the browser
  check is.
* A tag library's script or style element (`<s:script>`, `<aui:script>`, any `prefix:script` or `prefix:style` with
  an end tag) is code, like `<script>`: its body is never a unit (LocOS reads it as page text). Without an end tag it
  is read as HTML, so no text after it is lost.
* `skeleton["a11y_images"]` (missing-alt findings) is not produced.
* `check_markup` (B6) also refuses a page that declares `<!ENTITY`.

## Tests

`tests/locos_suite.rs` (the 13 LocOS test functions), `tests/locos_golden.rs` (806 pages: the suite fixtures, edge pages,
the oracle corpus and 750 random pages, compared with what LocOS makes of them, unit by unit and byte by byte, made by
`tests/tools/gen_golden.py`), `roundtrip.rs` (11 languages, legacy encodings, UTF-16, BOM, line ends),
`hostile.rs` and `gate_adversarial.rs` (random bytes, mutated and truncated fixtures, hostile translations, proptest),
`speed.rs`, `corpus.rs` (`oracle/corpus/serverpages/`, made by `tests/tools/gen_corpus.py`).

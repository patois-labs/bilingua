# Wiring bilingua-serverpages (the lead replaces the throwaway wire commit at merge)

* Format ids (the LocOS format names): `phtml`, `jsp`, `aspx`, `erb`, `shtml`. Filter types: `bilingua_serverpages::{Phtml,
  Jsp, Aspx, Erb, Shtml}` (unit structs, `Filter` impls).
* Extensions: `phtml`; `jsp`, `jspf`; `aspx`, `ascx`; `erb`; `shtml`, `shtm`, `stm`.
* Facade `filters()`: add the five boxes after `bilingua_php::Php` (any place before `bilingua_html::Html` and the text
  filter works). `sniff` is tight: `Phtml` wants `<?php` or `<?=` in the first 4 KB, `Jsp` a `<%@ page|taglib|include`
  or `<jsp:`, `Aspx` a `<%@ Page|Control|Master|Register|Import|...` directive (and not `language="java"`), `Shtml` an
  `<!--#include|echo|config|exec|set`, `Erb` a `<%=` without a `<%@`. None of them accepts a plain HTML page, JSON or an
  empty file. `Erb` is the loosest (`<%=` alone), so put it last of the five; all five go before `Html`.
* Facade `Cargo.toml`: `bilingua-serverpages = { workspace = true }`. Workspace: member `bilingua-serverpages` and
  `bilingua-serverpages = { path = "bilingua-serverpages" }`. `proptest` is a crate-local dev-dependency.
* `bilingua/tests/contract.rs` `crate_of`: `"phtml" | "jsp" | "aspx" | "erb" | "shtml" => "bilingua-serverpages"`.
* `bilingua/tools/check-portable.sh` and `.ps1`: add `serverpages` to `CRATES`.
* LocOS also routes two more things; the facade does it with the helpers this crate exports:
  * `.html` / `.htm` that hold a closed server island (LocOS #672, `server_page_for_html`): call
    `bilingua_serverpages::server_page_for_html(text)?` on the decoded page; `Some(dialect)` -> `dialect.filter()`
    extracts and merges it (the HTML filter would offer a `<%@ page %>` directive as page text). A page that mixes PHP
    and `<% %>` is refused with `Error::Parse`.
  * `.php`: the language-array filter (`bilingua_php::Php`) when its `sniff` is true, else `Phtml` when
    `php_has_page_text(bytes)?` is true, else the file is PHP code with nothing to translate (refuse, as LocOS does).
* Options: none. `ExtractOptions::include_locked` is ignored (LocOS has no locked units here). `X::extract_with(bytes, opts,
  &Limits)` takes explicit limits (defaults are the LocOS ones).
* Inline codes: every code maps to one LocOS token. Paired tags are `g` (`Open` + `Close`); an unclosed open tag is an
  `Open` with no `Close`, which `to_locos_text` prints as `x` (LocOS keeps it as an unpaired `g`); `img`, `br`, `wbr`,
  self-closed tags, `translate="no"` runs and output islands are `Standalone` (`x`). Nothing is a plain-text placeholder.
* Merge options still to come in the model (LocOS behaviour): "always write UTF-8" and "stamp the target language
  even when nothing is translated". Until then the file's own encoding is kept and the language is stamped only when a
  unit is translated.
* Oracle corpus: `oracle/corpus/serverpages/` (31 files, made by `tests/tools/gen_corpus.py`; 11 languages, BOM, CRLF,
  CR, Windows-1252, Windows-1251, Shift_JIS, UTF-16). `oracle/run.py` lists FORMATS by extension: add `phtml`, `jsp`,
  `jspf`, `aspx`, `ascx`, `erb`, `shtml`, `shtm`, `stm` there (the oracle code is the lead's).
* Python wheel: nothing format-specific; it goes through the facade.

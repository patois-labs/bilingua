# bilingua-dtd

DTD entity file filter for Bilingua (Mozilla localization `.dtd` files). Port of the Localization OS DTD filter; the
baseline for this format is LocOS and Okapi `okf_dtd`.

- The value of each general internal entity with a letter or digit outside its references and markup is a unit.
  Parameter entities, external entities (`SYSTEM`/`PUBLIC`), `%name;` references, processing instructions, comments
  and other declarations stay in the skeleton byte for byte. Nothing is ever expanded, resolved or fetched, and the
  file is never given to an XML parser (no XXE, no entity expansion attacks).
- The translator sees `&amp; &quot; &apos;` and character references as plain characters. Other references
  (`&brandShortName;`) and markup (`<html:b>`, `&lt;i>`) are inline tags. Build escapes `&`, the value's own quote and
  `%` (`&#037;`), keeps the quote character and every other byte; an unchanged value keeps its spelling.
- UTF-8 only (BOM kept), as Mozilla ships. Comments above an entity are its notes.
- Tests: `tests/locos_suite.rs` (the six LocOS cases), `tests/locos_golden.rs` (522 files against recorded LocOS
  output, made by `tests/tools/gen_golden.py`), `tests/okapi.rs` (Okapi's DTD test files and the corpus against
  recorded `okf_dtd` output, `tests/tools/gen_okapi.py`), round trips, hostile input (proptest), speed
  (`cargo test -p bilingua-dtd --release --test speed -- --nocapture`, then `tests/tools/speed_locos.py`).

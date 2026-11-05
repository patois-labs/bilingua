# bilingua-desktop

Two Bilingua filters in one crate: freedesktop `.desktop` launcher entries (`DesktopFilter`, id `desktop`) and Haiku
plain-text catalogs (`CatkeysFilter`, id `catkeys`). The spec is Localization OS's `desktop_filter.py` and
`catkeys_filter.py`; `tests/locos_desktop_catkeys.rs` ports its suite and `tests/locos_golden.rs` compares 1 200 random
files, every fixture and 1 731 locale codes against the LocOS filters (digests of every build).

## `.desktop`

* Units: unlocalized `Name`, `GenericName`, `Comment`, `Keywords` of `[Desktop Entry]` and `[Desktop Action ...]`;
  a `Keywords` list gives one unit per item. Text is unescaped (`\s \n \t \r \ \;`).
* Merge with `target_lang` and at least one translated unit: `Key[ll_CC]=value` after each source key (or replacing
  the file's own line for that locale). BCP 47 to POSIX: `fr-FR` is `fr_FR`, `sr-Latn` is `sr@latin`,
  `zh-Hans` is `zh_CN` (`posix_locale`). Without a target language the changed values are spliced in place
  (edge spaces written as `\s`). Nothing translated: the original bytes.
* `desktop_records(bytes)`: the LocOS `records()` view.

## `.catkeys`

* Units: the unescaped source of every entry that has a letter. The translation column (fourth) is what a merge
  writes, escaped like Haiku (`\ \n \t \" \r`). `include_locked` adds the letterless entries as locked units.
* Merge with `target_lang` and a translation: the header language becomes Haiku's English name (`german`,
  `portuguese (brazil)`; `language_name`, table made from Babel, the library LocOS uses). The header fingerprint is
  kept, as in LocOS (`catkeys_fingerprint` recomputes it).
* `catkeys_records(bytes)`: the LocOS `records()` view (source, context, comment, current translation).

## Both

UTF-8 only (a BOM is kept; CRLF kept); other encodings are refused with `Error::Encoding`, as LocOS does. No inline
tags: placeholders (`%s`) stay plain text. A file with nothing to translate returns a document with no units (LocOS
refuses it). Limits come from `ExtractOptions::limits`.

`tests/tools/` holds the generators (fixtures, goldens, locale table, speed) that need the LocOS venv.

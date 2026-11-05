# Wiring for the lead

- Workspace: `"bilingua-email"` in `members`, `bilingua-email = { path = "bilingua-email" }` in
  `[workspace.dependencies]` (done in the last commit, replace at merge). Dependencies: `bilingua-model`,
  `bilingua-html` (used as a library for `text/html` parts) and `regex`. Dev-dependencies: `proptest = "1"`,
  `serde_json`.
- Facade (`bilingua/Cargo.toml` and `filters()`): `Box::new(bilingua_email::Email)`. Format id `eml`, extension `eml`.
- `sniff` is medium-loose: the first header block must start with a Capitalised field name, have two or more
  `Name: value` fields, and include one of From, To, Cc, Subject, Date, Message-ID, Received, Return-Path,
  MIME-Version, Delivered-To or Reply-To. It must come BEFORE YAML and properties (which accept any `word: text`
  line) and before the plain text filter. LocOS itself finds `.eml` by extension only.
- `bilingua/tests/contract.rs`: `"eml" => "bilingua-email"` in `crate_of`.
- `tools/check-portable.sh` and `.ps1`: `email` in `CRATES`.
- Skeleton: `Skeleton::Zip` with one entry `message.eml` holding the message bytes (`ZipData::Raw`, base64 in JSON); the
  model has no binary text skeleton. The parser stamp is `bilingua-email`, schema 1.
- Oracle: `oracle/corpus/email/` (36 made-up messages); add `eml` to `FORMATS` in `oracle/run.py` and `scoreboard.py`
  and a line to `oracle/corpus/SOURCES.md` (not touched here).
- Options: none beyond `target_lang` (Content-Language and HTML `lang`/`dir` stamp, only when a unit is translated)
  and `limits`. LocOS's "stamp even when nothing is translated" is the merge option the adapter will need (it stamps
  Content-Language and the HTML language on every localized build).
- Inline tags: all HTML tags come from `bilingua-html` and map one to one onto LocOS `g`/`x` tokens; this crate has no
  placeholder tags of its own (plain text and the Subject keep `%s`, `{0}` as plain text, as LocOS does).

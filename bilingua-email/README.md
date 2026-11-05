# bilingua-email

Email message (`.eml`, RFC 5322 plus MIME) filter for Bilingua. Spec and baseline: the Localization OS filter
`app/filters/email_filter.py` and its suite `tests/verify_email_filter.py`.

```rust
use bilingua_email::Email;
use bilingua_model::*;

let mut doc = Email.extract(&bytes, &ExtractOptions { target_lang: Some("de-DE".into()), ..Default::default() })?;
doc.set_target("subject", vec![Inline::text("Neuer Betreff")])?;
let out = Email.merge(&doc)?;
```

## What it does

- **Units**, in this order: the `Subject`, then every `text/plain` and `text/html` part that is not an attachment, in
  file order (both halves of a `multipart/alternative`). Ids: `subject`, `p<part>.<n>` (paragraph `n` of text part
  number `part`), `h<part>.<id>` (a unit of the HTML part, with the id `bilingua-html` gave it). Other headers,
  attachments, inline images, `message/rfc822` (forwarded mail), boundaries, preamble and epilogue are never touched.
- **Plain text**: paragraphs as LocOS cuts them. `format=flowed` (RFC 3676, `delsp` too) is unwrapped into its logical
  lines; fixed text joins hard-wrapped lines (a line of 60 characters or more followed by a line that is not
  indented and not a list item) and a translation is wrapped again at the paragraph's own width. Quoted lines (`>`),
  the signature separator and blank lines stay as they are.
- **HTML parts** go through `bilingua-html` as a library: block units, inline tags as Bilingua `Inline` tags (they map
  one to one onto LocOS tokens), alt text and titles. `<meta>` page text is not offered (LocOS turns it off for mail
  parts). The part is read as UTF-8 text, whatever the `<meta charset>` says: the declaration describes the transfer.
- **Encodings**: quoted-printable and base64 decoded on reading; a changed part is written back in its own charset and
  transfer encoding (quoted-printable lines of 76 characters or fewer, base64 lines of 76). When the translation does
  not fit (a character the charset lacks, non-ASCII in a `7bit` part, an 8bit line over 998 bytes) the part goes
  to UTF-8 and/or quoted-printable and its `Content-Type` / `Content-Transfer-Encoding` headers say so. A line of
  the encoded body that reads as a MIME delimiter makes the part base64, so a translation can never end its part.
- **Charsets**: ASCII, UTF-8, ISO-8859-1 (true Latin-1), Windows-125x, ISO-8859-x, KOI8-R/U, Shift_JIS, EUC-JP,
  ISO-2022-JP (written by this crate: `encoding_rs` cannot encode it), GBK/GB2312/GB18030, Big5, EUC-KR. Text marked
  `us-ascii` that is really UTF-8 or Windows-1252 is read as that and relabelled when changed. UTF-16, UTF-7 and the
  WHATWG "replacement" charsets are refused with a plain message.
- **Subject**: RFC 2047 encoded words decoded (several words, mixed with plain text, folded, RFC 6532 raw UTF-8), and
  a changed Subject written as encoded words in the charset of the first original word when it holds the text, else
  UTF-8, folded at 78 columns exactly as Python's `email.header` does for LocOS.
- **Target language**: when at least one unit is translated and `target_lang` is set, `Content-Language` headers that
  already exist are set to the target, and each HTML part gets `lang` (and `dir` for right-to-left languages) on
  `<html>` and `<meta http-equiv="content-language">`. Nothing translated: the original bytes come back.
- **Untouched merge** returns the original bytes. A changed part is spliced into the original message, so everything
  else is byte for byte what came in.
- **Refused**: signed or encrypted mail (S/MIME, PGP/MIME, inline PGP: a translation would break the signature),
  files that are not messages, unknown or mismatched charsets, damaged base64, messages with no Subject or text.
- **Limits**: 500 MIME parts, 16 levels of nesting, 1 MB of headers per block, plus the input, unit and line limits of
  `bilingua-model` (`ExtractOptions::limits`). The walker is iterative: nesting depth costs no stack.

## Skeleton

`Skeleton::Zip` with one entry, `message.eml`, holding the message bytes; the parser stamp is `bilingua-email`,
schema 1. `merge` reads the message again and splices the changes in.

## Differences from LocOS, on purpose

- Encoded-word labels for gb2312 and big5: Python's `Header` writes the codec name (`eucgb2312_cn`, `big5-tw`), which
  no mail reader knows; this crate writes `gb2312` and `big5`, and reads the Python names too.
- The `Subject` of a message that cannot be decoded (an unknown charset in an encoded word, damaged base64) is
  skipped, as in LocOS, but a damaged base64 word does not crash as it does there.
- Attribute value escaping inside HTML parts follows `bilingua-html` (an apostrophe in an attribute value is
  written as it is; LocOS writes `&#x27;`).
- Panics inside `bilingua-html` (its entity scan slices text at byte 34 after an `&`, which can fall inside a
  character) are caught on native targets and returned as an `html` parse error.

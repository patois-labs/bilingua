# bilingua-rc

Windows resource scripts (`.rc`) for Bilingua. The spec and the baseline is the Localization OS filter
(`app/filters/rc_filter.py`); `tests/locos_suite.rs` ports its 13 test cases and `tests/locos_golden.rs` checks 500+
files and 5000+ builds against it byte for byte.

**Extracted** (in file order): STRINGTABLE strings; DIALOG / DIALOGEX captions and the text of LTEXT, RTEXT, CTEXT,
PUSHBUTTON, DEFPUSHBUTTON, PUSHBOX, the check and radio buttons, GROUPBOX and `CONTROL` (not image controls);
MENU / MENUEX items and popups; the VERSIONINFO strings people read (FileDescription, ProductName, Comments,
LegalCopyright, LegalTrademarks). Everything else (ACCELERATORS, RCDATA, TEXTINCLUDE, DLGINIT, icon and bitmap file
names, user types, preprocessor lines) stays in the skeleton. `#include`, `#define` and `#if` are never evaluated.

**Inline codes**: escapes (`\t`, `\n`, `\x41`, octal) and printf / FormatMessage placeholders are standalone tags that
keep their written form. `%1` and `%2!s!` are free (they may move); the others keep their order. `""` and `\"` are
quotes, `\\` a backslash, a backslash at the end of a line a continuation. In `L"..."` strings `\xhhhh` of a
printable character is shown as that character.

**Merge**: only changed units are rewritten (the L prefix is kept); every other byte is identical. A menu item or
control whose source has exactly one `&` accelerator keeps exactly one in the translation. With a target language
and at least one translated unit, the LANGUAGE statements (when they all name one language), the StringFileInfo block id
and the `Translation` language are set to the target's Windows LANGID.

**Encodings** (as LocOS): UTF-16 with a BOM, BOM-less UTF-16 LE (found by its zero bytes), UTF-8 with a BOM, the code page
a `#pragma code_page(N)` declares, UTF-8, then Windows-1252. A file with several different pragmas (one section per
language) is read section by section, each in the page of the pragma before it (the text before the first one in
UTF-8 or Windows-1252); LocOS refuses these. The file is written
back in the same encoding; a translation that does not fit an ANSI code page makes the file UTF-8 with every pragma
set to 65001 (one is added at the top when there was none).

Known differences from LocOS: Unicode categories come from Rust, not Python (`isalnum`, `isprintable`, `\d` outside
strings); a multi-byte code page is `encoding_rs`, except that Shift_JIS reads the four single bytes Python's `cp932`
reads. A merge where nothing is translated returns the original bytes even when a target language is set (LocOS
stamps the language anyway).

# bilingua-dxf

AutoCAD DXF (ASCII) for Bilingua. A port of the Localization OS filter (`app/filters/dxf_filter.py`).

**Units** (ENTITIES and BLOCKS sections only, in file order): TEXT (group 1); MTEXT (the 250-character group 3 chunks
and the final group 1, joined into one unit); ATTRIB (group 1); ATTDEF (group 1 and the group 3 prompt); the embedded
MTEXT of a multiline attribute (groups 1/3 after group 101); DIMENSION text overrides (empty, a single space and a
bare `<>` are not text); MULTILEADER / MLEADER content (group 304). Layer and block names, tags, style names, handles,
XDATA, HEADER, TABLES, CLASSES and OBJECTS are never units. A value with no letter outside its codes is not a unit.

**Inline codes** are Bilingua tags: MTEXT format codes (`\P \X \L \l \O \o \K \k`, `\f..; \F..; \H..; \C..; \c..; \T..;
\Q..; \W..; \A..; \p..;`, a whole `\S..;` stack), `%%d %%c %%p %%%`, `%%nnn`, `%%u`/`%%o` toggles, `^` controls, the
DIMENSION `<>` and `[]` are standalone tags; a balanced `{..}` group is an open/close pair; `\ \{ \}` are the literal
character, `\~` is a no-break space, `\U+XXXX` and `\M+NXXXX` show as the character. The tag ids are LocOS's token
numbers, so `locos::to_locos_text` gives the LocOS text one to one.

**Encodings**: AC1021 and later are UTF-8; earlier files use `$DWGCODEPAGE` (ANSI_874 to ANSI_1258, DOS437..DOS869,
ANSI_932/936/949/950); no `$ACADVER` reads as UTF-8 when the file is valid UTF-8 with non-ASCII bytes, else
Windows-1252. The code pages are Python's own (generated tables, `tools/`), so what LocOS reads and writes matches
byte for byte. A translated character the page cannot hold is written as `\U+XXXX` (a surrogate pair above U+FFFF).
Binary DXF is refused with a plain reason.

**Merge** splices only the changed values; every other byte is the original. The skeleton is bytes (one character per
byte) so a file with stray bytes outside its text still merges back exactly. A changed MTEXT is re-chunked into
group 3 pieces of at most 250 characters and a final group 1, never splitting an escape.

Limits: `ExtractOptions::limits` (input size, units, characters per unit); the group-code budget is LocOS's own, 200
group codes per allowed unit.

Tests: `tests/locos_suite.rs` (the 10 LocOS test functions), `locos_golden.rs` (253 files against recorded LocOS output),
`roundtrip.rs`, `hostile.rs` (proptest), `speed.rs`. Generators are in `tools/` and `tests/tools/`.

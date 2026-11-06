# bilingua-xmltree

Internal to Bilingua: the small XML tree that the Localization OS ports share (the XLIFF family in `bilingua-xliff`,
DOCX in `bilingua-docx`). It reads well-formed XML into an arena of nodes the way lxml reports it (resolved
namespaces, attribute values normalised, references and CDATA resolved, the byte span of every element), edits it
with the few lxml operations those ports need (append, insert, addnext, addprevious, remove) and writes it back the
way `lxml.etree.tostring` does. `parse_noblanks` drops ignorable white space the way python-docx's parser does
(`remove_blank_text=True`), and `Dom::canonical` gives a form in which two trees compare equal when they are the
same XML.

This crate is published only because the format crates depend on it. It is not a public API: anything in it may
change in any release. Use the format crates (or the `bilingua` facade) instead.

Apache-2.0, see LICENSE and NOTICE.

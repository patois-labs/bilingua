//! BILI-118 (unit model step 5): what each inline code is. One table ([`code_info`]) gives an element's chip label
//! and its type, so the two cannot disagree; the attributes come from the element's own start tag. Writers never
//! read any of it; under the LocOS profile `profile::annotate` clears it.

use crate::xml::{scan_attrs, unescape, Tree};
use bilingua_model::{CodeData, CodeType};

/// The text fields of ODF 1.3 that can sit in a paragraph (`text:page-number`, `text:date`, ...).
const FIELDS: &[&str] = &[
    "page-number", "page-count", "page-continuation", "date", "time", "chapter", "sequence", "sequence-ref", "variable-get",
    "variable-set", "variable-input", "user-field-get", "user-field-input", "user-defined", "expression", "text-input",
    "author-name", "author-initials", "creator", "title", "subject", "keywords", "description", "file-name", "template-name",
    "sheet-name", "initial-creator", "creation-date", "creation-time", "modification-date", "modification-time", "print-date",
    "print-time", "printed-by", "editing-cycles", "editing-duration", "word-count", "character-count", "paragraph-count",
    "table-count", "image-count", "object-count", "bookmark-ref", "reference-ref", "database-display", "database-name",
    "hidden-text", "conditional-text",
];

/// The chip label and the type of an element, by its local name.
pub(crate) fn code_info(local: &str) -> (&str, CodeType) {
    match local {
        "a" => ("link", CodeType::Link),
        "tab" => ("tab", CodeType::Tab),
        "line-break" => ("line break", CodeType::Linebreak),
        "soft-page-break" => ("page break", CodeType::Other),
        "annotation" => ("comment", CodeType::Other),
        "bookmark" | "bookmark-start" | "bookmark-end" => ("bookmark", CodeType::Other),
        "span" => ("span", CodeType::Style),
        "note" | "note-ref" => (local, CodeType::Footnoteref),
        "placeholder" => ("placeholder", CodeType::Placeholder),
        l if FIELDS.contains(&l) => (l, CodeType::Field),
        l => (l, CodeType::Other),
    }
}

/// An attribute's decoded value by local name, whatever its namespace (`xlink:href`, `office:title`).
fn attr_local(t: &Tree, node: u32, local: &str) -> Option<String> {
    let tag = t.start_tag(node);
    scan_attrs(tag)
        .into_iter()
        .find(|a| {
            let q = &tag[a.name_start..a.name_end];
            !q.starts_with("xmlns") && q.rsplit_once(':').map_or(q, |(_, l)| l) == local
        })
        .map(|a| unescape(&tag[a.val_start..a.val_end]).into_owned())
}

/// The code of element `node` with its type and attributes: `href` and `title` for a link, `style` for a span,
/// `field` (the element name) for a field.
pub(crate) fn typed(code: CodeData, t: &Tree, node: u32) -> CodeData {
    let local = t.local(node);
    let ty = code_info(local).1;
    let mut code = code.with_code_type(Some(ty));
    match ty {
        CodeType::Link => {
            code.set_attr("href", attr_local(t, node, "href").unwrap_or_default());
            code.set_attr("title", attr_local(t, node, "title").unwrap_or_default());
        }
        CodeType::Style => code.set_attr("style", attr_local(t, node, "style-name").unwrap_or_default()),
        CodeType::Field => code.set_attr("field", local),
        _ => {}
    }
    code
}

#[cfg(test)]
mod tests {
    use super::code_info;
    use bilingua_model::CodeType;

    /// Every label the odf crate sets, with its type (BILI-118).
    #[test]
    fn label_type_pairs() {
        let cases = [
            ("a", "link", CodeType::Link),
            ("tab", "tab", CodeType::Tab),
            ("line-break", "line break", CodeType::Linebreak),
            ("soft-page-break", "page break", CodeType::Other),
            ("annotation", "comment", CodeType::Other),
            ("bookmark", "bookmark", CodeType::Other),
            ("bookmark-start", "bookmark", CodeType::Other),
            ("bookmark-end", "bookmark", CodeType::Other),
            ("span", "span", CodeType::Style),
            ("note", "note", CodeType::Footnoteref),
            ("note-ref", "note-ref", CodeType::Footnoteref),
            ("placeholder", "placeholder", CodeType::Placeholder),
            ("page-number", "page-number", CodeType::Field),
            ("date", "date", CodeType::Field),
            ("frame", "frame", CodeType::Other),
            ("meta", "meta", CodeType::Other),
        ];
        for (local, label, ty) in cases {
            assert_eq!(code_info(local), (label, ty), "{local}");
        }
    }
}

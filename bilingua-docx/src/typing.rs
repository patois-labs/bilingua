//! BILI-118 (unit model step 5): what each inline code is. The type comes from the same fact as the chip label:
//! [`label_type`] for the fixed labels the extractor gives its codes, [`fmt_type`] for a formatting code (from the
//! same run features `names_of` turns into its label). Attributes come from the code's own markup: the link
//! target from the part's relationships, a field's name, an image's file and alt text. Writers never read any of
//! it, and the LocOS profile (`locos`) builds its own codes without it.

use crate::xml::{local_name, parse, parse_attrs, Node};
use crate::zipio::RawEntry;
use bilingua_model::{CodeData, CodeType};
use std::collections::HashMap;

/// The type of a code with a fixed label (every label the extractor sets apart from the formatting ones).
pub(crate) fn label_type(label: &str) -> Option<CodeType> {
    Some(match label {
        "link" => CodeType::Link,
        "field" => CodeType::Field,
        "tab" => CodeType::Tab,
        "break" => CodeType::Linebreak,
        "footnote-ref" => CodeType::Footnoteref,
        "image" => CodeType::Image,
        "smart tag" | "custom xml" | "direction" | "content control" | "ruby" | "math" | "textbox" | "symbol" | "code" => {
            CodeType::Other
        }
        _ => return None,
    })
}

/// The type of a formatting code from its run features (`fmt_features`, `dml_features`): the one feature that
/// marks the text (bold, italic, underline, strike, super- or subscript, a character style), colour, size, font,
/// caps and highlight not counted. None of them, or several, is `other`. A style also gives its style id.
pub(crate) fn fmt_type(features: &[String]) -> (CodeType, Option<&str>) {
    let marks: Vec<&str> = features
        .iter()
        .map(String::as_str)
        .filter(|f| !matches!(*f, "color" | "size" | "font" | "caps" | "highlight"))
        .collect();
    match marks.as_slice() {
        ["bold"] => (CodeType::Bold, None),
        ["italic"] => (CodeType::Italic, None),
        ["underline"] => (CodeType::Underline, None),
        ["strike"] => (CodeType::Strike, None),
        ["superscript"] => (CodeType::Superscript, None),
        ["subscript"] => (CodeType::Subscript, None),
        [s] if s.starts_with("style ") => (CodeType::Style, Some(&s["style ".len()..])),
        _ => (CodeType::Other, None),
    }
}

/// Fills the type and attributes of a code with a fixed label. `display` is the chip text (a field's is
/// `{NAME}` or `{NAME result}`); `rels` the part's relationships (id to target).
pub(crate) fn type_code(cd: &mut CodeData, label: &str, display: &str, rels: &HashMap<String, String>) {
    let ty = label_type(label);
    cd.set_code_type(ty);
    match ty {
        Some(CodeType::Link) => {
            let open = cd.open_markup().unwrap_or_default();
            let a = parse_attrs(open);
            let get = |n: &str| a.iter().find(|(k, _)| local_name(k) == n).map(|(_, v)| v.as_str());
            let href = match (get("id").and_then(|id| rels.get(id)), get("anchor")) {
                (Some(t), Some(anchor)) => format!("{t}#{anchor}"),
                (Some(t), None) => t.clone(),
                (None, Some(anchor)) => format!("#{anchor}"),
                (None, None) => String::new(),
            };
            let title = get("tooltip").unwrap_or_default().to_string();
            cd.set_attr("href", href);
            cd.set_attr("title", title);
        }
        Some(CodeType::Field) => {
            let name = display.trim_start_matches('{').trim_end_matches('}').split_whitespace().next().unwrap_or_default().to_string();
            cd.set_attr("field", name);
        }
        Some(CodeType::Image) => {
            let xml = cd.standalone_markup().unwrap_or_default();
            let alt = start_tag(xml, "docPr").map(parse_attrs).and_then(|a| a.into_iter().find(|(k, _)| k == "descr").map(|(_, v)| v));
            let src = start_tag(xml, "blip")
                .map(parse_attrs)
                .and_then(|a| a.into_iter().find(|(k, _)| local_name(k) == "embed").map(|(_, v)| v))
                .and_then(|id| rels.get(&id).cloned());
            cd.set_attr("src", src.unwrap_or_default());
            cd.set_attr("alt", alt.unwrap_or_default());
        }
        _ => {}
    }
}

/// The first start tag in `xml` whose local name is `local` (`<wp:docPr ...>`), up to its `>`.
fn start_tag<'a>(xml: &'a str, local: &str) -> Option<&'a str> {
    let mut from = 0;
    while let Some(p) = xml[from..].find('<') {
        let s = from + p;
        let rest = &xml[s + 1..];
        let end = rest.find(|c: char| c.is_ascii_whitespace() || c == '/' || c == '>').unwrap_or(rest.len());
        if local_name(&rest[..end]) == local {
            let close = rest.find('>').map_or(xml.len(), |q| s + 1 + q + 1);
            return Some(&xml[s..close]);
        }
        from = s + 1;
    }
    None
}

/// The relationships of `part` (id to target as written), from its `_rels/<file>.rels`; empty when there is none
/// or it does not parse.
pub(crate) fn part_rels(entries: &[RawEntry], part: &str) -> HashMap<String, String> {
    let (dir, file) = part.rsplit_once('/').map_or(("", part), |(d, f)| (d, f));
    let path = if dir.is_empty() { format!("_rels/{file}.rels") } else { format!("{dir}/_rels/{file}.rels") };
    let mut out = HashMap::new();
    let Some(e) = entries.iter().find(|e| e.name == path) else { return out };
    let Some(nodes) = std::str::from_utf8(&e.data).ok().and_then(|t| parse(t).ok()) else { return out };
    fn walk(nodes: &[Node], out: &mut HashMap<String, String>) {
        for n in nodes {
            if let Node::El(e) = n {
                if local_name(&e.name) == "Relationship" {
                    if let (Some(id), Some(t)) = (e.attr("Id"), e.attr("Target")) {
                        out.entry(id).or_insert(t);
                    }
                }
                walk(&e.children, out);
            }
        }
    }
    walk(&nodes, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every fixed label the extractor sets, with its type (BILI-118: the pair cannot drift apart unnoticed).
    #[test]
    fn label_type_pairs() {
        let pairs = [
            ("link", CodeType::Link),
            ("field", CodeType::Field),
            ("tab", CodeType::Tab),
            ("break", CodeType::Linebreak),
            ("footnote-ref", CodeType::Footnoteref),
            ("image", CodeType::Image),
            ("smart tag", CodeType::Other),
            ("custom xml", CodeType::Other),
            ("direction", CodeType::Other),
            ("content control", CodeType::Other),
            ("ruby", CodeType::Other),
            ("math", CodeType::Other),
            ("textbox", CodeType::Other),
            ("symbol", CodeType::Other),
            ("code", CodeType::Other),
        ];
        for (label, ty) in pairs {
            assert_eq!(label_type(label), Some(ty), "{label}");
        }
        assert_eq!(label_type("bold"), None, "formatting labels are typed by fmt_type");
    }

    #[test]
    fn fmt_type_pairs() {
        let f = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let cases: [(&[&str], CodeType, Option<&str>); 11] = [
            (&["bold"], CodeType::Bold, None),
            (&["italic"], CodeType::Italic, None),
            (&["underline"], CodeType::Underline, None),
            (&["strike"], CodeType::Strike, None),
            (&["superscript"], CodeType::Superscript, None),
            (&["subscript"], CodeType::Subscript, None),
            (&["style Strong"], CodeType::Style, Some("Strong")),
            (&["bold", "color", "size"], CodeType::Bold, None),
            (&["bold", "italic"], CodeType::Other, None),
            (&["color"], CodeType::Other, None),
            (&[], CodeType::Other, None),
        ];
        for (features, ty, style) in cases {
            assert_eq!(fmt_type(&f(features)), (ty, style), "{features:?}");
        }
    }

    #[test]
    fn attrs_from_markup() {
        let rels: HashMap<String, String> =
            [("rId4".to_string(), "https://x.com/".to_string()), ("rId9".to_string(), "media/image1.png".to_string())].into_iter().collect();
        let mut link = CodeData::paired("<w:hyperlink r:id=\"rId4\" w:tooltip=\"Go &amp; see\" w:history=\"1\">", "</w:hyperlink>");
        type_code(&mut link, "link", "<a>", &rels);
        assert_eq!(link.code_type(), Some(CodeType::Link));
        assert_eq!(link.attr("href"), Some("https://x.com/"));
        assert_eq!(link.attr("title"), Some("Go & see"));
        let mut anchor = CodeData::paired("<w:hyperlink w:anchor=\"_Toc1\">", "</w:hyperlink>");
        type_code(&mut anchor, "link", "<a>", &rels);
        assert_eq!(anchor.attr("href"), Some("#_Toc1"));
        assert_eq!(anchor.attr("title"), None);
        let mut field = CodeData::standalone("<w:fldSimple/>");
        type_code(&mut field, "field", "{PAGE 3}", &rels);
        assert_eq!((field.code_type(), field.attr("field")), (Some(CodeType::Field), Some("PAGE")));
        let mut img = CodeData::standalone("<w:drawing><wp:inline><wp:docPr id=\"1\" name=\"P\" descr=\"A cat\"/><a:blip r:embed=\"rId9\"/></wp:inline></w:drawing>");
        type_code(&mut img, "image", "<img>", &rels);
        assert_eq!((img.attr("src"), img.attr("alt")), (Some("media/image1.png"), Some("A cat")));
        let mut tab = CodeData::standalone("<w:tab/>");
        type_code(&mut tab, "tab", "<tab>", &rels);
        assert_eq!((tab.code_type(), tab.attrs().len()), (Some(CodeType::Tab), 0));
    }

    /// An extracted paragraph: each inline code gets the type of its label, the link its target and tooltip, the
    /// field its name; the writer-private codes (`_o`, `_base`) stay untyped.
    #[test]
    fn extract_fills_types() {
        use crate::tests::{body, extract, make};
        let xml = body(
            "<w:p><w:r><w:t xml:space=\"preserve\">Plain text here and </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>bold</w:t></w:r><w:r><w:t xml:space=\"preserve\"> then </w:t></w:r><w:hyperlink r:id=\"rId7\" w:tooltip=\"Tip\"><w:r><w:t>a link</w:t></w:r></w:hyperlink><w:r><w:tab/><w:t>after the tab</w:t></w:r><w:fldSimple w:instr=\" PAGE \"><w:r><w:t>3</w:t></w:r></w:fldSimple></w:p>",
        );
        let rels = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId7\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink\" Target=\"https://x.com/\" TargetMode=\"External\"/></Relationships>";
        let doc = extract(&make(&[("word/document.xml", &xml), ("word/_rels/document.xml.rels", rels)]));
        let u = &doc.units[0];
        let by = |l: &str| u.codes().values().find(|c| c.label() == Some(l)).unwrap_or_else(|| panic!("no {l} code in {:?}", u.codes()));
        assert_eq!(by("bold").code_type(), Some(CodeType::Bold));
        let link = by("link");
        assert_eq!((link.code_type(), link.attr("href"), link.attr("title")), (Some(CodeType::Link), Some("https://x.com/"), Some("Tip")));
        assert_eq!(by("tab").code_type(), Some(CodeType::Tab));
        let field = by("field");
        assert_eq!((field.code_type(), field.attr("field")), (Some(CodeType::Field), Some("PAGE")));
        for (k, c) in u.codes() {
            if k.starts_with('_') {
                assert_eq!((c.code_type(), c.attrs().len()), (None, 0), "{k}");
            }
        }
        // the JSON of an untyped code is what it was; a typed one adds `type` (and `attrs`)
        let j = serde_json::to_string(u.codes()).unwrap();
        assert!(j.contains("\"type\":\"link\",\"attrs\":{\"href\":\"https://x.com/\",\"title\":\"Tip\"}"), "{j}");
    }

    /// Under the LocOS profile the codes stay untyped (the profile builds its own).
    #[test]
    fn locos_profile_leaves_types_unset() {
        use crate::tests::doc_of;
        use bilingua_model::Filter;
        let bytes = doc_of("<w:p><w:r><w:t xml:space=\"preserve\">Plain and </w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>bold</w:t></w:r><w:r><w:tab/><w:t>x</w:t></w:r></w:p>");
        let opts = bilingua_model::ExtractOptions::new().with_profile(bilingua_model::profile::LOCOS);
        let doc = crate::Docx.extract(&bytes, &opts).unwrap();
        assert!(doc.units.iter().all(|u| u.codes().values().all(|c| c.code_type().is_none() && c.attrs().is_empty())));
    }
}

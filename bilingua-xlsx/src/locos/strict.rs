//! Strict Open XML (ISO/IEC 29500 Strict): LocOS's `ooxml_strict.py`. A Strict package is read with every Strict
//! namespace URI of its XML parts swapped for its Transitional twin (`to_transitional`); a part the export rewrote
//! is mapped back (`part_to_strict`), and every other part keeps its original bytes. The swap only touches a URI
//! standing alone in a quoted attribute value, never element text. A Strict URI the table does not know is refused.

const STRICT_PREFIX: &str = "http://purl.oclc.org/ooxml/";
const T: &str = "http://schemas.openxmlformats.org/";
const STRICT_RELS: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships";
const TRANS_RELS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// Strict name (after the prefix) and Transitional name (after `T`).
const NAMESPACES: &[(&str, &str)] = &[
    ("spreadsheetml/main", "spreadsheetml/2006/main"),
    ("wordprocessingml/main", "wordprocessingml/2006/main"),
    ("presentationml/main", "presentationml/2006/main"),
    ("drawingml/main", "drawingml/2006/main"),
    ("drawingml/chart", "drawingml/2006/chart"),
    ("drawingml/chartDrawing", "drawingml/2006/chartDrawing"),
    ("drawingml/diagram", "drawingml/2006/diagram"),
    ("drawingml/picture", "drawingml/2006/picture"),
    ("drawingml/spreadsheetDrawing", "drawingml/2006/spreadsheetDrawing"),
    ("drawingml/wordprocessingDrawing", "drawingml/2006/wordprocessingDrawing"),
    ("drawingml/lockedCanvas", "drawingml/2006/lockedCanvas"),
    ("drawingml/compatibility", "drawingml/2006/compatibility"),
    ("drawingml/table", "drawingml/2006/table"),
    ("officeDocument/math", "officeDocument/2006/math"),
    ("officeDocument/bibliography", "officeDocument/2006/bibliography"),
    ("officeDocument/characteristics", "officeDocument/2006/characteristics"),
    ("officeDocument/customXml", "officeDocument/2006/customXml"),
    ("officeDocument/docPropsVTypes", "officeDocument/2006/docPropsVTypes"),
    ("officeDocument/extendedProperties", "officeDocument/2006/extended-properties"),
    ("officeDocument/customProperties", "officeDocument/2006/custom-properties"),
    ("officeDocument/sharedTypes", "officeDocument/2006/sharedTypes"),
    ("schemaLibrary/main", "schemaLibrary/2006/main"),
    ("officeDocument/relationships", "officeDocument/2006/relationships"),
];

/// Relationship type names spelled differently in the two (Strict, Transitional).
const REL_RENAMES: &[(&str, &str)] = &[("extendedProperties", "extended-properties"), ("customProperties", "custom-properties")];

/// The message LocOS gives for a Strict part it cannot map.
pub const REFUSAL: &str = "This XLSX file is saved as Strict Open XML with parts this version cannot read. Save it in the standard Office format (for example Excel Workbook, Word Document or PowerPoint Presentation) and upload it again.";

fn map_strict(uri: &str) -> Option<String> {
    if let Some(rest) = uri.strip_prefix(STRICT_PREFIX) {
        if let Some((_, t)) = NAMESPACES.iter().find(|(s, _)| *s == rest) {
            return Some(format!("{T}{t}"));
        }
    }
    let name = uri.strip_prefix(STRICT_RELS)?.strip_prefix('/')?;
    let name = REL_RENAMES.iter().find(|(s, _)| *s == name).map(|(_, t)| *t).unwrap_or(name);
    Some(format!("{TRANS_RELS}/{name}"))
}

fn map_transitional(uri: &str) -> String {
    if let Some(rest) = uri.strip_prefix(T) {
        if let Some((s, _)) = NAMESPACES.iter().find(|(_, t)| *t == rest) {
            return format!("{STRICT_PREFIX}{s}");
        }
    }
    if let Some(name) = uri.strip_prefix(TRANS_RELS).and_then(|r| r.strip_prefix('/')) {
        let name = REL_RENAMES.iter().find(|(_, t)| *t == name).map(|(s, _)| *s).unwrap_or(name);
        return format!("{STRICT_RELS}/{name}");
    }
    uri.to_string()
}

/// Python's `\s` for bytes.
fn py_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

/// Replaces every `(["'])(<prefix>[^"'<>\s]*)\1` match by `map` (Python's `re.sub`, left to right). `skip` names
/// continuations after the prefix the pattern does not match (a negative look-ahead).
fn swap(xml: &[u8], prefix: &str, skip: &[&str], map: &mut dyn FnMut(&str) -> Result<String, ()>) -> Result<Vec<u8>, ()> {
    let p = prefix.as_bytes();
    let mut out = Vec::with_capacity(xml.len());
    let mut i = 0;
    let mut copied = 0;
    while i < xml.len() {
        let q = xml[i];
        if (q == b'"' || q == b'\'') && xml[i + 1..].starts_with(p) && !skip.iter().any(|s| xml[i + 1 + p.len()..].starts_with(s.as_bytes())) {
            let mut j = i + 1 + p.len();
            while j < xml.len() && !matches!(xml[j], b'"' | b'\'' | b'<' | b'>') && !py_space(xml[j]) {
                j += 1;
            }
            if j < xml.len() && xml[j] == q {
                let uri = String::from_utf8_lossy(&xml[i + 1..j]);
                let new = map(&uri)?;
                out.extend_from_slice(&xml[copied..=i]);
                out.extend_from_slice(new.as_bytes());
                out.push(q);
                copied = j + 1;
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    out.extend_from_slice(&xml[copied..]);
    Ok(out)
}

/// One XML part with every Strict namespace and relationship type made Transitional (`part_to_transitional`);
/// `Err` when a Strict URI is not one LocOS knows.
pub fn part_to_transitional(xml: &[u8]) -> Result<Vec<u8>, ()> {
    if !contains(xml, b"purl.oclc.org/ooxml/") {
        return Ok(xml.to_vec());
    }
    let out = swap(xml, STRICT_PREFIX, &[], &mut |u| map_strict(u).ok_or(()))?;
    if contains(&out, b"\"http://purl.oclc.org/ooxml/") || contains(&out, b"'http://purl.oclc.org/ooxml/") {
        return Err(());
    }
    Ok(out)
}

/// The inverse, for a part the export re-serialised (`part_to_strict`).
pub fn part_to_strict(xml: &[u8]) -> Vec<u8> {
    if !contains(xml, b"schemas.openxmlformats.org/") {
        return xml.to_vec();
    }
    swap(xml, T, &["package/", "markup-compatibility/"], &mut |u| Ok(map_transitional(u))).unwrap_or_else(|_| xml.to_vec())
}

pub fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// True when the package's root relationships (`_rels/.rels`) name a Strict main part (`is_strict`).
pub fn is_strict(root_rels: Option<&[u8]>) -> bool {
    root_rels.is_some_and(|r| contains(r, format!("{STRICT_RELS}/officeDocument").as_bytes()))
}

/// The members `to_transitional` rewrites.
pub fn is_xml_member(name: &str) -> bool {
    let n = name.to_lowercase();
    n.ends_with(".xml") || n.ends_with(".rels") || n.ends_with(".vml") || name == "[Content_Types].xml"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_the_names() {
        let s = b"<a xmlns=\"http://purl.oclc.org/ooxml/spreadsheetml/main\" t='http://purl.oclc.org/ooxml/officeDocument/relationships/extendedProperties'>http://purl.oclc.org/ooxml/spreadsheetml/main</a>";
        let t = part_to_transitional(s).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&t),
            "<a xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" t='http://schemas.openxmlformats.org/officeDocument/2006/relationships/extended-properties'>http://purl.oclc.org/ooxml/spreadsheetml/main</a>"
        );
        assert_eq!(part_to_strict(&t), s.to_vec());
        // an unknown Strict URI is refused; package and markup-compatibility names are never mapped back
        assert!(part_to_transitional(b"<a x=\"http://purl.oclc.org/ooxml/nope\"/>").is_err());
        let pkg = b"<a x=\"http://schemas.openxmlformats.org/package/2006/relationships\"/>";
        assert_eq!(part_to_strict(pkg), pkg.to_vec());
        // a quote that does not close the URI is not a match
        let odd = b"<a x='http://purl.oclc.org/ooxml/drawingml/main\"/>";
        assert!(part_to_transitional(odd).is_err());
    }
}

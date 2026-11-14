//! Bilingual translation tables in DOCX (Articulate Storyline 360 "Export to Word"), as Localization OS
//! `app/filters/docx_bilingual.py`.
//!
//! Storyline exports the course text to a Word document whose tables carry the source text and a column titled
//! "Translation"; only that column may change, and the import needs the same number of strings. Translated as an
//! ordinary DOCX, every cell would be machine translated (string ids, headers, the source column). So when the main
//! document has such a table, only the paragraphs of the Translation cells are units; every other unit of the
//! document (other parts, alt text, headings) is locked. The walk and the unit ids do not change.
//!
//! Detection is narrow, as LocOS: a table whose first row has a cell reading exactly "Translation" (case does not
//! matter, a trailing colon is ignored) with a source header ("Original Text", "Original", "Source Text" or
//! "Source") somewhere to its left, and at least one later row with text in the Translation cell. Rows whose cell
//! count differs from the header row and repeated header rows are skipped. A document that merely contains such a
//! table (a style guide with a term table) keeps its prose: the switch happens only when the text outside every
//! table (headings and titles aside) is smaller than the source column.

use crate::xml::{El, Node, Ns};
use std::collections::HashSet;

const SOURCE_HEADERS: &[&str] = &["original text", "original", "source text", "source"];

/// What the detection found, for the upload notice.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Found {
    /// Paragraph numbers (`El::ord` in the main part) inside Translation cells.
    pub keep: HashSet<usize>,
    pub tables: usize,
    pub rows: usize,
    pub empty_rows: usize,
}

fn all_w<'a>(e: &'a El, ns: &Ns, local: &str, out: &mut Vec<&'a El>) {
    for c in e.elements() {
        if ns.is_w(&c.name, local) {
            out.push(c);
        }
        all_w(c, ns, local, out);
    }
}

fn kids<'a>(e: &'a El, ns: &Ns, local: &str) -> Vec<&'a El> {
    e.elements().filter(|c| ns.is_w(&c.name, local)).collect()
}

/// Every `w:t` text inside, stripped (Python `strip()`).
fn cell_text(e: &El, ns: &Ns) -> String {
    let mut ts = Vec::new();
    all_w(e, ns, "t", &mut ts);
    let s: String = ts.iter().map(|t| t.text()).collect();
    bilingua_model::py_strip(&s).to_string()
}

fn norm(s: &str) -> String {
    let words: Vec<&str> = s.split(bilingua_model::py_isspace_char).filter(|w| !w.is_empty()).collect();
    words.join(" ").trim_end_matches(':').to_lowercase()
}

fn columns(header: &[String]) -> Option<(usize, usize)> {
    for (j, name) in header.iter().enumerate() {
        if name == "translation" {
            if let Some(i) = (0..j).rev().find(|i| SOURCE_HEADERS.contains(&header[*i].as_str())) {
                return Some((i, j));
            }
        }
    }
    None
}

/// Characters in paragraphs outside every table, headings and titles aside.
fn prose_chars(e: &El, ns: &Ns, in_table: bool) -> usize {
    let mut n = 0;
    for c in e.elements() {
        if ns.is_w(&c.name, "tbl") {
            n += prose_chars(c, ns, true);
        } else if ns.is_w(&c.name, "p") && !in_table {
            let style = kids(c, ns, "pPr").first().and_then(|ppr| kids(ppr, ns, "pStyle").first().and_then(|s| ns.attr(s, "val")));
            let sid = style.unwrap_or_default().to_lowercase();
            if !(sid.starts_with("heading") || sid.starts_with("title")) {
                let mut ts = Vec::new();
                all_w(c, ns, "t", &mut ts);
                n += ts.iter().map(|t| t.text().chars().count()).sum::<usize>();
            }
            n += prose_chars(c, ns, in_table);
        } else {
            n += prose_chars(c, ns, in_table);
        }
    }
    n
}

/// The Translation-cell paragraphs of the main part (its parsed and numbered nodes), or `None` when the document
/// has no bilingual table with translation text (or has more prose than source text).
pub fn translation_paragraphs(nodes: &[Node], ns: &Ns) -> Option<Found> {
    let root = nodes.iter().find_map(|n| if let Node::El(e) = n { Some(e) } else { None })?;
    let mut f = Found::default();
    let mut src_chars = 0usize;
    let mut tables = Vec::new();
    all_w(root, ns, "tbl", &mut tables);
    for tbl in tables {
        let trs = kids(tbl, ns, "tr");
        if trs.len() < 2 {
            continue;
        }
        let head = kids(trs[0], ns, "tc");
        let header: Vec<String> = head.iter().map(|tc| norm(&cell_text(tc, ns))).collect();
        let Some((src, tgt)) = columns(&header) else { continue };
        let mut found = 0;
        for tr in &trs[1..] {
            let cells = kids(tr, ns, "tc");
            if cells.len() != head.len() {
                continue; // a merged label row (scene, slide, layer)
            }
            if cells.iter().map(|tc| norm(&cell_text(tc, ns))).collect::<Vec<_>>() == header {
                continue; // a repeated header row
            }
            if cell_text(cells[tgt], ns).is_empty() {
                if !cell_text(cells[src], ns).is_empty() {
                    f.empty_rows += 1;
                }
                continue;
            }
            let mut ps = Vec::new();
            all_w(cells[tgt], ns, "p", &mut ps);
            f.keep.extend(ps.iter().map(|p| p.ord));
            src_chars += cell_text(cells[src], ns).chars().count();
            found += 1;
        }
        if found > 0 {
            f.tables += 1;
            f.rows += found;
        }
    }
    if f.keep.is_empty() || prose_chars(root, ns, false) > src_chars {
        return None;
    }
    Some(f)
}

/// The upload notice, LocOS's wording.
pub fn notice(f: &Found) -> String {
    let mut msg = format!(
        "Detected a bilingual translation table (Articulate Storyline 'Export to Word' layout): only the Translation column is translated ({} rows in {} tables). IDs, source text, headings and other content are left unchanged.",
        f.rows, f.tables
    );
    if f.empty_rows > 0 {
        msg.push_str(&format!(
            " {} rows have an empty Translation cell and are left empty; re-export from Storyline so the Translation column is filled.",
            f.empty_rows
        ));
    }
    msg
}

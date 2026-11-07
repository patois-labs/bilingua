//! The Localization OS profile for Excel workbooks: a port of LocOS's `xlsx_filter.py` (walk 2, cell mode) and
//! `xlsx_package.py` (the in-place export).
//!
//! Units are what openpyxl gives LocOS: every worksheet in workbook order (hidden ones included, chartsheets left
//! out), cells by row then column, a cell whose value is a string that is not a formula and has a letter or digit
//! (Python `str.isalnum`). A shared or inline rich string is its plain text (the runs joined, phonetic runs left
//! out). Then the text-box and shape paragraphs of each worksheet's drawing (style `text box`), numbered after the
//! last cell, with the PowerPoint filter's run tokens (`r` pairs numbered 1.. by run, `x` points numbered after the
//! runs; a paragraph with one run and nothing else is plain text). Sheet and cell protection, hidden rows and
//! columns and the Bilingua options (`skip_hidden`, comments, sheet names) do not change the walk, as in LocOS.
//!
//! The export writes into the original package: a changed cell that held a shared string keeps pointing into the
//! shared string table (the `si` rewritten in place when every cell using it gets the same translation, else a new
//! `si` appended and the cell repointed), any other changed cell becomes an inline string; a rich string keeps its
//! run format only when all its text runs have the same `rPr`. A target language is stamped as `dc:language` in
//! `docProps/core.xml` (the part made when there is none) and on the runs of every translated text-box paragraph;
//! a right-to-left target turns every worksheet view right to left. Only the parts that change are rewritten; a
//! Strict Open XML workbook comes back Strict.
//!
//! The document carries the original package as a `Skeleton::Binary`.

mod comments;
mod dom;
mod formula;
mod strict;
mod table;
mod values;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_modes;

pub use table::{report_options, table_mode};

use bilingua_model::{guard, parser_info, profile, CodeData, Document, Error, ExtractOptions, Granularity, Inline, MergeOptions, Skeleton, TagKind, Unit, UnitContext};
use dom::Dom;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

const M: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const PKG_RELS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const CT: &str = "http://schemas.openxmlformats.org/package/2006/content-types";
const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const CP: &str = "http://schemas.openxmlformats.org/package/2006/metadata/core-properties";
const DC: &str = "http://purl.org/dc/elements/1.1/";
const CORE_TYPE: &str = "application/vnd.openxmlformats-package.core-properties+xml";
const CORE_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties";
/// LocOS's style for a drawing paragraph.
pub const TEXT_BOX: &str = "text box";
/// `Unit::hidden` key: the unit's LocOS number.
const KEY: &str = "k";

fn bad(msg: impl Into<String>) -> Error {
    Error::Parse { format: "xlsx".into(), msg: msg.into() }
}

fn invalid(name: &str) -> Error {
    bad(format!("This XLSX file has invalid spreadsheet XML in {name}. Save it again in Excel."))
}

// ------------------------------------------------------------------------------------------- paths

fn dirname(p: &str) -> &str {
    p.rfind('/').map(|i| &p[..i]).unwrap_or("")
}

/// Python's `posixpath.normpath`.
fn normpath(p: &str) -> String {
    if p.is_empty() {
        return ".".into();
    }
    let lead = if p.starts_with("//") && !p.starts_with("///") {
        2
    } else if p.starts_with('/') {
        1
    } else {
        0
    };
    let mut out: Vec<&str> = Vec::new();
    for comp in p.split('/') {
        if comp.is_empty() || comp == "." {
            continue;
        }
        if comp != ".." || (lead == 0 && out.is_empty()) || out.last() == Some(&"..") {
            out.push(comp);
        } else if !out.is_empty() {
            out.pop();
        }
    }
    let s = format!("{}{}", "/".repeat(lead), out.join("/"));
    if s.is_empty() {
        ".".into()
    } else {
        s
    }
}

/// The package path of a relationship target relative to `part` (OPC 9.3), or `None` when it leaves the package.
fn resolve(part: &str, target: &str) -> Option<String> {
    let path = if target.starts_with('/') {
        normpath(target.trim_start_matches('/'))
    } else {
        let d = dirname(part);
        normpath(&if d.is_empty() { target.to_string() } else { format!("{d}/{target}") })
    };
    if path.starts_with("..") || path.starts_with('/') || path == "." {
        return None;
    }
    Some(path)
}

fn rels_name(part: &str) -> String {
    let (d, f) = match part.rfind('/') {
        Some(i) => (&part[..i], &part[i + 1..]),
        None => ("", part),
    };
    if d.is_empty() {
        format!("_rels/{f}.rels")
    } else {
        format!("{d}/_rels/{f}.rels")
    }
}

/// openpyxl's and LocOS's cell coordinate (`$A$1` allowed): (row, column).
fn coord(r: &str, name: &str) -> Result<(i64, i64), Error> {
    let s = r.trim();
    let s = s.strip_prefix('$').unwrap_or(s);
    let letters: String = s.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    let rest = &s[letters.len()..];
    let rest = rest.strip_prefix('$').unwrap_or(rest);
    if letters.is_empty() || letters.len() > 3 || rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid(name));
    }
    let col = letters.to_ascii_uppercase().bytes().fold(0i64, |n, b| n * 26 + (b - b'A' + 1) as i64);
    let row = rest.parse::<i64>().map_err(|_| invalid(name))?;
    Ok((row, col))
}

/// A row's `r` (an integer, or a float with no fraction).
fn row_number(v: &str, name: &str) -> Result<i64, Error> {
    let t = v.trim();
    if let Ok(n) = t.replace('_', "").parse::<i64>() {
        return Ok(n);
    }
    match t.parse::<f64>() {
        Ok(f) if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.0e15 => Ok(f as i64),
        _ => Err(invalid(name)),
    }
}

// ------------------------------------------------------------------------------------------- package

/// The text of an XML part as lxml reads it: UTF-8 (a BOM dropped), or UTF-16 with its BOM (OPC allows both; a
/// rewritten part is written as UTF-8, as LocOS's `save` writes it).
fn decode_part(raw: &[u8]) -> Option<String> {
    if let Some(b) = raw.strip_prefix(&[0xEF, 0xBB, 0xBF][..]) {
        return String::from_utf8(b.to_vec()).ok();
    }
    let utf16 = |b: &[u8], le: bool| -> Option<String> {
        if !b.len().is_multiple_of(2) {
            return None;
        }
        let u: Vec<u16> = b.chunks(2).map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) }).collect();
        String::from_utf16(&u).ok()
    };
    if let Some(b) = raw.strip_prefix(&[0xFF, 0xFE][..]) {
        return utf16(b, true);
    }
    if let Some(b) = raw.strip_prefix(&[0xFE, 0xFF][..]) {
        return utf16(b, false);
    }
    String::from_utf8(raw.to_vec()).ok()
}

/// The workbook package (LocOS's `xlsx_package.Package`) over the Transitional view of its parts.
struct Package {
    /// Every entry name of the original package.
    names: HashSet<String>,
    /// The XML text of each member, Strict names swapped when the package is Strict.
    parts: HashMap<String, Vec<u8>>,
    roots: HashMap<String, Dom>,
    dirty: BTreeSet<String>,
    added: Vec<String>,
    cells: HashMap<String, BTreeMap<(i64, i64), usize>>,
    /// Deferred shared-string edits: (sheet part, c element, string index, text).
    shared: Vec<(String, usize, usize, String)>,
    shared_strings: Option<String>,
    sheets: Vec<String>,
    strict: bool,
    /// The workbook part's name.
    workbook: String,
    /// Sheets whose rows and cells all carry their address (column mode inserts cells), and their rows by number.
    explicit: HashSet<String>,
    rows: HashMap<String, BTreeMap<i64, usize>>,
    /// Members dropped from the package (the calc chain), and whether a formula was overwritten.
    removed: BTreeSet<String>,
    formula_removed: bool,
    /// Per sheet, for `unshare`: the cells by element and the shared formulas by group.
    formula_index: HashMap<String, FormulaIndex>,
}

type FormulaIndex = (HashMap<usize, (i64, i64)>, HashMap<String, Vec<usize>>);

/// (id, type suffix, resolved path) of each internal relationship, in document order (a later duplicate id
/// replaces the value, as in a Python dict).
type Rels = Vec<(Option<String>, String, String)>;

/// One deferred shared-string edit: (sheet part, `c` element, text).
type Edit = (String, usize, String);

fn rel_by_id<'a>(rels: &'a Rels, id: &str) -> Option<&'a (Option<String>, String, String)> {
    rels.iter().find(|(i, _, _)| i.as_deref() == Some(id))
}

impl Package {
    fn open(entries: &[bilingua_zip::RawEntry]) -> Result<Package, Error> {
        let names: HashSet<String> = entries.iter().map(|e| e.name.clone()).collect();
        let strict = strict::is_strict(entries.iter().find(|e| e.name == "_rels/.rels").map(|e| e.data.as_slice()));
        let mut parts = HashMap::new();
        for e in entries.iter().filter(|e| !e.meta.dir) {
            let data = if strict && strict::is_xml_member(&e.name) {
                strict::part_to_transitional(&e.data).map_err(|_| bad(strict::REFUSAL))?
            } else {
                e.data.clone()
            };
            parts.insert(e.name.clone(), data);
        }
        let mut pkg = Package {
            names,
            parts,
            roots: HashMap::new(),
            dirty: BTreeSet::new(),
            added: Vec::new(),
            cells: HashMap::new(),
            shared: Vec::new(),
            shared_strings: None,
            sheets: Vec::new(),
            strict,
            workbook: String::new(),
            explicit: HashSet::new(),
            rows: HashMap::new(),
            removed: BTreeSet::new(),
            formula_removed: false,
            formula_index: HashMap::new(),
        };
        let wb = pkg.main_part()?;
        pkg.workbook = wb.clone();
        let wrels = pkg.rels(&wb)?;
        pkg.shared_strings = wrels.iter().find(|(_, k, _)| k == "sharedStrings").map(|(_, _, p)| p.clone());
        let d = &pkg.roots[&wb];
        let mut sheets = Vec::new();
        let mut visible = Vec::new();
        if let Some(list) = d.find(d.root, M, "sheets") {
            for sh in d.iter_tag(list, M, "sheet") {
                let Some(rid) = d.get_ns(sh, R, "id").filter(|r| !r.is_empty()) else { continue };
                // openpyxl looks the id up and fails on one the workbook does not have
                let Some((_, kind, path)) = rel_by_id(&wrels, rid) else {
                    return Err(bad("This XLSX file has invalid spreadsheet XML. Save it again in Excel."));
                };
                if !pkg.names.contains(path) {
                    continue;
                }
                visible.push(d.get(sh, "state").unwrap_or("visible") == "visible");
                if kind != "chartsheet" {
                    sheets.push(path.clone());
                }
            }
        }
        if !visible.iter().any(|v| *v) {
            return Err(bad("This XLSX file has no visible sheet this version can read. Open it in Excel, save it as an Excel Workbook (.xlsx) and upload it again."));
        }
        pkg.sheets = sheets;
        Ok(pkg)
    }

    /// Parses the part when it is not parsed yet; false when the package has no such member.
    fn load(&mut self, name: &str) -> Result<bool, Error> {
        if self.roots.contains_key(name) {
            return Ok(true);
        }
        let Some(raw) = self.parts.get(name) else { return Ok(false) };
        let text = decode_part(raw).ok_or_else(|| invalid(name))?;
        let d = dom::parse(&text).map_err(|_| invalid(name))?;
        self.roots.insert(name.to_string(), d);
        Ok(true)
    }

    /// A part already loaded.
    fn dom(&mut self, name: &str) -> &mut Dom {
        self.roots.get_mut(name).expect("the part is loaded")
    }

    fn rels(&mut self, part: &str) -> Result<Rels, Error> {
        let rn = rels_name(part);
        if !self.load(&rn)? {
            return Ok(Vec::new());
        }
        let d = &self.roots[&rn];
        let mut out: Rels = Vec::new();
        for rel in d.iter_tag(d.root, PKG_RELS, "Relationship") {
            if d.get(rel, "TargetMode").unwrap_or("").to_lowercase() == "external" {
                continue;
            }
            let Some(path) = resolve(part, d.get(rel, "Target").unwrap_or("")) else { continue };
            let id = d.get(rel, "Id").map(str::to_string);
            let kind = d.get(rel, "Type").unwrap_or("").rsplit('/').next().unwrap_or("").to_string();
            match out.iter_mut().find(|(i, _, _)| *i == id) {
                Some(slot) => *slot = (id, kind, path),
                None => out.push((id, kind, path)),
            }
        }
        Ok(out)
    }

    fn main_part(&mut self) -> Result<String, Error> {
        let mut rels = self.rels("")?;
        rels.sort();
        for (_, kind, path) in rels {
            if kind == "officeDocument" && self.names.contains(&path) && self.load(&path)? {
                let d = &self.roots[&path];
                if d.is(d.root, M, "workbook") {
                    return Ok(path);
                }
            }
        }
        Err(bad("This XLSX file is not an Excel workbook (no workbook part)."))
    }

    /// (row, column) to `c` element, addressed exactly as openpyxl reads the sheet (a later duplicate wins).
    fn ensure_index(&mut self, path: &str) -> Result<(), Error> {
        if self.cells.contains_key(path) {
            return Ok(());
        }
        if !self.load(path)? {
            return Err(invalid(path));
        }
        let d = &self.roots[path];
        let mut cells = BTreeMap::new();
        let mut row_n = 0i64;
        for row in d.iter_tag(d.root, M, "row") {
            row_n = match d.get(row, "r") {
                Some(r) => row_number(r, path)?,
                None => row_n + 1,
            };
            let mut col_n = 0i64;
            for c in d.element_children(row) {
                let key = match d.get(c, "r").filter(|r| !r.is_empty()) {
                    Some(r) => {
                        let k = coord(r, path)?;
                        col_n = k.1;
                        k
                    }
                    None => {
                        col_n += 1;
                        (row_n, col_n)
                    }
                };
                cells.insert(key, c);
            }
        }
        // openpyxl's `_clean_merge_range`: every cell of a merged range but its top-left one becomes a `MergedCell`
        // with no value, so a value Excel left under a merge is no unit (W4 xlsx-2, `01-stress002`)
        for mc in d.iter_tag(d.root, M, "mergeCell") {
            let Some((a, b)) = d.get(mc, "ref").and_then(|r| r.split_once(':')) else { continue };
            let (Ok((r1, c1)), Ok((r2, c2))) = (coord(a, path), coord(b, path)) else { continue };
            let (top, bottom, left, right) = (r1.min(r2), r1.max(r2), c1.min(c2), c1.max(c2));
            let covered: Vec<(i64, i64)> = cells.range((top, i64::MIN)..=(bottom, i64::MAX)).map(|(k, _)| *k).filter(|&(r, c)| c >= left && c <= right && (r, c) != (top, left)).collect();
            for k in covered {
                cells.remove(&k);
            }
        }
        self.cells.insert(path.to_string(), cells);
        Ok(())
    }

    /// The shared string table as openpyxl reads it (`read_string_table`).
    fn string_table(&mut self) -> Result<Vec<String>, Error> {
        let Some(path) = self.shared_strings.clone() else { return Ok(Vec::new()) };
        if !self.load(&path)? {
            return Ok(Vec::new());
        }
        let d = &self.roots[&path];
        Ok(d.iter_tag(d.root, M, "si").into_iter().map(|si| rich_content(d, si).replace("x005F_", "")).collect())
    }

    /// Every unit cell: (sheet index, row, column, value), in openpyxl's order (`_iter_cells`): a string value with
    /// a letter or digit that does not start with `=`.
    fn unit_cells(&mut self) -> Result<Vec<(usize, i64, i64, String)>, Error> {
        let sst = self.string_table()?;
        let mut out = Vec::new();
        for (n, path) in self.sheets.clone().iter().enumerate() {
            self.ensure_index(path)?;
            let d = &self.roots[path];
            for (&(row, col), &c) in &self.cells[path] {
                if let Some(v) = cell_value(d, c, &sst, path)? {
                    // LocOS `_iter_cells`: a string that starts with `=` is a formula look-alike, never a unit,
                    // even a shared or inline string (`= Mandatory fields`)
                    if !v.starts_with('=') && bilingua_model::py_has_alnum(&v) {
                        out.push((n, row, col, v));
                    }
                }
            }
        }
        Ok(out)
    }

    /// (sheet index, drawing part, `a:p`) of every text-bearing paragraph in the worksheets' drawings, in sheet
    /// order then XML order, `mc:Fallback` copies skipped (`drawing_paragraphs`).
    fn drawing_paragraphs(&mut self) -> Result<Vec<(usize, String, usize)>, Error> {
        let mut out = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for (n, path) in self.sheets.clone().iter().enumerate() {
            self.load(path)?;
            let drawings: Vec<Option<String>> = {
                let d = &self.roots[path];
                d.iter_tag(d.root, M, "drawing").into_iter().map(|x| d.get_ns(x, R, "id").map(str::to_string)).collect()
            };
            if drawings.is_empty() {
                continue;
            }
            let srels = self.rels(path)?;
            for rid in drawings {
                let Some((_, kind, dpath)) = rid.and_then(|r| rel_by_id(&srels, &r).cloned()) else { continue };
                if kind != "drawing" || seen.contains(&dpath) || !self.names.contains(&dpath) {
                    continue;
                }
                seen.insert(dpath.clone());
                if !self.load(&dpath)? {
                    continue;
                }
                let d = &self.roots[&dpath];
                for p in d.iter_tag(d.root, A, "p") {
                    if d.has_ancestor(p, |x| d.is(x, MC, "Fallback")) {
                        continue;
                    }
                    if has_text(d, p) {
                        out.push((n, dpath.clone(), p));
                    }
                }
            }
        }
        Ok(out)
    }

    /// Writes `text` into the cell as a literal string (`set_cell_text`).
    fn set_cell_text(&mut self, sheet: usize, row: i64, col: i64, text: &str) -> Result<(), Error> {
        self.set_cell_text_with(sheet, row, col, text, false)
    }

    /// `set_cell_text(.., create)`: with `create` a cell that is not there is made (column mode's target cells).
    fn set_cell_text_with(&mut self, sheet: usize, row: i64, col: i64, text: &str, create: bool) -> Result<(), Error> {
        let path = self.sheets[sheet].clone();
        self.ensure_index(&path)?;
        let found = self.cells[&path].get(&(row, col)).copied();
        let c = match found {
            Some(c) => c,
            None if create => self.create_cell(&path, row, col)?,
            None => return Err(bad(format!("Excel cell {}{row} was not found.", col_letters(col)))),
        };
        self.dirty.insert(path.clone());
        if let Some(f) = self.roots[&path].find(c, M, "f") {
            // a formula overwritten by text (a column-mode target cell): see `unshare`
            self.unshare(&path, c, f)?;
            self.formula_removed = true;
        }
        let d = self.roots.get_mut(&path).expect("sheet part");
        let digits = d.find(c, M, "v").and_then(|v| d.text(v)).map(|t| bilingua_model::py_strip(&t).to_string()).filter(|t| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit()));
        if d.get(c, "t") == Some("s") && d.find(c, M, "f").is_none() {
            if let Some(i) = digits.and_then(|t| t.parse::<usize>().ok()) {
                self.shared.push((path, c, i, text.to_string()));
                return Ok(());
            }
        }
        let template = if d.get(c, "t") == Some("inlineStr") { d.find(c, M, "is") } else { None };
        let new = string_item(d, c, "is", text, template);
        for child in d.element_children(c) {
            if d.is(child, M, "f") || d.is(child, M, "v") || d.is(child, M, "is") {
                d.remove(child);
            }
        }
        d.remove_attr(c, "cm");
        d.remove_attr(c, "vm");
        d.set(c, "t", "inlineStr");
        match d.find(c, M, "extLst") {
            Some(ext) => d.add_previous(ext, &[new]),
            None => d.append(c, &[new]),
        }
        Ok(())
    }

    fn resolve_shared(&mut self) -> Result<(), Error> {
        if self.shared.is_empty() {
            return Ok(());
        }
        for path in self.sheets.clone() {
            self.load(&path)?; // column mode reads only the sheets it plans
        }
        let mut refs: HashMap<u64, usize> = HashMap::new();
        for path in &self.sheets {
            let d = &self.roots[path];
            for c in d.iter_tag(d.root, M, "c") {
                if d.get(c, "t") == Some("s") {
                    let v = d.findtext(c, M, "v").unwrap_or_default();
                    let v = bilingua_model::py_strip(&v);
                    if !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()) {
                        if let Ok(i) = v.parse::<u64>() {
                            *refs.entry(i).or_default() += 1;
                        }
                    }
                }
            }
        }
        // edits grouped by string index, in first-edit order (a Python defaultdict)
        let mut by_index: Vec<(usize, Vec<Edit>)> = Vec::new();
        let mut slot: HashMap<usize, usize> = HashMap::new();
        for (path, c, i, text) in std::mem::take(&mut self.shared) {
            match slot.get(&i) {
                Some(&k) => by_index[k].1.push((path, c, text)),
                None => {
                    slot.insert(i, by_index.len());
                    by_index.push((i, vec![(path, c, text)]));
                }
            }
        }
        let no_table = || bad("This Excel file has no shared string table.");
        let sst_path = self.shared_strings.clone().ok_or_else(no_table)?;
        if !self.load(&sst_path)? {
            return Err(no_table());
        }
        let mut repoint: Vec<(String, usize, usize)> = Vec::new();
        {
            let sst = self.roots.get_mut(&sst_path).expect("shared strings");
            let root = sst.root;
            let items = sst.findall(root, M, "si");
            let ext = sst.find(root, M, "extLst");
            let mut appended: HashMap<(usize, String), usize> = HashMap::new();
            for (i, edits) in &by_index {
                let Some(&item) = items.get(*i) else {
                    return Err(bad(format!("Shared string {i} is missing from this Excel file.")));
                };
                let texts: BTreeSet<&str> = edits.iter().map(|(_, _, t)| t.as_str()).collect();
                if texts.len() == 1 && edits.len() >= refs.get(&(*i as u64)).copied().unwrap_or(0) {
                    let new = string_item(sst, item, "si", &edits[0].2, Some(item));
                    sst.clear_elements(item);
                    let kids = std::mem::take(&mut sst.nodes[new].children);
                    sst.append(item, &kids);
                    continue;
                }
                for (path, c, text) in edits {
                    let key = (*i, text.clone());
                    let at = match appended.get(&key) {
                        Some(n) => *n,
                        None => {
                            let new = string_item(sst, root, "si", text, Some(item));
                            match ext {
                                Some(e) => sst.add_previous(e, &[new]),
                                None => sst.append(root, &[new]),
                            }
                            let n = items.len() + appended.len();
                            appended.insert(key, n);
                            n
                        }
                    };
                    repoint.push((path.clone(), *c, at));
                }
            }
            if !appended.is_empty() && sst.get(root, "uniqueCount").is_some() {
                sst.set(root, "uniqueCount", &(items.len() + appended.len()).to_string());
            }
        }
        for (path, c, at) in repoint {
            let d = self.dom(&path);
            if let Some(v) = d.find(c, M, "v") {
                d.set_text(v, &at.to_string());
            }
        }
        self.dirty.insert(sst_path);
        Ok(())
    }

    /// Every worksheet view right to left (`set_sheets_rtl`).
    fn set_sheets_rtl(&mut self) -> Result<(), Error> {
        for path in self.sheets.clone() {
            self.load(&path)?;
            let d = self.roots.get_mut(&path).expect("sheet part");
            let root = d.root;
            let mut views = d.iter_tag(root, M, "sheetView");
            if views.is_empty() {
                let bx = d.new_element(root, M, "sheetViews");
                let view = d.new_element(bx, M, "sheetView");
                d.set(view, "workbookViewId", "0");
                d.append(bx, &[view]);
                let after: Vec<usize> = d.element_children(root).into_iter().filter(|&c| d.is(c, M, "sheetPr") || d.is(c, M, "dimension")).collect();
                match after.last() {
                    Some(&a) => d.add_next(a, &[bx]),
                    None => d.insert_first(root, &[bx]),
                }
                views = vec![view];
            }
            let mut changed = false;
            for view in views {
                if !matches!(d.get(view, "rightToLeft"), Some("1") | Some("true")) {
                    d.set(view, "rightToLeft", "1");
                    changed = true;
                }
            }
            if changed {
                self.dirty.insert(path);
            }
        }
        Ok(())
    }

    /// `dc:language` in `docProps/core.xml`, the part made when the package has none (`stamp_language`).
    fn stamp_language(&mut self, tag: &str) -> Result<(), Error> {
        let core = self.rels("")?.into_iter().find(|(_, k, _)| k == "core-properties").map(|(_, _, p)| p);
        let core = match core {
            Some(c) if self.load(&c)? => c,
            _ => self.add_core()?,
        };
        let d = self.dom(&core);
        let root = d.root;
        let lang = match d.find(root, DC, "language") {
            Some(l) => l,
            None => {
                let l = d.new_element(root, DC, "language");
                d.append(root, &[l]);
                l
            }
        };
        d.set_text(lang, tag);
        self.dirty.insert(core);
        Ok(())
    }

    /// A new core properties part with its package relationship and content type.
    fn add_core(&mut self) -> Result<String, Error> {
        let mut name = "docProps/core.xml".to_string();
        let mut n = 1;
        while self.names.contains(&name) || self.added.contains(&name) {
            n += 1;
            name = format!("docProps/core{n}.xml");
        }
        // LocOS fails here on a package with no `_rels/.rels` (it reads the core part through it)
        if !self.load("_rels/.rels")? {
            return Err(bad("This XLSX file has no package relationships."));
        }
        self.roots.insert(name.clone(), Dom::with_root(CP, "coreProperties", &[("cp", CP), ("dc", DC)]));
        self.added.push(name.clone());
        let rels = self.dom("_rels/.rels");
        let root = rels.root;
        let ids: HashSet<String> = rels.iter_tag(root, PKG_RELS, "Relationship").into_iter().filter_map(|r| rels.get(r, "Id").map(str::to_string)).collect();
        let rid = (1..10_000).map(|k| format!("rIdCore{k}")).find(|r| !ids.contains(r)).ok_or_else(|| bad("Too many relationships."))?;
        let rel = rels.new_element(root, PKG_RELS, "Relationship");
        rels.set(rel, "Id", &rid);
        rels.set(rel, "Type", CORE_REL);
        rels.set(rel, "Target", &name);
        rels.append(root, &[rel]);
        self.dirty.insert("_rels/.rels".into());
        if self.load("[Content_Types].xml")? {
            let types = self.dom("[Content_Types].xml");
            let root = types.root;
            let o = types.new_element(root, CT, "Override");
            types.set(o, "PartName", &format!("/{name}"));
            types.set(o, "ContentType", CORE_TYPE);
            types.append(root, &[o]);
            self.dirty.insert("[Content_Types].xml".into());
        }
        Ok(name)
    }

    /// The package with the changed parts re-serialised; every other member keeps its bytes and order (`save`).
    fn save(&mut self, entries: &[bilingua_zip::RawEntry]) -> Result<Vec<u8>, Error> {
        use bilingua_zip::{Body, EntryMeta};
        self.resolve_shared()?;
        if self.formula_removed {
            self.drop_calc_chain()?;
        }
        let mut changed: HashMap<String, Vec<u8>> = HashMap::new();
        for name in &self.dirty {
            let standalone = self.parts.get(name).is_some_and(|raw| strict::contains(&raw[..raw.len().min(200)], b"standalone=\"yes\""));
            let d = self.roots.get(name).ok_or_else(|| bad(format!("{name} is missing")))?;
            let decl = if standalone { "<?xml version='1.0' encoding='UTF-8' standalone='yes'?>\n" } else { "<?xml version='1.0' encoding='UTF-8'?>\n" };
            let mut bytes = format!("{decl}{}", d.serialize()).into_bytes();
            if self.strict && strict::is_xml_member(name) {
                bytes = strict::part_to_strict(&bytes);
            }
            changed.insert(name.clone(), bytes);
        }
        let mut out: Vec<(EntryMeta, Body)> = Vec::with_capacity(entries.len() + self.added.len());
        for e in entries {
            if self.removed.contains(&e.name) {
                continue;
            }
            match changed.remove(&e.name) {
                Some(b) => {
                    let mut m = e.meta.clone();
                    m.stored = false;
                    out.push((m, Body::Owned(b)));
                }
                None => match &e.packed {
                    Some((p, raw)) => out.push((e.meta.clone(), Body::Packed(p, raw))),
                    None => out.push((e.meta.clone(), Body::Plain(&e.data))),
                },
            }
        }
        for name in &self.added {
            let b = changed.remove(name).unwrap_or_default();
            out.push((EntryMeta { name: name.clone(), stored: false, dir: false, modified: None }, Body::Owned(b)));
        }
        bilingua_zip::write_zip_parts(&out)
    }
}

fn col_letters(mut n: i64) -> String {
    let mut out = String::new();
    while n > 0 {
        let rem = (n - 1) % 26;
        n = (n - 1) / 26;
        out.insert(0, (b'A' + rem as u8) as char);
    }
    out
}

/// openpyxl's `Text.content` of an `si` or `is`: the last `t` child, then the last `t` of each `r` child (any
/// namespace: openpyxl matches local names); phonetic runs left out.
fn rich_content(d: &Dom, item: usize) -> String {
    let mut plain: Option<String> = None;
    let mut runs = Vec::new();
    for c in d.element_children(item) {
        match d.local(c) {
            "t" => plain = d.text(c),
            "r" => {
                let mut t = None;
                for x in d.element_children(c) {
                    if d.local(x) == "t" {
                        t = d.text(x);
                    }
                }
                if let Some(t) = t {
                    runs.push(t);
                }
            }
            _ => {}
        }
    }
    let mut s = plain.unwrap_or_default();
    for r in runs {
        s.push_str(&r);
    }
    s
}

/// The sign and digits of a Python `int()` literal (white space around, an optional sign, `_` between digits,
/// any Unicode decimal digit), `None` when `int()` refuses it.
fn py_int_parts(s: &str) -> Option<(bool, String)> {
    let t = bilingua_model::py_strip(s);
    let (neg, body) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    if body.is_empty() || body.starts_with('_') || body.ends_with('_') || body.contains("__") || !body.chars().all(|c| c == '_' || bilingua_model::py_isdecimal_char(c)) {
        return None;
    }
    Some((neg, body.replace('_', "")))
}

/// Python `int()` of a string, for the ASCII digits that fit an `i64`.
fn py_int(s: &str) -> Option<i64> {
    let (neg, digits) = py_int_parts(s)?;
    let n = digits.parse::<i64>().ok()?;
    Some(if neg { -n } else { n })
}

/// Python `float()` of a string, as a check only.
fn py_float_ok(s: &str) -> bool {
    let t = bilingua_model::py_strip(s).replace('_', "");
    let l = t.trim_start_matches(['+', '-']).to_ascii_lowercase();
    matches!(l.as_str(), "inf" | "infinity" | "nan") || t.parse::<f64>().is_ok()
}

/// The value openpyxl reads for a cell when it is a string that is not a formula (`parse_cell`); an error for a
/// value openpyxl cannot read (it refuses the workbook).
fn cell_value(d: &Dom, c: usize, sst: &[String], name: &str) -> Result<Option<String>, Error> {
    let t = d.get(c, "t").unwrap_or("n");
    if d.find(c, M, "f").is_some() {
        return Ok(None); // a formula: "=..." or an array / data table formula, never a unit
    }
    if t == "inlineStr" {
        return Ok(d.find(c, M, "is").map(|is| rich_content(d, is)));
    }
    let Some(v) = d.findtext(c, M, "v").filter(|v| !v.is_empty()) else { return Ok(None) };
    match t {
        "n" => {
            let ok = if v.contains(['.', 'E', 'e']) { py_float_ok(&v) } else { py_int_parts(&v).is_some() };
            if !ok {
                return Err(invalid(name));
            }
            Ok(None)
        }
        "s" => {
            // Python indexing: a negative index counts from the end (a LocOS / openpyxl quirk kept on purpose)
            let i = py_int(&v).ok_or_else(|| invalid(name))?;
            let at = if i < 0 { sst.len() as i64 + i } else { i };
            if at < 0 || at as usize >= sst.len() {
                return Err(invalid(name));
            }
            Ok(Some(sst[at as usize].clone()))
        }
        "b" => {
            py_int_parts(&v).ok_or_else(|| invalid(name))?;
            Ok(None)
        }
        "d" => Ok(None),
        // "str", an error value ("#N/A") and any type openpyxl does not know: the text as it is
        _ => Ok(Some(v)),
    }
}

fn py_strip_ne(s: &str) -> bool {
    !bilingua_model::py_strip(s).is_empty()
}

/// True when a DrawingML `a:p` has a run with visible text (`has_text`).
fn has_text(d: &Dom, p: usize) -> bool {
    d.iter_tag(p, A, "r").into_iter().any(|r| d.findtext(r, A, "t").is_some_and(|t| py_strip_ne(&t)))
}

/// LocOS's `_needs_preserve`.
fn needs_preserve(text: &str) -> bool {
    text != bilingua_model::py_strip(text) || text.contains(['\n', '\r', '\t']) || text.contains("  ")
}

/// A new `si` / `is` element holding `text` for a place under `at`, formatted from `template` (`_string_item`):
/// one run with the shared `rPr` when every text run of the template has the same `rPr`, else a plain `t`; the
/// template's `phoneticPr` kept.
fn string_item(d: &mut Dom, at: usize, tag: &str, text: &str, template: Option<usize>) -> usize {
    let item = d.new_element(at, M, tag);
    let mut rpr = None;
    if let Some(tpl) = template {
        let text_runs: Vec<usize> = d.findall(tpl, M, "r").into_iter().filter(|&r| d.findtext(r, M, "t").is_some_and(|t| !t.is_empty())).collect();
        let rprs: Vec<Option<usize>> = text_runs.iter().map(|&r| d.find(r, M, "rPr")).collect();
        let shapes: BTreeSet<String> = rprs.iter().map(|x| x.map(|x| d.serialize_with_tail(x)).unwrap_or_default()).collect();
        if !rprs.is_empty() && shapes.len() == 1 {
            rpr = rprs[0];
        }
    }
    let t = match rpr {
        Some(rpr) => {
            let run = d.new_element(item, M, "r");
            d.append(item, &[run]);
            let copy = d.copy_with_tail(rpr);
            d.append(run, &copy);
            let t = d.new_element(run, M, "t");
            d.append(run, &[t]);
            t
        }
        None => {
            let t = d.new_element(item, M, "t");
            d.append(item, &[t]);
            t
        }
    };
    d.set_text(t, text);
    if needs_preserve(text) {
        d.set(t, "xml:space", "preserve");
    }
    if let Some(ph) = template.and_then(|tpl| d.find(tpl, M, "phoneticPr")) {
        let copy = d.copy_with_tail(ph);
        d.append(item, &copy);
    }
    item
}

// ------------------------------------------------------------------------------------------- drawing text

const OPEN: char = '\u{27E6}';
const CLOSE: char = '\u{27E7}';

/// The content children of an `a:p`: (is a run, element).
fn items(d: &Dom, p: usize) -> Vec<(bool, usize)> {
    d.element_children(p).into_iter().filter_map(|c| if d.is(c, A, "r") { Some((true, c)) } else if d.is(c, A, "br") || d.is(c, A, "fld") { Some((false, c)) } else { None }).collect()
}

fn run_text(d: &Dom, r: usize) -> String {
    d.find(r, A, "t").and_then(|t| d.text(t)).unwrap_or_default()
}

/// The unit for one paragraph (`pptx_filter.tokenize`): plain text for one run and nothing else; otherwise run `n`
/// is the pair with id `n` (1..runs) and break or field `m` the point with id `m` (numbered after the runs).
fn paragraph_inlines(d: &Dom, p: usize) -> (Vec<Inline>, BTreeMap<String, CodeData>) {
    let items = items(d, p);
    let runs = items.iter().filter(|(r, _)| *r).count();
    let mut codes = BTreeMap::new();
    if items.len() == 1 && runs == 1 {
        let t = run_text(d, items[0].1);
        return (if t.is_empty() { vec![] } else { vec![Inline::text(t)] }, codes);
    }
    let mut out = Vec::new();
    let (mut rn, mut xn) = (0usize, runs);
    for (is_run, el) in items {
        if is_run {
            rn += 1;
            let id = rn.to_string();
            out.push(Inline::tag(id.clone(), TagKind::Open, "<a:r>"));
            let t = run_text(d, el);
            if !t.is_empty() {
                out.push(Inline::text(t));
            }
            out.push(Inline::tag(id.clone(), TagKind::Close, "</a:r>"));
            codes.insert(id, CodeData::paired("<a:r>", "</a:r>").with_label("run"));
        } else {
            xn += 1;
            let id = xn.to_string();
            let label = if d.is(el, A, "br") { "line break" } else { "field" };
            let markup = d.serialize_with_tail(el);
            out.push(Inline::tag(id.clone(), TagKind::Standalone, if d.is(el, A, "br") { "<a:br/>" } else { "<a:fld/>" }));
            codes.insert(id, CodeData::standalone(markup).with_label(label));
        }
    }
    (out, codes)
}

/// (`Some(run index)` or `None`, text) and point spans of a translation (`pptx_filter._spans`).
enum Span {
    Text(Option<usize>, String),
    Point(i64),
}

/// LocOS's token grammar `⟦/?[a-z]?\d+⟧` at the start of `s`: (closing, kind, number text, byte length).
fn token_at(s: &str) -> Option<(bool, Option<char>, &str, usize)> {
    let rest = s.strip_prefix(OPEN)?;
    let (closing, rest) = match rest.strip_prefix('/') {
        Some(r) => (true, r),
        None => (false, rest),
    };
    let (kind, rest) = match rest.chars().next() {
        Some(c) if c.is_ascii_lowercase() => (Some(c), &rest[1..]),
        _ => (None, rest),
    };
    // Python's `\d` is any Unicode decimal digit
    let digits: usize = rest.chars().take_while(|&c| bilingua_model::py_isdecimal_char(c)).map(char::len_utf8).sum();
    if digits == 0 || !rest[digits..].starts_with(CLOSE) {
        return None;
    }
    let len = s.len() - rest.len() + digits + CLOSE.len_utf8();
    Some((closing, kind, &rest[..digits], len))
}

/// Literal text that looks like a LocOS token (`⟦x2⟧` written by the author or the translator) must not be read as
/// one when a translation is written: literal text is escaped on its way to the writer and unescaped where the
/// spans are cut, so only real codes are tokens (the shared helper in `bilingua_model::profile::tokens`).
pub(crate) use bilingua_model::profile::tokens::{escape_literal as esc_lit, unescape_literal as unesc_lit};

fn spans(text: &str, n_runs: usize) -> Vec<Span> {
    let mut out = Vec::new();
    let mut cur: Option<usize> = None;
    let mut buf = String::new();
    let mut i = 0;
    let flush = |out: &mut Vec<Span>, buf: &mut String, cur: Option<usize>| {
        if !buf.is_empty() {
            out.push(Span::Text(cur, unesc_lit(&std::mem::take(buf))));
        }
    };
    while i < text.len() {
        let rest = &text[i..];
        if let Some((closing, kind, num, len)) = token_at(rest) {
            let n = num.parse::<u64>().ok();
            match (kind, n) {
                (Some('r'), Some(n)) if (1..=n_runs as u64).contains(&n) => {
                    flush(&mut out, &mut buf, cur);
                    cur = if closing { None } else { Some(n as usize - 1) };
                }
                (Some('x'), _) if !closing => {
                    // a number past i64 (or in other digits) is no point of this paragraph: dropped, as in LocOS
                    flush(&mut out, &mut buf, cur);
                    let i = n.and_then(|n| i64::try_from(n).ok()).map(|n| n - n_runs as i64 - 1).unwrap_or(i64::MAX);
                    out.push(Span::Point(i));
                }
                _ => {
                    // an unknown token stays as literal text, in its own span as in LocOS
                    flush(&mut out, &mut buf, cur);
                    out.push(Span::Text(cur, rest[..len].to_string()));
                }
            }
            i += len;
            continue;
        }
        let ch = rest.chars().next().unwrap();
        buf.push(ch);
        i += ch.len_utf8();
    }
    flush(&mut out, &mut buf, cur);
    out
}

/// Replaces the content of one `a:p` with `text` (`pptx_filter.write_paragraph`).
fn write_paragraph(d: &mut Dom, p: usize, text: &str) {
    let items = items(d, p);
    let runs: Vec<usize> = items.iter().filter(|(r, _)| *r).map(|(_, e)| *e).collect();
    let points: Vec<usize> = items.iter().filter(|(r, _)| !*r).map(|(_, e)| *e).collect();
    let rprs: Vec<Option<usize>> = runs.iter().map(|&r| d.find(r, A, "rPr")).collect();
    let fallback = rprs.first().copied().flatten();
    let mut used: BTreeSet<usize> = BTreeSet::new();
    let mut new: Vec<Vec<usize>> = Vec::new();
    let new_run = |d: &mut Dom, line: &str, rpr: Option<usize>| -> Vec<usize> {
        let r = d.new_element(p, A, "r");
        if let Some(x) = rpr {
            let copy = d.copy_with_tail(x);
            d.append(r, &copy);
        }
        let t = d.new_element(r, A, "t");
        d.append(r, &[t]);
        d.set_text(t, line);
        vec![r]
    };
    for span in spans(text, runs.len()) {
        match span {
            Span::Point(i) => {
                if i >= 0 && (i as usize) < points.len() && !used.contains(&(i as usize)) {
                    used.insert(i as usize);
                    new.push(d.copy_with_tail(points[i as usize]));
                }
            }
            Span::Text(ri, s) => {
                if s.is_empty() {
                    continue;
                }
                let rpr = match ri {
                    Some(i) => rprs[i],
                    None => fallback,
                };
                let s = s.replace("\r\n", "\n").replace('\r', "\n");
                for (j, line) in s.split('\n').enumerate() {
                    if j > 0 {
                        let br = d.new_element(p, A, "br");
                        if let Some(x) = rpr {
                            let copy = d.copy_with_tail(x);
                            d.append(br, &copy);
                        }
                        new.push(vec![br]);
                    }
                    if !line.is_empty() {
                        new.push(new_run(d, line, rpr));
                    }
                }
            }
        }
    }
    for (i, &el) in points.iter().enumerate() {
        if !used.contains(&i) {
            new.push(d.copy_with_tail(el)); // a dropped break or field is kept, never deleted
        }
    }
    if !new.iter().any(|g| d.is(g[0], A, "r")) {
        let r = new_run(d, "", fallback);
        new.insert(0, r);
    }
    for (_, el) in &items {
        d.remove(*el);
    }
    let end = d.find(p, A, "endParaRPr");
    for g in new {
        match end {
            Some(e) => d.add_previous(e, &g),
            None => d.append(p, &g),
        }
    }
}

/// The target language on the runs of one paragraph, right to left when `rtl` (`pptx_filter._stamp_language`).
fn stamp_paragraph(d: &mut Dom, p: usize, tag: &str, rtl: bool) {
    for r in d.element_children(p) {
        if d.is(r, A, "r") || d.is(r, A, "fld") || d.is(r, A, "br") {
            let rpr = match d.find(r, A, "rPr") {
                Some(x) => x,
                None => {
                    let x = d.new_element(r, A, "rPr");
                    d.insert_first(r, &[x]);
                    x
                }
            };
            d.set(rpr, "lang", tag);
        }
    }
    if let Some(end) = d.find(p, A, "endParaRPr") {
        d.set(end, "lang", tag);
    }
    if rtl {
        let ppr = match d.find(p, A, "pPr") {
            Some(x) => x,
            None => {
                let x = d.new_element(p, A, "pPr");
                d.insert_first(p, &[x]);
                x
            }
        };
        d.set(ppr, "rtl", "1");
    }
}

// ------------------------------------------------------------------------------------------- filter

fn read(bytes: &[u8], l: &bilingua_model::Limits) -> Result<Vec<bilingua_zip::RawEntry>, Error> {
    let entries = crate::zipio::read_zip(bytes, &bilingua_zip::ZipLimits::from(l))?;
    bilingua_zip::check_xml_parts(&entries, l)?;
    Ok(entries)
}

/// Extracts a workbook the way LocOS parses it: cell mode (walk 2, comments when `xo.translate_comments`) or, with
/// a column mapping, column mode.
pub fn extract(bytes: &[u8], opts: &ExtractOptions, xo: &crate::XlsxOptions) -> Result<Document, Error> {
    if table_mode(xo) {
        return table::extract(bytes, opts, xo);
    }
    let l = opts.limits();
    guard::check_input(bytes, &l)?;
    let entries = read(bytes, &l)?;
    let mut pkg = Package::open(&entries)?;
    let cells = pkg.unit_cells()?;
    guard::check_unit_count(cells.len(), &l)?;
    let mut units = Vec::with_capacity(cells.len());
    for (idx, (sheet, row, col, value)) in cells.into_iter().enumerate() {
        let mut u = Unit::new(idx.to_string(), vec![Inline::text(value)]);
        u.context = UnitContext::new().with_kind("cell").with_key(format!("{}{row}", col_letters(col))).with_location(pkg.sheets[sheet].clone());
        u.context.extra.insert(profile::P_INDEX.to_string(), idx.to_string());
        u.hidden.insert(KEY.to_string(), idx.to_string());
        units.push(u);
    }
    let base = units.len();
    let paras = pkg.drawing_paragraphs()?;
    guard::check_unit_count(base + paras.len(), &l)?;
    for (j, (_, part, p)) in paras.into_iter().enumerate() {
        pkg.load(&part)?;
        let (inl, codes) = paragraph_inlines(pkg.dom(&part), p);
        let idx = base + j;
        let mut u = Unit::new(idx.to_string(), inl);
        u.codes = codes;
        u.context = UnitContext::new().with_kind("text box").with_location(part.clone());
        u.context.extra.insert(profile::P_INDEX.to_string(), idx.to_string());
        u.context.extra.insert(profile::STYLE.to_string(), TEXT_BOX.to_string());
        u.hidden.insert(KEY.to_string(), idx.to_string());
        units.push(u);
    }
    if xo.translate_comments {
        // #690: the comment units, appended at COMMENT_BASE + their place in the comment walk
        let found = comments::units(&mut pkg)?;
        guard::check_unit_count(units.len() + found.len(), &l)?;
        units.extend(found);
    }
    let doc = Document::new("xlsx", units, Skeleton::binary(bytes.to_vec(), parser_info!()))
        .with_source_lang(opts.source_lang.clone())
        .with_target_lang(opts.target_lang.clone())
        .with_granularity(Granularity::Paragraph);
    guard::finish(doc, vec![], &l)
}

/// LocOS's `xml_target_text`: vertical tab and form feed become new lines, characters XML 1.0 cannot hold go.
fn xml_target_text(text: &str) -> String {
    text.chars()
        .map(|c| if c == '\u{0b}' || c == '\u{0c}' { '\n' } else { c })
        .filter(|&c| !matches!(c, '\u{00}'..='\u{08}' | '\u{0e}'..='\u{1f}' | '\u{fffe}' | '\u{ffff}'))
        .collect()
}

/// The LocOS text of a translation: run pairs are `r` tokens and points `x` tokens, numbered by tag id.
fn target_text(target: &[Inline], esc: bool) -> String {
    let mut s = String::new();
    for i in target {
        match i {
            Inline::Text(t) if esc => s.push_str(&esc_lit(t)),
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { id, kind, .. } => s.push_str(&match kind {
                TagKind::Open => format!("{OPEN}r{id}{CLOSE}"),
                TagKind::Close => format!("{OPEN}/r{id}{CLOSE}"),
                TagKind::Standalone => format!("{OPEN}x{id}{CLOSE}"),
            }),
        }
    }
    s
}

/// The translations by LocOS number: a unit counts when every segment has a target (the segments joined with
/// their separators).
/// With `esc`, literal text is escaped for the paragraph writer (`spans` unescapes it); cells and comments take the
/// plain text.
fn written_texts(doc: &Document, esc: bool) -> BTreeMap<usize, String> {
    let mut out = BTreeMap::new();
    for u in doc.units.iter().filter(|u| u.translate) {
        let Some(k) = u.hidden(KEY).and_then(|k| k.parse::<usize>().ok()) else { continue };
        if u.segments.is_empty() || !u.segments.iter().all(|s| s.has_target()) {
            continue;
        }
        let mut inl: Vec<Inline> = Vec::new();
        for (i, s) in u.segments.iter().enumerate() {
            if i > 0 {
                inl.push(Inline::text(u.separators.get(i - 1).cloned().unwrap_or_default()));
            }
            inl.extend(s.target.iter().cloned());
        }
        out.insert(k, target_text(&inl, esc));
    }
    out
}

/// Writes a document extracted under the profile (`XlsxFilter._rebuild`): the original package when nothing
/// changes.
pub fn merge(doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
    let Skeleton::Binary { data, .. } = &doc.skeleton else {
        return Err(Error::Write("xlsx: a LocOS-profile document needs its original file".into()));
    };
    let texts = written_texts(doc, false);
    let box_texts = written_texts(doc, true);
    let table = table::is_table(doc);
    let tag = opts.stamp_lang(doc, !texts.is_empty()).and_then(profile::lang_tag);
    if texts.is_empty() && tag.is_none() {
        return Ok(data.clone());
    }
    let l = bilingua_model::Limits::default();
    let entries = read(data, &l)?;
    let mut pkg = Package::open(&entries)?;
    let rtl = tag.as_deref().is_some_and(profile::is_rtl);
    if table {
        table::apply(&mut pkg, doc, &texts)?;
    } else {
        let cells = pkg.unit_cells()?;
        for (idx, (sheet, row, col, value)) in cells.iter().enumerate() {
            let Some(tr) = texts.get(&idx) else { continue };
            let tr = xml_target_text(tr);
            if tr != *value {
                pkg.set_cell_text(*sheet, *row, *col, &tr)?;
            }
        }
        let base = cells.len();
        for (j, (_, part, p)) in pkg.drawing_paragraphs()?.into_iter().enumerate() {
            let Some(tr) = box_texts.get(&(base + j)) else { continue };
            let tr = xml_target_text(tr);
            pkg.load(&part)?;
            let d = pkg.dom(&part);
            write_paragraph(d, p, &tr);
            if let Some(t) = &tag {
                stamp_paragraph(d, p, t, rtl);
            }
            pkg.dirty.insert(part);
        }
        // comment units (keys from COMMENT_BASE up): the original's comments are patched in place
        comments::apply(&mut pkg, &texts)?;
    }
    if let Some(t) = &tag {
        pkg.stamp_language(t)?;
        if rtl {
            pkg.set_sheets_rtl()?;
        }
    }
    if pkg.dirty.is_empty() && pkg.shared.is_empty() {
        return Ok(data.clone());
    }
    pkg.save(&entries)
}

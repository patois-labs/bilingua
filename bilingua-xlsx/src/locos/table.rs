//! Column mode under the Localization OS engine (`columns=` / `xlsx.table`; LocOS's `_parse_table` and
//! `_rebuild_table`).
//!
//! With a column mapping each worksheet is planned from its own first non-empty row (header detection and the
//! column roles are the CSV filter's: `bilingua_csv::plan`). The source column's text cells become units, sheet by
//! sheet, row by row; the translation is written into the target column of the same row (cells and rows are made
//! where there are none, in address order), or over the source cell when the plan has no target column for the
//! target language. A target someone already filled is kept when the translation equals the source. A sheet the
//! mapping does not fit is left alone; a mapping that fits no sheet is refused.
//!
//! A unit remembers its source cell and its sheet's target columns (`Unit::hidden`), so the merge needs no new
//! plan. Text boxes and comments are not units in column mode.

use super::*;
use super::{formula, values};
use bilingua_csv::plan::{self, Plan};

/// `Unit::hidden` key: `sheet,row,col` of the source cell.
const XC: &str = "xc";
/// `Unit::hidden` key: the sheet's target columns, `locale=column;..` (0-based), `*` for the one every target uses.
const XT: &str = "xt";
/// `Unit::hidden` key: `1` when the document was read under the LocOS profile.
const XL: &str = "xl";

/// Whether these options ask for column mode (a mapping, even a blank one, as in LocOS).
pub fn table_mode(xo: &crate::XlsxOptions) -> bool {
    xo.columns.as_deref().is_some_and(|s| !s.is_empty())
}

/// Whether the document was read in column mode.
pub(super) fn is_table(doc: &Document) -> bool {
    doc.units.iter().any(|u| u.hidden(XC).is_some())
}

/// The merge options `merge_with_report` uses: the profile's for everything but a column-mode document read without
/// the profile.
pub fn report_options(doc: &Document) -> MergeOptions {
    let plain = is_table(doc) && !doc.units.iter().any(|u| u.hidden(XL) == Some("1"));
    if plain {
        MergeOptions::default()
    } else {
        profile::locos().merge_options("xlsx")
    }
}

/// The sheet's target columns, `locale=column` pairs; a locale is percent-escaped so `;`, `=` and `%` in it survive.
fn encode_targets(p: &Plan) -> String {
    let esc = |l: &str| l.replace('%', "%25").replace(';', "%3B").replace('=', "%3D");
    p.targets.iter().map(|(l, c)| format!("{}={c}", esc(l))).collect::<Vec<_>>().join(";")
}

fn decode_targets(s: &str) -> Vec<(String, usize)> {
    let unesc = |l: &str| l.replace("%3D", "=").replace("%3B", ";").replace("%25", "%");
    s.split(';').filter_map(|part| part.rsplit_once('=')).filter_map(|(l, c)| Some((unesc(l), c.parse().ok()?))).collect()
}

/// Merged areas larger than this are refused (LocOS's guard refuses a workbook whose cells and merged areas pass
/// 500,000).
const MAX_MERGED_CELLS: i64 = 1_000_000;

/// What column mode reads in the workbook once: the shared strings and the number formats.
pub(super) struct Ctx {
    sst: Vec<String>,
    formats: values::Formats,
}

impl Ctx {
    pub(super) fn load(pkg: &mut Package) -> Result<Ctx, Error> {
        Ok(Ctx { sst: pkg.string_table()?, formats: values::Formats::load(pkg)? })
    }
}

/// A worksheet as openpyxl loads it, for column mode: `str(cell.value)` of every cell (merged cells other than an
/// area's first are empty), whether the value is a string, how wide the sheet is and which rows have cells.
pub(super) struct View {
    vals: HashMap<(i64, i64), (String, bool)>,
    width: i64,
    rows: Vec<i64>,
}

/// One cell's value (`WorkSheetParser.parse_cell`), `shared` holding the sheet's shared-formula masters so far.
fn parse_cell(d: &Dom, c: usize, key: (i64, i64), ctx: &Ctx, shared: &mut HashMap<Option<String>, (String, (i64, i64))>, name: &str) -> Result<(String, bool), Error> {
    let style = ctx.formats.style_of(d.get(c, "s"), name)?;
    let has_r = d.get(c, "r").is_some_and(|r| !r.is_empty());
    let t = d.get(c, "t").unwrap_or("n");
    if let Some(f) = d.find(c, M, "f") {
        let value = format!("={}", d.text(f).unwrap_or_default());
        return Ok(match d.get(f, "t") {
            // openpyxl makes these an ArrayFormula / DataTableFormula object, not a string
            Some("array") | Some("dataTable") => (value, false),
            Some("shared") => {
                let si = d.get(f, "si").map(str::to_string);
                if let Some((master, origin)) = shared.get(&si) {
                    let moved = formula::translate(master, *origin, has_r.then_some(key)).map_err(|_| invalid(name))?;
                    (moved, true)
                } else {
                    if value != "=" {
                        if !has_r || !formula::parses(&value) {
                            return Err(invalid(name));
                        }
                        shared.insert(si, (value.clone(), key));
                    }
                    (value, true)
                }
            }
            _ => (value, true),
        });
    }
    if t == "inlineStr" {
        return Ok(match d.find(c, M, "is") {
            Some(is) => (rich_content(d, is), true),
            None => (String::new(), false),
        });
    }
    let Some(v) = d.findtext(c, M, "v").filter(|v| !v.is_empty()) else { return Ok((String::new(), false)) };
    Ok(match t {
        "n" => {
            let n = values::cast_number(&v).ok_or_else(|| invalid(name))?;
            (ctx.formats.number_str(n, &v, style), false)
        }
        "s" => {
            // Python indexing: a negative index counts from the end
            let i = py_int(&v).ok_or_else(|| invalid(name))?;
            let at = if i < 0 { ctx.sst.len() as i64 + i } else { i };
            if at < 0 || at as usize >= ctx.sst.len() {
                return Err(invalid(name));
            }
            (ctx.sst[at as usize].clone(), true)
        }
        "b" => {
            let (_, digits) = py_int_parts(&v).ok_or_else(|| invalid(name))?;
            let zero = values::ascii_digits(&digits).trim_start_matches('0').is_empty();
            ((if zero { "False" } else { "True" }).to_string(), false)
        }
        "d" => (values::from_iso(&v, name)?, false),
        // "str", an error value ("#N/A") and any type openpyxl does not know: the text as it is
        _ => (v, true),
    })
}

/// `(row, column)` of a merged area's corner (`$` allowed).
fn merge_corner(r: &str, name: &str) -> Result<(i64, i64), Error> {
    let k = coord(r, name)?;
    if k.0 < 1 {
        return Err(invalid(name));
    }
    Ok(k)
}

impl View {
    /// Reads every cell of the sheet in document order, as openpyxl does (an error where openpyxl fails to load).
    pub(super) fn build(pkg: &mut Package, path: &str, ctx: &Ctx) -> Result<View, Error> {
        pkg.ensure_index(path)?;
        let d = &pkg.roots[path];
        let mut vals: HashMap<(i64, i64), (String, bool)> = HashMap::new();
        let mut shared = HashMap::new();
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
                let v = parse_cell(d, c, key, ctx, &mut shared, path)?;
                vals.insert(key, v);
            }
        }
        let mut width = vals.keys().map(|k| k.1).max().unwrap_or(0);
        // merged areas: every cell but the first is an empty MergedCell (`_clean_merge_range`); the last
        // mergeCells element is the one openpyxl keeps
        if let Some(list) = d.findall(d.root, M, "mergeCells").last().copied() {
            let mut area = 0i64;
            for m in d.findall(list, M, "mergeCell") {
                let r = d.get(m, "ref").ok_or_else(|| invalid(path))?;
                let (a, b) = match r.split_once(':') {
                    Some((a, b)) => (merge_corner(a, path)?, merge_corner(b, path)?),
                    None => {
                        let k = merge_corner(r, path)?;
                        (k, k)
                    }
                };
                if a.0 > b.0 || a.1 > b.1 {
                    return Err(invalid(path));
                }
                let n = (b.0 - a.0 + 1) * (b.1 - a.1 + 1);
                area += n;
                if area > MAX_MERGED_CELLS {
                    return Err(bad(format!("This XLSX file merges more than {MAX_MERGED_CELLS} cells. Split it into smaller files.")));
                }
                if n > 1 {
                    width = width.max(b.1);
                }
                for row in a.0..=b.0 {
                    for col in a.1..=b.1 {
                        if (row, col) != a {
                            vals.insert((row, col), (String::new(), false));
                        }
                    }
                }
            }
        }
        let mut rows: Vec<i64> = pkg.cells[path].keys().map(|k| k.0).collect();
        rows.dedup();
        Ok(View { vals, width, rows })
    }

    fn str_at(&self, row: i64, col: i64) -> &str {
        self.vals.get(&(row, col)).map_or("", |v| v.0.as_str())
    }

    /// The row as wide as `len` (`[_cell_str(c.value) for c in row]`, cut).
    fn row(&self, row: i64, len: i64) -> Vec<String> {
        (1..=len.min(self.width)).map(|c| self.str_at(row, c).to_string()).collect()
    }
}

/// Extracts a workbook in column mode.
pub(super) fn extract(bytes: &[u8], opts: &ExtractOptions, xo: &crate::XlsxOptions) -> Result<Document, Error> {
    let l = opts.limits();
    guard::check_input(bytes, &l)?;
    let entries = read(bytes, &l)?;
    let mut pkg = Package::open(&entries)?;
    let spec = plan::parse_spec(xo.columns.as_deref())?;
    if spec.delimiter.is_some() {
        return Err(Error::Unsupported("delimiter= applies to CSV/TSV files only.".into()));
    }
    let source_locale = opts.source_lang.clone().unwrap_or_default();
    let mut targets = xo.target_locales.clone();
    if let Some(t) = opts.target_lang.as_ref().filter(|t| !t.is_empty()) {
        if !targets.contains(t) {
            targets.push(t.clone());
        }
    }
    let locos_profile = opts.is_profile(profile::LOCOS);
    let ctx = Ctx::load(&mut pkg)?;
    // openpyxl loads every worksheet before the walk: a sheet it cannot read refuses the workbook
    let mut views = Vec::new();
    for path in pkg.sheets.clone() {
        views.push(View::build(&mut pkg, &path, &ctx)?);
    }
    let mut units: Vec<Unit> = Vec::new();
    let mut fits = false;
    for (n, (path, view)) in pkg.sheets.clone().iter().zip(&views).enumerate() {
        let mut plan: Option<Plan> = None;
        let mut header_seen = false;
        let note_len = |p: &Plan| p.notes.iter().map(|(_, i)| *i as i64 + 1).max().unwrap_or(0);
        for &row in &view.rows {
            if plan.is_none() {
                let blank = pkg.cells[path].range((row, 0)..=(row, i64::MAX)).all(|(k, _)| bilingua_model::py_strip(view.str_at(k.0, k.1)).is_empty());
                if blank {
                    continue;
                }
                let first = view.row(row, view.width);
                match plan::resolve(&spec, &first, first.len(), &source_locale, &targets) {
                    Ok(p) if !p.inplace => {
                        fits = true;
                        plan = Some(p);
                    }
                    _ => break,
                }
            }
            let p = plan.as_ref().expect("planned");
            if p.header && !header_seen {
                header_seen = true;
                continue;
            }
            let Some(s) = p.source.filter(|&s| (s as i64) < view.width) else { continue };
            let Some((value, true)) = view.vals.get(&(row, s as i64 + 1)).cloned() else { continue };
            if !plan::is_translatable(&value) {
                continue;
            }
            let idx = units.len();
            guard::check_unit_count(idx + 1, &l)?;
            let mut u = Unit::new(idx.to_string(), vec![Inline::text(value)]);
            u.context = UnitContext::new().with_kind("cell").with_key(format!("{}{row}", col_letters(s as i64 + 1))).with_location(path.clone());
            u.context.notes = plan::note_parts(p, &view.row(row, note_len(p)));
            u.context.extra.insert(profile::P_INDEX.to_string(), idx.to_string());
            u.hidden.insert(KEY.to_string(), idx.to_string());
            u.hidden.insert(XC.to_string(), format!("{n},{row},{}", s + 1));
            u.hidden.insert(XT.to_string(), encode_targets(p));
            if locos_profile {
                u.hidden.insert(XL.to_string(), "1".to_string());
            }
            units.push(u);
        }
    }
    if !fits {
        return Err(Error::Unsupported("The column mapping matches no sheet in this workbook (check the source= column).".into()));
    }
    let doc = Document::new("xlsx", units, Skeleton::binary(bytes.to_vec(), parser_info!()))
        .with_source_lang(opts.source_lang.clone())
        .with_target_lang(opts.target_lang.clone())
        .with_granularity(Granularity::Paragraph);
    guard::finish(doc, vec![], &l)
}

/// Writes the translations of a column-mode document (`_rebuild_table`). Values are read as openpyxl read the
/// original (each sheet's view is taken before its first write).
pub(super) fn apply(pkg: &mut Package, doc: &Document, texts: &BTreeMap<usize, String>) -> Result<(), Error> {
    let ctx = Ctx::load(pkg)?;
    let mut views: HashMap<usize, View> = HashMap::new();
    for u in &doc.units {
        let Some(xc) = u.hidden(XC) else { continue };
        let Some(tr) = u.hidden(KEY).and_then(|k| k.parse::<usize>().ok()).and_then(|k| texts.get(&k)) else { continue };
        let mut it = xc.split(',').map(|x| x.parse::<i64>().ok());
        let (Some(Some(sheet)), Some(Some(row)), Some(Some(col))) = (it.next(), it.next(), it.next()) else { continue };
        let Some(path) = usize::try_from(sheet).ok().and_then(|s| pkg.sheets.get(s)).cloned() else { continue };
        let sheet = sheet as usize;
        let tr = xml_target_text(tr);
        if !views.contains_key(&sheet) {
            let v = View::build(pkg, &path, &ctx)?;
            views.insert(sheet, v);
        }
        let view = &views[&sheet];
        let value = view.str_at(row, col).to_string();
        let p = Plan { targets: decode_targets(u.hidden(XT).unwrap_or("")), ..Plan::default() };
        match plan::target_col(&p, doc.target_lang.as_deref()) {
            None => {
                if tr != value {
                    pkg.set_cell_text(sheet, row, col, &tr)?;
                }
            }
            Some(tcol) => {
                let existing = view.str_at(row, tcol as i64 + 1).to_string();
                if tr == value && !bilingua_model::py_strip(&existing).is_empty() {
                    continue; // untranslated: keep a target someone already filled
                }
                if tr != existing {
                    pkg.set_cell_text_with(sheet, row, tcol as i64 + 1, &tr, true)?;
                }
            }
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------- cells and rows

impl Package {
    /// Gives every row and cell of the sheet its address, so inserting one cannot shift the position of a later
    /// row or cell, and indexes the rows by number (`_make_explicit`).
    fn make_explicit(&mut self, path: &str) -> Result<(), Error> {
        if self.explicit.contains(path) {
            return Ok(());
        }
        self.ensure_index(path)?;
        let entries: Vec<((i64, i64), usize)> = self.cells[path].iter().map(|(k, v)| (*k, *v)).collect();
        let d = self.roots.get_mut(path).expect("sheet part");
        for ((row, col), c) in entries {
            d.set(c, "r", &format!("{}{row}", col_letters(col)));
        }
        let mut row_n = 0i64;
        let mut by_number = BTreeMap::new();
        let root = d.root;
        for row in d.iter_tag(root, M, "row") {
            row_n = match d.get(row, "r") {
                Some(r) => row_number(r, path)?,
                None => row_n + 1,
            };
            d.set(row, "r", &row_n.to_string());
            by_number.insert(row_n, row);
        }
        self.rows.insert(path.to_string(), by_number);
        self.explicit.insert(path.to_string());
        Ok(())
    }

    /// A new, empty cell at (row, column), the row made when needed, in address order (`_create_cell`).
    pub(super) fn create_cell(&mut self, path: &str, row: i64, col: i64) -> Result<usize, Error> {
        use std::ops::Bound::{Excluded, Unbounded};
        let data = {
            let d = &self.roots[path];
            d.find(d.root, M, "sheetData")
        }
        .ok_or_else(|| bad("This Excel sheet has no cell table."))?;
        self.make_explicit(path)?;
        let by_number = self.rows.get_mut(path).expect("rows");
        let d = self.roots.get_mut(path).expect("sheet part");
        let row_el = match by_number.get(&row) {
            Some(&r) => r,
            None => {
                let r = d.new_element(data, M, "row");
                d.set(r, "r", &row.to_string());
                match by_number.range((Excluded(row), Unbounded)).next() {
                    Some((_, &next)) => d.add_previous(next, &[r]),
                    None => d.append(data, &[r]),
                }
                by_number.insert(row, r);
                r
            }
        };
        d.remove_attr(row_el, "spans"); // an optional hint the new cell may fall outside
        let c = d.new_element(row_el, M, "c");
        d.set(c, "r", &format!("{}{row}", col_letters(col)));
        let next = d.element_children(row_el).into_iter().find(|&o| d.get(o, "r").and_then(|r| coord(r, path).ok()).is_some_and(|k| k.1 > col));
        match next {
            Some(n) => d.add_previous(n, &[c]),
            None => d.append(row_el, &[c]),
        }
        self.cells.get_mut(path).expect("index").insert((row, col), c);
        let root = d.root;
        if let Some(dim) = d.find(root, M, "dimension") {
            let r = d.get(dim, "ref").unwrap_or("").to_string();
            let corners: Result<Vec<(i64, i64)>, Error> = r.split(':').map(|x| coord(x, path)).collect();
            if let Ok(corners) = corners {
                let rows: Vec<i64> = corners.iter().map(|k| k.0).chain([row]).collect();
                let cols: Vec<i64> = corners.iter().map(|k| k.1).chain([col]).collect();
                let lo = format!("{}{}", col_letters(*cols.iter().min().expect("one")), rows.iter().min().expect("one"));
                let hi = format!("{}{}", col_letters(*cols.iter().max().expect("one")), rows.iter().max().expect("one"));
                d.set(dim, "ref", &if lo == hi { lo } else { format!("{lo}:{hi}") });
            }
        }
        Ok(c)
    }

    /// Before the shared-formula master `c` loses its formula, every dependent of the same group (no text of its
    /// own) gets its own formula, translated from the master's place (`_unshare`); an error where openpyxl's
    /// translator raises.
    pub(super) fn unshare(&mut self, path: &str, c: usize, f: usize) -> Result<(), Error> {
        let d = &self.roots[path];
        let master = d.text(f).unwrap_or_default();
        if d.get(f, "t") != Some("shared") || bilingua_model::py_strip(&master).is_empty() {
            return Ok(());
        }
        let Some(si) = d.get(f, "si").map(str::to_string) else { return Ok(()) };
        if !self.formula_index.contains_key(path) {
            // the cells by element and the shared formulas by group, once per sheet (writes keep both valid)
            let at: HashMap<usize, (i64, i64)> = self.cells[path].iter().map(|(k, v)| (*v, *k)).collect();
            let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
            for other in d.iter_tag(d.root, M, "f") {
                if let (Some("shared"), Some(s)) = (d.get(other, "t"), d.get(other, "si")) {
                    groups.entry(s.to_string()).or_default().push(other);
                }
            }
            self.formula_index.insert(path.to_string(), (at, groups));
        }
        let (at, groups) = &self.formula_index[path];
        let d = &self.roots[path];
        let Some(&origin) = at.get(&c) else { return Ok(()) };
        let formula = format!("={}", bilingua_model::py_strip(&master));
        let mut edits: Vec<(usize, String)> = Vec::new();
        for &other in groups.get(&si).map(Vec::as_slice).unwrap_or(&[]) {
            if other == f || d.get(other, "t") != Some("shared") || d.get(other, "si") != Some(si.as_str()) {
                continue;
            }
            let Some(&key) = d.nodes[other].parent.and_then(|p| at.get(&p)) else { continue };
            if !bilingua_model::py_strip(&d.text(other).unwrap_or_default()).is_empty() {
                continue;
            }
            let moved = formula::translate(&formula, origin, Some(key)).map_err(|_| {
                Error::Write(format!("xlsx: the shared formula of {}{} cannot be moved to {}{}", col_letters(origin.1), origin.0, col_letters(key.1), key.0))
            })?;
            edits.push((other, moved.strip_prefix('=').unwrap_or("").to_string()));
        }
        let d = self.roots.get_mut(path).expect("sheet part");
        for (other, text) in edits {
            d.set_text(other, &text);
            for a in ["t", "si", "ref"] {
                d.remove_attr(other, a);
            }
        }
        Ok(())
    }

    /// Removes `xl/calcChain.xml` (its member, content-type override and workbook relationship); Excel rebuilds it
    /// (`_drop_calc_chain`).
    pub(super) fn drop_calc_chain(&mut self) -> Result<(), Error> {
        let rn = rels_name(&self.workbook);
        if !self.load(&rn)? {
            return Ok(());
        }
        let hits: Vec<(usize, Option<String>)> = {
            let d = &self.roots[&rn];
            d.iter_tag(d.root, PKG_RELS, "Relationship")
                .into_iter()
                .filter(|&r| d.get(r, "Type").unwrap_or("").rsplit('/').next() == Some("calcChain"))
                .map(|r| (r, resolve(&self.workbook, d.get(r, "Target").unwrap_or(""))))
                .collect()
        };
        for (rel, path) in hits {
            self.roots.get_mut(&rn).expect("rels").remove(rel);
            self.dirty.insert(rn.clone());
            let Some(p) = path.filter(|p| self.names.contains(p)) else { continue };
            self.removed.insert(p.clone());
            if self.load("[Content_Types].xml")? {
                let t = self.roots.get_mut("[Content_Types].xml").expect("types");
                let root = t.root;
                for o in t.iter_tag(root, CT, "Override") {
                    if t.get(o, "PartName").unwrap_or("").trim_start_matches('/') == p {
                        t.remove(o);
                        self.dirty.insert("[Content_Types].xml".into());
                    }
                }
            }
        }
        Ok(())
    }
}

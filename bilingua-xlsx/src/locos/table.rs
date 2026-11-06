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

fn encode_targets(p: &Plan) -> String {
    p.targets.iter().map(|(l, c)| format!("{l}={c}")).collect::<Vec<_>>().join(";")
}

fn decode_targets(s: &str) -> Vec<(String, usize)> {
    s.split(';').filter_map(|part| part.rsplit_once('=')).filter_map(|(l, c)| Some((l.to_string(), c.parse().ok()?))).collect()
}

/// The text of a cell as openpyxl's `str(value)` has it, for header detection and notes: a number or a formula is
/// its text, a boolean `True` / `False`. Also whether the value is a string (not a formula).
fn cell_str(d: &Dom, c: usize, sst: &[String], name: &str) -> Result<(String, bool), Error> {
    if let Some(v) = cell_value(d, c, sst, name)? {
        return Ok((v, true));
    }
    if let Some(f) = d.find(c, M, "f") {
        return Ok((format!("={}", d.text(f).unwrap_or_default()), false));
    }
    let v = d.findtext(c, M, "v").unwrap_or_default();
    if v.is_empty() {
        return Ok((String::new(), false));
    }
    let out = match d.get(c, "t").unwrap_or("n") {
        "n" => {
            if v.contains(['.', 'E', 'e']) {
                match bilingua_model::py_strip(&v).replace('_', "").parse::<f64>() {
                    Ok(f) if f.is_finite() && f.fract() == 0.0 && f.abs() < 1e16 => format!("{f:.1}"),
                    Ok(f) => format!("{f}"),
                    Err(_) => v,
                }
            } else {
                match py_int_parts(&v) {
                    Some((neg, digits)) => {
                        let t = digits.trim_start_matches('0');
                        if t.is_empty() {
                            "0".to_string()
                        } else if neg {
                            format!("-{t}")
                        } else {
                            t.to_string()
                        }
                    }
                    None => v,
                }
            }
        }
        "b" => {
            if py_int_parts(&v).is_some_and(|(_, dg)| dg.trim_start_matches('0').is_empty()) {
                "False".into()
            } else {
                "True".into()
            }
        }
        _ => v,
    };
    Ok((out, false))
}

/// One row as strings, as wide as the sheet (`[_cell_str(c.value) for c in row]`).
fn row_values(pkg: &Package, path: &str, row: i64, width: i64, sst: &[String]) -> Result<Vec<String>, Error> {
    let d = &pkg.roots[path];
    let mut out = vec![String::new(); width as usize];
    for (&(_, col), &c) in pkg.cells[path].range((row, 0)..=(row, i64::MAX)) {
        out[(col - 1) as usize] = cell_str(d, c, sst, path)?.0;
    }
    Ok(out)
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
    let sst = pkg.string_table()?;
    let mut units: Vec<Unit> = Vec::new();
    let mut fits = false;
    for (n, path) in pkg.sheets.clone().iter().enumerate() {
        pkg.ensure_index(path)?;
        let (width, rows) = {
            let cells = &pkg.cells[path];
            let Some(width) = cells.keys().map(|k| k.1).max() else { continue };
            let mut rows: Vec<i64> = cells.keys().map(|k| k.0).collect();
            rows.dedup();
            (width, rows)
        };
        let mut plan: Option<Plan> = None;
        let mut planned = false;
        let mut header_seen = false;
        for row in rows {
            let values = row_values(&pkg, path, row, width, &sst)?;
            if !planned {
                if values.iter().all(|v| bilingua_model::py_strip(v).is_empty()) {
                    continue;
                }
                planned = true;
                plan = plan::resolve(&spec, &values, values.len(), &source_locale, &targets).ok();
                match &plan {
                    Some(p) if !p.inplace => fits = true,
                    _ => break,
                }
            }
            let p = plan.as_ref().expect("planned");
            if p.header && !header_seen {
                header_seen = true;
                continue;
            }
            let Some(s) = p.source.filter(|&s| (s as i64) < width) else { continue };
            let Some(&c) = pkg.cells[path].get(&(row, s as i64 + 1)) else { continue };
            let Some(value) = cell_value(&pkg.roots[path], c, &sst, path)?.filter(|v| plan::is_translatable(v)) else { continue };
            let idx = units.len();
            guard::check_unit_count(idx + 1, &l)?;
            let mut u = Unit::new(idx.to_string(), vec![Inline::text(value)]);
            u.context = UnitContext::new().with_kind("cell").with_key(format!("{}{row}", col_letters(s as i64 + 1))).with_location(path.clone());
            u.context.notes = plan::note_parts(p, &values);
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

/// Writes the translations of a column-mode document (`_rebuild_table`).
pub(super) fn apply(pkg: &mut Package, doc: &Document, texts: &BTreeMap<usize, String>) -> Result<(), Error> {
    let sst = pkg.string_table()?;
    for u in &doc.units {
        let Some(xc) = u.hidden(XC) else { continue };
        let Some(tr) = u.hidden(KEY).and_then(|k| k.parse::<usize>().ok()).and_then(|k| texts.get(&k)) else { continue };
        let mut it = xc.split(',').map(|x| x.parse::<i64>().ok());
        let (Some(Some(sheet)), Some(Some(row)), Some(Some(col))) = (it.next(), it.next(), it.next()) else { continue };
        let tr = xml_target_text(tr);
        let sheet = sheet as usize;
        let path = pkg.sheets[sheet].clone();
        pkg.ensure_index(&path)?;
        let value = match pkg.cells[&path].get(&(row, col)) {
            Some(&c) => cell_str(&pkg.roots[&path], c, &sst, &path)?.0,
            None => String::new(),
        };
        let p = Plan { targets: decode_targets(u.hidden(XT).unwrap_or("")), ..Plan::default() };
        match plan::target_col(&p, doc.target_lang.as_deref()) {
            None => {
                if tr != value {
                    pkg.set_cell_text(sheet, row, col, &tr)?;
                }
            }
            Some(tcol) => {
                let existing = match pkg.cells[&path].get(&(row, tcol as i64 + 1)) {
                    Some(&c) => cell_str(&pkg.roots[&path], c, &sst, &path)?.0,
                    None => String::new(),
                };
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

fn col_number(letters: &str) -> i64 {
    letters.to_ascii_uppercase().bytes().fold(0, |n, b| n * 26 + (b - b'A' + 1) as i64)
}

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

    /// Before the shared-formula master `c` loses its formula, every dependent of the same group gets its own
    /// formula, translated from the master's place (`_unshare`).
    pub(super) fn unshare(&mut self, path: &str, c: usize, f: usize) {
        let d = &self.roots[path];
        let master = d.text(f).unwrap_or_default();
        if d.get(f, "t") != Some("shared") || bilingua_model::py_strip(&master).is_empty() {
            return;
        }
        let Some(si) = d.get(f, "si").map(str::to_string) else { return };
        let where_: HashMap<usize, (i64, i64)> = self.cells[path].iter().map(|(k, v)| (*v, *k)).collect();
        let Some(&origin) = where_.get(&c) else { return };
        let formula = bilingua_model::py_strip(&master).to_string();
        let mut edits: Vec<(usize, String)> = Vec::new();
        for other in d.iter_tag(d.root, M, "f") {
            if other == f || d.get(other, "t") != Some("shared") || d.get(other, "si") != Some(si.as_str()) {
                continue;
            }
            let Some(&key) = d.nodes[other].parent.and_then(|p| where_.get(&p)) else { continue };
            if !bilingua_model::py_strip(&d.text(other).unwrap_or_default()).is_empty() {
                continue;
            }
            edits.push((other, translate_formula(&formula, key.0 - origin.0, key.1 - origin.1)));
        }
        let d = self.roots.get_mut(path).expect("sheet part");
        for (other, text) in edits {
            d.set_text(other, &text);
            for a in ["t", "si", "ref"] {
                d.remove_attr(other, a);
            }
        }
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

// ------------------------------------------------------------------------------------------- formulas

/// Moves the relative references of a formula by (rows, columns), as openpyxl's `Translator` does for a shared
/// formula's dependents: cell references (`A1`, `$A1`, `A$1`), column ranges (`A:B`) and row ranges (`1:3`)
/// outside strings, quoted sheet names and structured references. A reference moved off the sheet becomes
/// `#REF!`.
fn translate_formula(formula: &str, drow: i64, dcol: i64) -> String {
    let ch: Vec<char> = formula.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let word = |c: char| c.is_alphanumeric() || c == '_' || c == '.' || c == '$';
    while i < ch.len() {
        let c = ch[i];
        if c == '"' || c == '\'' {
            // a string or a quoted sheet name, a doubled quote inside is one quote
            out.push(c);
            i += 1;
            while i < ch.len() {
                out.push(ch[i]);
                if ch[i] == c {
                    if ch.get(i + 1) == Some(&c) {
                        out.push(c);
                        i += 2;
                        continue;
                    }
                    i += 1;
                    break;
                }
                i += 1;
            }
        } else if c == '[' {
            let mut depth = 0;
            while i < ch.len() {
                out.push(ch[i]);
                if ch[i] == '[' {
                    depth += 1;
                } else if ch[i] == ']' {
                    depth -= 1;
                    if depth == 0 {
                        i += 1;
                        break;
                    }
                }
                i += 1;
            }
        } else if word(c) {
            let start = i;
            while i < ch.len() && word(ch[i]) {
                i += 1;
            }
            let run: String = ch[start..i].iter().collect();
            let next = ch.get(i).copied();
            if next == Some('(') || next == Some('!') {
                out.push_str(&run); // a function or an unquoted sheet name
                continue;
            }
            // a range `X:Y` of columns or of rows
            if next == Some(':') {
                let mut j = i + 1;
                while j < ch.len() && word(ch[j]) {
                    j += 1;
                }
                let second: String = ch[i + 1..j].iter().collect();
                if let (Some(a), Some(b)) = (shift_part(&run, true, drow, dcol), shift_part(&second, true, drow, dcol)) {
                    out.push_str(&format!("{a}:{b}"));
                    i = j;
                    continue;
                }
            }
            out.push_str(&shift_cell(&run, drow, dcol).unwrap_or(run));
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// A whole column (`$A`, `B`) or whole row (`$1`, `3`) reference moved; `None` when `s` is neither.
fn shift_part(s: &str, _range: bool, drow: i64, dcol: i64) -> Option<String> {
    let (abs, body) = match s.strip_prefix('$') {
        Some(b) => (true, b),
        None => (false, s),
    };
    if !body.is_empty() && body.len() <= 3 && body.bytes().all(|b| b.is_ascii_alphabetic()) {
        let n = col_number(body);
        if n > 16384 {
            return None;
        }
        let m = if abs { n } else { n + dcol };
        return Some(if m < 1 || m > 16384 { "#REF!".into() } else { format!("{}{}", if abs { "$" } else { "" }, col_letters(m)) });
    }
    if !body.is_empty() && body.bytes().all(|b| b.is_ascii_digit()) {
        let n: i64 = body.parse().ok()?;
        if n < 1 || n > 1_048_576 {
            return None;
        }
        let m = if abs { n } else { n + drow };
        return Some(if m < 1 || m > 1_048_576 { "#REF!".into() } else { format!("{}{m}", if abs { "$" } else { "" }) });
    }
    None
}

/// A cell reference (`A1`, `$A$1`) moved; `None` when `s` is not one.
fn shift_cell(s: &str, drow: i64, dcol: i64) -> Option<String> {
    let (abs_c, rest) = match s.strip_prefix('$') {
        Some(r) => (true, r),
        None => (false, s),
    };
    let letters: String = rest.chars().take_while(char::is_ascii_alphabetic).collect();
    let rest = &rest[letters.len()..];
    let (abs_r, digits) = match rest.strip_prefix('$') {
        Some(r) => (true, r),
        None => (false, rest),
    };
    if letters.is_empty() || letters.len() > 3 || digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (col, row): (i64, i64) = (col_number(&letters), digits.parse().ok()?);
    if col > 16384 || row < 1 || row > 1_048_576 {
        return None;
    }
    let (nc, nr) = (if abs_c { col } else { col + dcol }, if abs_r { row } else { row + drow });
    if nc < 1 || nc > 16384 || nr < 1 || nr > 1_048_576 {
        return Some("#REF!".into());
    }
    Some(format!("{}{}{}{nr}", if abs_c { "$" } else { "" }, col_letters(nc), if abs_r { "$" } else { "" }))
}

#[cfg(test)]
mod formula_tests {
    use super::translate_formula;

    #[test]
    fn moves_relative_references_only() {
        assert_eq!(translate_formula("UPPER(A2)", 1, 0), "UPPER(A3)");
        assert_eq!(translate_formula("$A$1+B$2+$C3", 2, 1), "$A$1+C$2+$C5");
        assert_eq!(translate_formula("SUM(A1:B2,\"A1\")", 1, 1), "SUM(B2:C3,\"A1\")");
        assert_eq!(translate_formula("'My Sheet'!A1+Other!B1", 1, 0), "'My Sheet'!A2+Other!B2");
        assert_eq!(translate_formula("SUM(A:B)+SUM(1:2)", 3, 1), "SUM(B:C)+SUM(4:5)");
        assert_eq!(translate_formula("LOG10(A1)*1.5", 1, 0), "LOG10(A2)*1.5");
        assert_eq!(translate_formula("T[Col1]+A1", 1, 0), "T[Col1]+A2");
        assert_eq!(translate_formula("A1", -1, 0), "#REF!");
    }
}

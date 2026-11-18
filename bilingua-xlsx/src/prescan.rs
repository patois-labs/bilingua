//! The limit pre-scan (BILI-115): before any sheet is parsed into a tree, the sheets and the shared strings are
//! walked once with [`crate::xml::scan`], which builds nothing, and a workbook over the cell, row, cell-text or
//! unit limit is refused with the error the full extraction would give, instead of after every sheet tree was built.
//!
//! It must never refuse a workbook the full extraction accepts, so everything it counts is either exactly what the
//! extraction counts or less than it:
//! * rows and cells: the walk of `Extractor::process_sheet` replayed on the scan events (the same sheets in the same
//!   order, the same namespace prefix, the same rows and cells, the same row numbers), charged against the same
//!   [`SheetBudget`], so it refuses at the same row or cell with the same message;
//! * cell text: each shared string charged as the extraction charges it, its characters times one plus the cells
//!   that use it (cells counted as `Extractor::cell` counts them: a `t="s"` cell with no formula and a numeric
//!   first `v`), against the same budget. Inline strings are left out, so the total is at most the extraction's;
//! * units: the shared strings that a cell uses and that hold a letter, each of which becomes a unit (left out
//!   when `xlsx.skip_hidden` may drop some). The other units (inline strings, headers, comments, sheet names) only
//!   add to the real number, which `guard::finish` checks against the same limit.
//!
//! The scan reads the parts as `parse` does; on a part `parse` would refuse it may read something else, but the
//! extraction then fails on that part anyway. A workbook that breaks several limits may be refused for another one
//! than the extraction would name first (a comment over the context budget, an inline string over the text total).
//!
//! The parts are already unpacked in memory when it runs (the zip is read first), so the saving is the trees, not
//! the unpacked bytes.

use crate::budget::SheetBudget;
use crate::refs::{cell_name, parse_ref};
use crate::xml::{parse_attrs, scan, unescape, Ev, Ns, ScanStop};
use crate::zipio::RawEntry;
use bilingua_model::{Error, Limits};
use std::collections::HashMap;

/// The unpacked size of the sheets and shared strings below which the scan is skipped: the smallest at which the
/// cell or unit limit can be reached (a cell takes at least 4 bytes, `<c/>`; a shared-string unit at least 16,
/// `<si><t>a</t></si>`), or the text limit can be reached without repeats (a character takes at least one byte).
///
/// It is a cost threshold, not a bound: below it a workbook can still be over a limit, through the row number a
/// row states (`<row r="999999"/>`) or a long shared string used by many cells (one 3,000-character string in 3,000
/// cells passes 8 Mi characters in about 25 KB). Skipping is always safe: the extraction refuses those itself, and
/// with parts this small its trees are small too, so refusing there costs little. Under `Limits::large` and
/// `unbounded` it is hundreds of megabytes or more, so ordinary workbooks are not scanned twice.
fn worth_scanning(l: &Limits) -> u64 {
    let cells = (l.xlsx_max_cells as u64).saturating_add(1).saturating_mul(4);
    let units = (l.max_units as u64).saturating_add(1).saturating_mul(16);
    let text = (l.xlsx_max_total_text_chars as u64).saturating_add(1);
    cells.min(units).min(text)
}

fn attr(raw: &str, name: &str) -> Option<String> {
    parse_attrs(raw).into_iter().find(|(k, _)| k == name).map(|(_, v)| v)
}

/// Where the sheet walk is: the element kinds `process_sheet` and `cell` look at, `Other` for the rest.
#[derive(Clone, Copy, PartialEq)]
enum Sheet {
    Root,
    Data,
    Row,
    Cell,
    /// The first `v` of a cell.
    Value,
    Other,
}

/// The cell being read.
#[derive(Default)]
struct CellState {
    /// The cell reference `Extractor::cell` gets is a valid one.
    valid: bool,
    shared: bool,
    formula: bool,
    has_value: bool,
    value: String,
}

/// Replays `process_sheet` over one sheet: rows and cells charged against `budget`, shared-string uses into `uses`.
/// `Ok(false)` when the sheet is not XML the extraction reads (the scan then gives up: the extraction fails on it).
fn sheet(text: &str, budget: &mut SheetBudget, uses: &mut HashMap<usize, usize>) -> Result<bool, Error> {
    let mut stack: Vec<Sheet> = Vec::new();
    let mut ns: Option<Ns> = None;
    let mut root_done = false;
    let (mut row_no, mut col_no) = (0u32, 0u32);
    let mut cell = CellState::default();
    let r = scan(text, &mut |ev| -> Result<(), Error> {
        match ev {
            Ev::Start(name, raw) => {
                let top = stack.last().copied();
                if top.is_none() && !root_done {
                    // the first top-level element is the root the extraction walks (`root_mut`)
                    root_done = true;
                    ns = Some(Ns::from_start(raw, crate::extract::SS_URIS, ""));
                    stack.push(Sheet::Root);
                    return Ok(());
                }
                let kind = match (top, ns.as_ref()) {
                    (Some(Sheet::Root), Some(ns)) if ns.is(name, "sheetData") => Sheet::Data,
                    (Some(Sheet::Data), Some(ns)) if ns.is(name, "row") => {
                        row_no = attr(raw, "r").and_then(|v| v.trim().parse::<u32>().ok()).unwrap_or(row_no + 1);
                        budget.row(i64::from(row_no))?;
                        col_no = 0;
                        Sheet::Row
                    }
                    (Some(Sheet::Row), Some(ns)) if ns.is(name, "c") => {
                        budget.cell()?;
                        let r = attr(raw, "r");
                        col_no = match r.as_deref().and_then(parse_ref) {
                            Some((col, _)) => col,
                            None => col_no + 1,
                        };
                        let name = match r {
                            Some(r) if parse_ref(&r).is_some() => r,
                            _ => cell_name(col_no, row_no),
                        };
                        cell = CellState { valid: parse_ref(&name).is_some(), shared: attr(raw, "t").as_deref() == Some("s"), ..CellState::default() };
                        Sheet::Cell
                    }
                    (Some(Sheet::Cell), Some(ns)) if ns.is(name, "f") => {
                        cell.formula = true;
                        Sheet::Other
                    }
                    (Some(Sheet::Cell), Some(ns)) if ns.is(name, "v") && !cell.has_value => {
                        cell.has_value = true;
                        Sheet::Value
                    }
                    _ => Sheet::Other,
                };
                stack.push(kind);
            }
            Ev::End => {
                if stack.pop() == Some(Sheet::Cell) && cell.valid && cell.shared && !cell.formula {
                    if let Ok(idx) = cell.value.trim().parse::<usize>() {
                        *uses.entry(idx).or_insert(0) += 1;
                    }
                }
            }
            Ev::Text(t) => {
                if stack.last() == Some(&Sheet::Value) {
                    cell.value.push_str(&unescape(t));
                }
            }
        }
        Ok(())
    });
    match r {
        Ok(()) => Ok(true),
        Err(ScanStop::Bad) => Ok(false),
        Err(ScanStop::Stop(e)) => Err(e),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Sst {
    Root,
    Item,
    /// A `t` of the item, or a `t` of one of its runs: its text is the item's.
    Text,
    Run,
    Other,
}

/// Charges every shared string against `budget` as the extraction does (characters times one plus its uses), and
/// counts the ones that become units. `None` when the part is not XML the extraction reads.
fn shared_strings(text: &str, budget: &mut SheetBudget, uses: &HashMap<usize, usize>) -> Result<Option<usize>, Error> {
    let mut stack: Vec<Sst> = Vec::new();
    let mut ns: Option<Ns> = None;
    let mut root_done = false;
    let (mut n, mut chars, mut letter, mut units) = (0usize, 0usize, false, 0usize);
    let r = scan(text, &mut |ev| -> Result<(), Error> {
        match ev {
            Ev::Start(name, raw) => {
                let top = stack.last().copied();
                if top.is_none() && !root_done {
                    // the first top-level element is the root the extraction walks (`root_mut`)
                    root_done = true;
                    ns = Some(Ns::from_start(raw, crate::extract::SS_URIS, ""));
                    stack.push(Sst::Root);
                    return Ok(());
                }
                let kind = match (top, ns.as_ref()) {
                    (Some(Sst::Root), Some(ns)) if ns.is(name, "si") => {
                        (chars, letter) = (0, false);
                        Sst::Item
                    }
                    (Some(Sst::Item), Some(ns)) if ns.is(name, "t") => Sst::Text,
                    (Some(Sst::Item), Some(ns)) if ns.is(name, "r") => Sst::Run,
                    (Some(Sst::Run), Some(ns)) if ns.is(name, "t") => Sst::Text,
                    _ => Sst::Other,
                };
                stack.push(kind);
            }
            Ev::End => {
                if stack.pop() == Some(Sst::Item) {
                    let idx = n;
                    n += 1;
                    let used = uses.get(&idx).copied();
                    budget.text(chars, 1 + used.unwrap_or(0))?;
                    if used.is_some() && letter {
                        units += 1;
                    }
                }
            }
            Ev::Text(t) => {
                if stack.last() == Some(&Sst::Text) {
                    let t = unescape(t);
                    chars += t.chars().count();
                    letter = letter || t.chars().any(char::is_alphabetic);
                }
            }
        }
        Ok(())
    });
    match r {
        Ok(()) => Ok(Some(units)),
        Err(ScanStop::Bad) => Ok(None),
        Err(ScanStop::Stop(e)) => Err(e),
    }
}

/// The pre-scan: `sheets` are the worksheet parts in the order the extraction walks them, `sst` the shared-strings
/// part it reads. Refuses a workbook over the row, cell, cell-text or unit limit; anything else is left to the
/// extraction.
pub(crate) fn prescan(entries: &[RawEntry], sheets: &[&str], sst: &str, skip_hidden: bool, l: &Limits) -> Result<(), Error> {
    #[cfg(test)]
    if tests::OFF.with(|o| o.get()) {
        return Ok(());
    }
    let r = run(entries, sheets, sst, skip_hidden, l);
    #[cfg(test)]
    if r.is_err() {
        tests::REFUSED.with(|n| n.set(n.get() + 1));
    }
    r
}

fn run(entries: &[RawEntry], sheets: &[&str], sst: &str, skip_hidden: bool, l: &Limits) -> Result<(), Error> {
    let find = |name: &str| entries.iter().find(|e| e.name == name);
    let size: u64 = sheets.iter().chain(std::iter::once(&sst)).filter_map(|n| find(n)).map(|e| e.data.len() as u64).sum();
    #[cfg(test)]
    let skip = !tests::ALWAYS.with(|a| a.get()) && size < worth_scanning(l);
    #[cfg(not(test))]
    let skip = size < worth_scanning(l);
    if skip {
        return Ok(());
    }
    let mut budget = SheetBudget::new(*l);
    let mut uses: HashMap<usize, usize> = HashMap::new();
    for name in sheets {
        let Some(e) = find(name) else { continue };
        let Ok(text) = std::str::from_utf8(&e.data) else { return Ok(()) };
        if !sheet(text, &mut budget, &mut uses)? {
            return Ok(());
        }
    }
    let Some(e) = find(sst) else { return Ok(()) };
    let Ok(text) = std::str::from_utf8(&e.data) else { return Ok(()) };
    let Some(units) = shared_strings(text, &mut budget, &uses)? else { return Ok(()) };
    // after every string's text, as `guard::finish` counts units after the whole walk
    if !skip_hidden {
        bilingua_model::guard::check_unit_count(units, l)?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::fixtures::{zip, NS};
    use bilingua_model::{Error, ExtractOptions, Limits};

    thread_local! {
        /// Turns the pre-scan off, to compare an extraction with and without it.
        pub(crate) static OFF: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        /// Runs the pre-scan whatever the size of the parts, so small test workbooks exercise every limit.
        pub(crate) static ALWAYS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
        /// How often the pre-scan refused on this thread.
        pub(crate) static REFUSED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

    fn book(sheets: &[String], sst: &str) -> Vec<u8> {
        let mut files: Vec<(String, String)> = Vec::new();
        let mut rels = String::new();
        let mut list = String::new();
        let mut overrides = String::new();
        for (i, s) in sheets.iter().enumerate() {
            let n = i + 1;
            rels.push_str(&format!(r#"<Relationship Id="rId{n}" Type="{REL}/worksheet" Target="worksheets/sheet{n}.xml"/>"#));
            list.push_str(&format!(r#"<sheet name="S{n}" sheetId="{n}" r:id="rId{n}"/>"#));
            overrides.push_str(&format!(
                r#"<Override PartName="/xl/worksheets/sheet{n}.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>"#
            ));
            files.push((format!("xl/worksheets/sheet{n}.xml"), s.clone()));
        }
        rels.push_str(&format!(r#"<Relationship Id="rIdS" Type="{REL}/sharedStrings" Target="sharedStrings.xml"/>"#));
        files.push(("xl/sharedStrings.xml".into(), sst.to_string()));
        files.push((
            "[Content_Types].xml".into(),
            format!(
                r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>{overrides}</Types>"#
            ),
        ));
        files.push((
            "_rels/.rels".into(),
            format!(r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="{REL}/officeDocument" Target="xl/workbook.xml"/></Relationships>"#),
        ));
        files.push(("xl/workbook.xml".into(), format!(r#"<?xml version="1.0"?><workbook xmlns="{NS}" xmlns:r="{REL}"><sheets>{list}</sheets></workbook>"#)));
        files.push((
            "xl/_rels/workbook.xml.rels".into(),
            format!(r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{rels}</Relationships>"#),
        ));
        let files: Vec<(&str, String)> = files.iter().map(|(n, d)| (n.as_str(), d.clone())).collect();
        zip(&files)
    }

    /// Sheets and shared strings with what the pre-scan must read as the extraction does: rows without `r`, cells
    /// without `r` or with a bad one, a formula cell, an inline string, a value split by a comment, a second `v`,
    /// cells outside `sheetData` and in another namespace, a second `sheetData`, a string used twice, an unused one,
    /// one without a letter, a rich one with a phonetic run, CDATA, entities and a CRLF. `p` is the prefix ("" or
    /// "x:").
    fn tricky(p: &str) -> (Vec<String>, String) {
        let xmlns = match p.strip_suffix(':') {
            Some(x) => format!(r#"xmlns:{x}="{NS}""#),
            None => format!(r#"xmlns="{NS}""#),
        };
        let sheet = [
            format!(r#"<?xml version="1.0"?><{p}worksheet {xmlns} xmlns:o="urn:other"><{p}cols/><o:c t="s"><o:v>0</o:v></o:c><{p}sheetData>"#),
            format!(r#"<{p}row r="1"><{p}c r="A1" t="s"><{p}v>0</{p}v></{p}c><{p}c t="s"><{p}v>1</{p}v></{p}c><{p}c r="!!" t="s"><{p}v> 2 </{p}v></{p}c></{p}row>"#),
            format!(r#"<{p}row><{p}c r="A2" t="s"><{p}f>A1</{p}f><{p}v>3</{p}v></{p}c><{p}c r="B2" t="inlineStr"><{p}is><{p}t>Inline words</{p}t></{p}is></{p}c>"#),
            format!(r#"<{p}c r="C2" t="s"><{p}v>1<!-- x -->0</{p}v></{p}c><{p}c r="D2" t="s"><{p}v>4</{p}v><{p}v>5</{p}v></{p}c><o:c t="s"><o:v>6</o:v></o:c></{p}row>"#),
            format!(r#"<{p}row r="7"/><{p}row r="8"><{p}c r="A8" t="s"><{p}v>0</{p}v></{p}c><{p}c r="B8" t="s"><{p}v>7</{p}v></{p}c>"#),
            format!(r#"<{p}c r="C8" t="s"><{p}v>8</{p}v></{p}c><{p}c r="D8" t="s"><{p}v>99</{p}v></{p}c></{p}row></{p}sheetData>"#),
            format!(r#"<{p}sheetData><{p}row><{p}c t="s"><{p}v>9</{p}v></{p}c></{p}row></{p}sheetData></{p}worksheet>"#),
        ]
        .concat();
        let items = [
            format!("<{p}si><{p}t>Alpha beta</{p}t></{p}si>"),
            format!("<{p}si><{p}r><{p}rPr/><{p}t>Rich </{p}t></{p}r><{p}r><{p}t>text&amp;more</{p}t></{p}r><{p}rPh><{p}t>ruby ruby ruby</{p}t></{p}rPh></{p}si>"),
            format!("<{p}si><{p}t><![CDATA[Cdata <b> text]]></{p}t></{p}si>"),
            format!("<{p}si><{p}t>Formula only</{p}t></{p}si>"),
            format!("<{p}si><{p}t>12345</{p}t></{p}si>"),
            format!("<{p}si><{p}t>Second value</{p}t></{p}si>"),
            format!("<{p}si><{p}t>Other namespace</{p}t></{p}si>"),
            format!("<{p}si><{p}t>Line\r\none &#x263A; and <!-- c --> more</{p}t></{p}si>"),
            format!("<{p}si><{p}t>Never used here</{p}t><o:t>skip me</o:t></{p}si>"),
            format!("<{p}si><{p}t>Second sheet data</{p}t></{p}si>"),
            format!("<{p}si><{p}t>Unused tail item</{p}t></{p}si>"),
        ];
        let sst = format!(r#"<?xml version="1.0"?><{p}sst {xmlns} xmlns:o="urn:other">{}</{p}sst>"#, items.concat());
        (vec![sheet.clone(), sheet], sst)
    }

    /// A plain workbook of `rows` x `cols` shared-string cells, every string distinct.
    fn grid(rows: usize, cols: usize) -> Vec<u8> {
        let mut sheet = format!(r#"<?xml version="1.0"?><worksheet xmlns="{NS}"><sheetData>"#);
        let mut sst = format!(r#"<?xml version="1.0"?><sst xmlns="{NS}">"#);
        for r in 0..rows {
            sheet.push_str(&format!(r#"<row r="{}">"#, r + 1));
            for c in 0..cols {
                let i = r * cols + c;
                sheet.push_str(&format!(r#"<c t="s"><v>{i}</v></c>"#));
                sst.push_str(&format!("<si><t>Item number {i} words</t></si>"));
            }
            sheet.push_str("</row>");
        }
        sheet.push_str("</sheetData></worksheet>");
        sst.push_str("</sst>");
        book(&[sheet], &sst)
    }

    fn run(bytes: &[u8], l: Limits, skip_hidden: bool, off: bool) -> Result<usize, String> {
        OFF.with(|o| o.set(off));
        let xo = crate::XlsxOptions { skip_hidden, ..Default::default() };
        let r = crate::extract_with(bytes, &ExtractOptions::default().with_limits(l), &xo);
        OFF.with(|o| o.set(false));
        r.map(|d| d.units.len()).map_err(|e: Error| e.to_string())
    }

    fn refused() -> usize {
        REFUSED.with(|n| n.get())
    }

    /// The pre-scan changes nothing but when a refusal comes: with any one spreadsheet or unit limit at any value
    /// around the workbook's own numbers, the extraction with it and without it ends the same (the same units, or
    /// the same error), and it refuses some of them itself.
    #[test]
    fn prescan_never_changes_the_outcome() {
        let mut books = vec![("fixture".to_string(), crate::fixtures::workbook_zip()), ("grid".to_string(), grid(7, 3))];
        for p in ["", "x:"] {
            let (sheets, sst) = tricky(p);
            books.push((format!("tricky {p:?}"), book(&sheets, &sst)));
        }
        let before = refused();
        // the fixtures are far below the size threshold: scan them anyway, so every limit's refusal is compared
        ALWAYS.with(|a| a.set(true));
        type Make = fn(usize) -> Limits;
        let sets: [(&str, Make); 5] = [
            ("cells", |k| Limits::new().with_xlsx_max_cells(k)),
            ("rows", |k| Limits::new().with_xlsx_max_rows(k as u32)),
            ("text", |k| Limits::new().with_xlsx_max_total_text_chars(k)),
            ("cell chars", |k| Limits::new().with_xlsx_max_cell_chars(k)),
            ("units", |k| Limits::new().with_max_units(k)),
        ];
        for (name, bytes) in &books {
            for skip_hidden in [false, true] {
                let base = run(bytes, Limits::new(), skip_hidden, true);
                assert!(base.is_ok(), "{name}: {base:?}");
                for (what, make) in &sets {
                    let at = refused();
                    for k in (0..40).chain((40..700).step_by(7)) {
                        let l = make(k);
                        let with = run(bytes, l, skip_hidden, false);
                        let without = run(bytes, l, skip_hidden, true);
                        assert_eq!(with, without, "{name}, {what} = {k}, skip_hidden {skip_hidden}");
                    }
                    // each limit is refused by the pre-scan itself for some value (units only when they are counted)
                    if !(*what == "units" && skip_hidden) {
                        assert!(refused() > at, "{name}: the pre-scan refused nothing for {what}, skip_hidden {skip_hidden}");
                    }
                }
            }
        }
        ALWAYS.with(|a| a.set(false));
        assert!(refused() > before, "the pre-scan refused none of them");
    }

    /// A workbook over the cell or unit limit is refused by the pre-scan, before a sheet tree is built, with the
    /// error the extraction gives without it; one just under them is not.
    #[test]
    fn over_the_limits_the_prescan_refuses() {
        let l = Limits::new().with_xlsx_max_cells(2_000).with_max_units(1_500);
        let before = refused();
        let over = grid(101, 20);
        let e = run(&over, l, false, false).unwrap_err();
        assert_eq!(refused(), before + 1);
        assert_eq!(Err(e.clone()), run(&over, l, false, true));
        assert!(e.contains("more than 2000 cells"), "{e}");
        let units_over = grid(80, 20);
        let e = run(&units_over, l, false, false).unwrap_err();
        assert_eq!(refused(), before + 2);
        assert_eq!(Err(e.clone()), run(&units_over, l, false, true));
        assert!(e.contains("more than 1500 segments"), "{e}");
        let under = grid(75, 20);
        assert_eq!(run(&under, l, false, false), Ok(1_500));
        assert_eq!(refused(), before + 2);
        // the hosted profile: 300,000 shared strings
        let hosted = grid(30_000, 10);
        let e = run(&hosted, Limits::new(), false, false).unwrap_err();
        assert_eq!(refused(), before + 3);
        assert_eq!(Err(e), run(&hosted, Limits::new(), false, true));
    }

    /// The scan reads a part as `parse` builds it.
    #[test]
    fn scan_follows_parse() {
        use crate::xml::{parse, scan, Ev, Node, ScanStop};
        fn flat(nodes: &[Node], out: &mut Vec<String>) {
            for n in nodes {
                match n {
                    Node::El(e) => {
                        out.push(format!("<{}|{}", e.name, e.start));
                        flat(&e.children, out);
                        out.push(">".into());
                    }
                    Node::Text(t) => out.push(format!("t{t}")),
                    Node::Raw(_) => {}
                }
            }
        }
        let (sheets, sst) = tricky("x:");
        let small = "<a><b/>x<![CDATA[<c>]]><?p <d>?><!-- <e> --><f g='>'>y</f></a><h/>";
        for s in [sheets[0].as_str(), sst.as_str(), small] {
            let mut want = Vec::new();
            flat(&parse(s).unwrap(), &mut want);
            let mut got = Vec::new();
            let r: Result<(), ScanStop<()>> = scan(s, &mut |ev| {
                got.push(match ev {
                    Ev::Start(n, raw) => format!("<{n}|{raw}"),
                    Ev::End => ">".into(),
                    Ev::Text(t) => format!("t{t}"),
                });
                Ok(())
            });
            assert!(r.is_ok());
            assert_eq!(got, want);
        }
    }
}

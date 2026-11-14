//! Resource limit test: the memory one rich string takes while its inlines are built. One test per file, because
//! the counting allocator sees every thread of the test binary.

use bilingua_model::{ExtractOptions, Filter, Inline};
use bilingua_xlsx::Xlsx;
use bilingua_zip::{write_zip, EntryMeta};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Counts live heap bytes and their peak, so a test can see how much memory one call takes.
struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            let now = LIVE.fetch_add(l.size(), Ordering::Relaxed) + l.size();
            PEAK.fetch_max(now, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) };
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

/// Peak extra heap while `f` runs, in bytes.
fn peak_during<T>(f: impl FnOnce() -> T) -> (T, usize) {
    let base = LIVE.load(Ordering::Relaxed);
    PEAK.store(base, Ordering::Relaxed);
    let r = f();
    (r, PEAK.load(Ordering::Relaxed).saturating_sub(base))
}

const NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PKG_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

/// A one-sheet workbook whose A1 is an inline string with the runs `runs`.
fn workbook(runs: &str) -> Vec<u8> {
    let ct = "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/></Types>".to_string();
    let root = format!("<Relationships xmlns=\"{PKG_REL}\"><Relationship Id=\"rId1\" Type=\"{R}/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>");
    let wb = format!("<workbook xmlns=\"{NS}\" xmlns:r=\"{R}\"><sheets><sheet name=\"Sheet1\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>");
    let wrels = format!("<Relationships xmlns=\"{PKG_REL}\"><Relationship Id=\"rId1\" Type=\"{R}/worksheet\" Target=\"worksheets/sheet1.xml\"/></Relationships>");
    let sheet = format!("<worksheet xmlns=\"{NS}\"><sheetData><row r=\"1\"><c r=\"A1\" t=\"inlineStr\"><is>{runs}</is></c></row></sheetData></worksheet>");
    let parts: Vec<(EntryMeta, Vec<u8>)> = [
        ("[Content_Types].xml", ct),
        ("_rels/.rels", root),
        ("xl/workbook.xml", wb),
        ("xl/_rels/workbook.xml.rels", wrels),
        ("xl/worksheets/sheet1.xml", sheet),
    ]
    .into_iter()
    .map(|(n, d)| (EntryMeta { name: n.to_string(), stored: false, dir: false, modified: None }, d.into_bytes()))
    .collect();
    write_zip(&parts).unwrap()
}

/// One run with a 40 KB run property, then 20,000 empty runs: each empty run took a copy of the run property and
/// its key (about 1.6 GB) before the runs were merged. Now each run points at the run whose formatting it uses.
#[test]
fn empty_runs_do_not_copy_a_long_run_property() {
    let rpr = format!("<rPr><b/><rFont val=\"{}\"/></rPr>", "F".repeat(40_000));
    let runs = format!("<r>{rpr}<t>a</t></r>{}", "<r><t></t></r>".repeat(20_000));
    let bytes = workbook(&runs);
    let (r, peak) = peak_during(|| Xlsx::default().extract(&bytes, &ExtractOptions::default()));
    let doc = r.expect("extract");
    assert_eq!(doc.units.len(), 1);
    let text: String = doc.units[0].source().iter().filter_map(|i| if let Inline::Text(t) = i { Some(t.as_str()) } else { None }).collect();
    assert_eq!(text, "a");
    assert!(peak < 200 * 1024 * 1024, "peak extra heap {peak} bytes for a {} byte cell", runs.len());
}

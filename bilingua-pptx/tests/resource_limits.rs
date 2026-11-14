//! Resource limit test: the memory one paragraph takes while its inlines are built. One test per file, because
//! the counting allocator sees every thread of the test binary.

use bilingua_model::{ExtractOptions, Filter, Inline};
use bilingua_pptx::Pptx;
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

const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PKG_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";

/// A one-slide deck whose one paragraph holds `inner`.
fn deck(inner: &str) -> Vec<u8> {
    let ct = "<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/><Override PartName=\"/ppt/slides/slide1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/></Types>".to_string();
    let root = format!("<Relationships xmlns=\"{PKG_REL}\"><Relationship Id=\"rId1\" Type=\"{R}/officeDocument\" Target=\"ppt/presentation.xml\"/></Relationships>");
    let pres = format!("<p:presentation xmlns:r=\"{R}\" xmlns:p=\"{P}\"><p:sldIdLst><p:sldId id=\"256\" r:id=\"rId1\"/></p:sldIdLst></p:presentation>");
    let prels = format!("<Relationships xmlns=\"{PKG_REL}\"><Relationship Id=\"rId1\" Type=\"{R}/slide\" Target=\"slides/slide1.xml\"/></Relationships>");
    let slide = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<p:sld xmlns:a=\"{A}\" xmlns:p=\"{P}\" xmlns:r=\"{R}\"><p:cSld><p:spTree><p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"S\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:p>{inner}</a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"
    );
    let parts: Vec<(EntryMeta, Vec<u8>)> = [
        ("[Content_Types].xml", ct),
        ("_rels/.rels", root),
        ("ppt/presentation.xml", pres),
        ("ppt/_rels/presentation.xml.rels", prels),
        ("ppt/slides/slide1.xml", slide),
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
    let rpr = format!("<a:rPr lang=\"en-US\" b=\"1\"><a:latin typeface=\"{}\"/></a:rPr>", "F".repeat(40_000));
    let inner = format!("<a:r>{rpr}<a:t>a</a:t></a:r>{}", "<a:r><a:t></a:t></a:r>".repeat(20_000));
    let bytes = deck(&inner);
    let (r, peak) = peak_during(|| Pptx.extract(&bytes, &ExtractOptions::default()));
    let doc = r.expect("extract");
    assert_eq!(doc.units.len(), 1);
    let text: String = doc.units[0].source().iter().filter_map(|i| if let Inline::Text(t) = i { Some(t.as_str()) } else { None }).collect();
    assert_eq!(text, "a");
    assert!(peak < 200 * 1024 * 1024, "peak extra heap {peak} bytes for a {} byte slide", inner.len());
}

//! Run 2 G5-1: the last heading is copied into every later unit. Each copy is now charged to the context budget
//! before it is made, so a long heading over many paragraphs is refused before the copies are allocated (the check
//! of the finished document came after all of them).
mod common;
// A counting allocator: the test measures the bytes the extraction asks for, not its time. One test per file, so
// no other test allocates at the same time.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Counting;
static ASKED: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        ASKED.fetch_add(l.size(), Ordering::Relaxed);
        System.alloc(l)
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        System.dealloc(p, l)
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        ASKED.fetch_add(n.saturating_sub(l.size()), Ordering::Relaxed);
        System.realloc(p, l, n)
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

/// The bytes `f` asks the allocator for.
fn asked<R>(f: impl FnOnce() -> R) -> (R, usize) {
    let before = ASKED.load(Ordering::Relaxed);
    let r = f();
    (r, ASKED.load(Ordering::Relaxed) - before)
}

fn context_refusal<T>(r: Result<T, bilingua_model::Error>) {
    match r {
        Err(bilingua_model::Error::Limit(e)) => assert_eq!(e.kind, bilingua_model::LimitKind::Context, "{e}"),
        Err(e) => panic!("expected the context limit, got {e}"),
        Ok(_) => panic!("expected the context limit, the file was accepted"),
    }
}

/// Context budget of these tests: 4 MB. A 90 000-letter heading over 5 000 paragraphs is 450 MB of copies.
const BUDGET: u64 = 4 << 20;
const HEADING: usize = 90_000;
const PARAS: usize = 5_000;
/// Far above what the extraction needs once the copies are charged, far below the 450 MB of copies.
const MOST_ASKED: usize = 120 << 20;

fn heading() -> String {
    "a".repeat(HEADING)
}

use bilingua_model::{ExtractOptions, Filter, Limits};
use bilingua_odf::Odt;
use common::{Pkg, NSDECL};

fn odt(h: &str, paras: usize) -> Vec<u8> {
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<office:document-content {NSDECL}><office:body><office:text><text:h text:outline-level=\"1\">{h}</text:h>{}</office:text></office:body></office:document-content>",
        "<text:p>x</text:p>".repeat(paras)
    );
    Pkg::new(content).build()
}

#[test]
fn a_long_heading_over_many_paragraphs_is_refused_before_copying() {
    let opts = ExtractOptions::new().with_limits(Limits::new().with_max_context_bytes(BUDGET));
    let bytes = odt(&heading(), PARAS);
    let (r, n) = asked(|| Odt::default().extract(&bytes, &opts));
    context_refusal(r);
    assert!(n < MOST_ASKED, "asked for {n} bytes");
    let d = Odt::default().extract(&odt("Top", 2), &opts).unwrap();
    assert_eq!(d.units.iter().filter(|u| u.context.heading.as_deref() == Some("Top")).count(), 2);
}

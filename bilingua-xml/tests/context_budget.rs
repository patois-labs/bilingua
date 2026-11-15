//! Run 2 G5-1: one ITS note can apply to every element, and each unit carries it twice. Both copies are now charged
//! to the context budget before they are made, so a long note over many elements is refused before the copies are
//! allocated (the check of the finished document came after all of them, and did not count the second copy).
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
use bilingua_xml::XmlFilter;

fn its(note: &str, paras: usize) -> String {
    format!(
        "<doc xmlns:its=\"http://www.w3.org/2005/11/its\" its:version=\"2.0\"><its:rules version=\"2.0\"><its:locNoteRule selector=\"//p\" locNoteType=\"description\"><its:locNote>{note}</its:locNote></its:locNoteRule></its:rules>{}</doc>",
        "<p>x</p>".repeat(paras)
    )
}

/// The same note set as a local attribute on the root, which every element inherits.
fn local(note: &str, paras: usize) -> String {
    format!(
        "<doc xmlns:its=\"http://www.w3.org/2005/11/its\" its:version=\"2.0\" its:locNote=\"{note}\" its:locNoteType=\"description\">{}</doc>",
        "<p>x</p>".repeat(paras)
    )
}

#[test]
fn a_long_loc_note_rule_over_many_elements_is_refused() {
    let opts = ExtractOptions::new().with_limits(Limits::new().with_max_context_bytes(BUDGET));
    for (name, build) in [("rule", its as fn(&str, usize) -> String), ("local", local)] {
        let x = build(&heading(), PARAS);
        let (r, n) = asked(|| XmlFilter::new().extract(x.as_bytes(), &opts));
        context_refusal(r);
        assert!(n < MOST_ASKED, "{name}: asked for {n} bytes");
        let d = XmlFilter::new().extract(build("Mind the gap", 2).as_bytes(), &opts).unwrap();
        assert!(d.units.iter().filter(|u| u.context.notes.iter().any(|t| t == "Mind the gap")).count() >= 2, "{name}");
    }
}

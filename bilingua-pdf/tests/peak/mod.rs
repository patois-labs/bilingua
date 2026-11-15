//! A global allocator that records, per thread, the most bytes held at once: the memory tests of the resource
//! limit fixes assert that a hostile file is refused before the memory is taken, not only that it is refused.
#![allow(dead_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

struct Peak;

thread_local! {
    static NOW: Cell<isize> = const { Cell::new(0) };
    static TOP: Cell<isize> = const { Cell::new(0) };
}

fn add(n: isize) {
    let _ = NOW.try_with(|c| {
        let v = c.get() + n;
        c.set(v);
        let _ = TOP.try_with(|t| {
            if v > t.get() {
                t.set(v);
            }
        });
    });
}

// SAFETY: every call goes straight to the system allocator; only the byte counts are added.
unsafe impl GlobalAlloc for Peak {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            add(l.size() as isize);
        }
        p
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(l) };
        if !p.is_null() {
            add(l.size() as isize);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) };
        add(-(l.size() as isize));
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        let q = unsafe { System.realloc(p, l, new) };
        if !q.is_null() {
            add(new as isize - l.size() as isize);
        }
        q
    }
}

#[global_allocator]
static ALLOC: Peak = Peak;

/// Runs `f` and returns its result with the most bytes this thread held at once during the call, above what it
/// held when the call started.
pub fn peak<R>(f: impl FnOnce() -> R) -> (R, usize) {
    let base = NOW.with(Cell::get);
    TOP.with(|t| t.set(base));
    let r = f();
    let top = TOP.with(Cell::get);
    (r, (top - base).max(0) as usize)
}

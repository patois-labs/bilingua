//! A counter of scanning steps, for tests that must show a parser is linear without timing it. A parser calls
//! [`tick`] where it examines input (a character read by a scanner, a line taken); a test calls [`reset`], runs
//! the parser on an input of size n and on one of size 2n, and compares [`get`]: linear work doubles, quadratic
//! work quadruples, whatever the machine. The counter is per thread (tests run in parallel) and costs one add.
//! Not part of the semver API.

use std::cell::Cell;

thread_local! {
    static WORK: Cell<u64> = const { Cell::new(0) };
}

/// Adds `n` steps to this thread's counter.
#[inline]
pub fn tick(n: u64) {
    WORK.with(|w| w.set(w.get().wrapping_add(n)));
}

/// Sets this thread's counter to zero.
pub fn reset() {
    WORK.with(|w| w.set(0));
}

/// This thread's counter.
pub fn get() -> u64 {
    WORK.with(Cell::get)
}

/// Work done by `f`, from a zero counter.
pub fn count<R>(f: impl FnOnce() -> R) -> (R, u64) {
    reset();
    let r = f();
    (r, get())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_per_thread_and_resets() {
        let ((), w) = count(|| {
            tick(3);
            tick(4);
        });
        assert_eq!(w, 7);
        std::thread::spawn(|| assert_eq!(get(), 0)).join().unwrap();
        reset();
        assert_eq!(get(), 0);
    }
}

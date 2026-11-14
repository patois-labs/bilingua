//! A counter of scanning steps, for tests that must show a parser is linear without timing it. A parser calls
//! [`tick`] where it examines input (a character read by a scanner, a line taken); a test calls [`reset`], runs
//! the parser on an input of size n and on one of size 2n, and compares [`get`]: linear work doubles, quadratic
//! work quadruples, whatever the machine. The counter is per thread (tests run in parallel).
//!
//! It only counts with the `work-count` cargo feature, which no release build turns on (the format crates enable
//! it from their dev-dependencies, for their tests). Without it [`tick`] compiles to nothing and [`get`] is always
//! 0. Not part of the semver API.

#[cfg(feature = "work-count")]
thread_local! {
    static WORK: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Adds `n` steps to this thread's counter (nothing without the `work-count` feature).
#[cfg(feature = "work-count")]
#[inline]
pub fn tick(n: u64) {
    WORK.with(|w| w.set(w.get().wrapping_add(n)));
}

/// Adds `n` steps to this thread's counter (nothing without the `work-count` feature).
#[cfg(not(feature = "work-count"))]
#[inline(always)]
pub fn tick(_n: u64) {}

/// Sets this thread's counter to zero.
#[cfg(feature = "work-count")]
pub fn reset() {
    WORK.with(|w| w.set(0));
}

/// Sets this thread's counter to zero.
#[cfg(not(feature = "work-count"))]
#[inline(always)]
pub fn reset() {}

/// This thread's counter; always 0 without the `work-count` feature.
#[cfg(feature = "work-count")]
pub fn get() -> u64 {
    WORK.with(std::cell::Cell::get)
}

/// This thread's counter; always 0 without the `work-count` feature.
#[cfg(not(feature = "work-count"))]
#[inline(always)]
pub fn get() -> u64 {
    0
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

    #[cfg(feature = "work-count")]
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

    #[cfg(not(feature = "work-count"))]
    #[test]
    fn counts_nothing_without_the_feature() {
        let ((), w) = count(|| tick(3));
        assert_eq!(w, 0);
    }
}

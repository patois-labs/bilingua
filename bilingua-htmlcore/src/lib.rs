//! The HTML engine shared by `bilingua-html` and `bilingua-serverpages`: a token-for-token port of the Localization
//! OS `html_filter` (tokenizer, block units with inline tags, `translate="no"`, accessibility and metadata
//! attribute units, the target-language declaration) and of `html_guard` (what a browser makes of a built page,
//! and the export check built on it), with the Python string helpers they need.
//!
//! This crate is plumbing for Bilingua's own HTML-family filters and moves in lockstep with them; it is not part
//! of the semver API. Use `bilingua-html` (or a server-page filter) instead.

#[doc(hidden)]
pub mod browser;
#[doc(hidden)]
pub mod entities;
#[doc(hidden)]
pub mod html;
#[doc(hidden)]
pub mod pyx;
#[doc(hidden)]
pub mod xml;

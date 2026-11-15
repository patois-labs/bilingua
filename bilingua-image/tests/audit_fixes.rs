//! BILI-59: an SVG holding U+0000 (or another character XML forbids) is refused with a parse error, not read.

use bilingua_image::{ImageFilter, SvgFilter};
use bilingua_model::*;

#[test]
fn a_nul_character_is_refused() {
    let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\"><text>He\u{0}llo</text></svg>";
    assert!(matches!(SvgFilter.extract(svg.as_bytes(), &ExtractOptions::new()), Err(Error::Parse { .. })));
    assert!(ImageFilter.extract(svg.as_bytes(), &ExtractOptions::new()).is_err());
    let ok = svg.replace('\u{0}', "");
    assert_eq!(SvgFilter.extract(ok.as_bytes(), &ExtractOptions::new()).unwrap().units.len(), 1);
}

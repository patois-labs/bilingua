//! `.sdlxlf` is the short spelling of a Trados SDLXLIFF (LocOS registers it as an alias of `sdlxliff`); Bilingua
//! claims it too, so the extension alone picks the SDLXLIFF filter.

use bilingua::*;

const SDL: &[u8] = include_bytes!("../../bilingua-xliffpkg/tests/data/sample.sdlxliff");

#[test]
fn sdlxlf_is_an_sdlxliff() {
    for name in ["job.sdlxlf", "JOB.SDLXLF", "job.sdlxliff"] {
        assert_eq!(detect(name, SDL).unwrap().id(), "sdlxliff", "{name}");
    }
    let a = extract("job.sdlxlf", SDL, &ExtractOptions::default()).unwrap();
    let b = extract("job.sdlxliff", SDL, &ExtractOptions::default()).unwrap();
    assert_eq!(a.format, "sdlxliff");
    assert_eq!(a.units, b.units);
    assert_eq!(merge(&a).unwrap(), SDL, "an untouched merge gives the original bytes");
}

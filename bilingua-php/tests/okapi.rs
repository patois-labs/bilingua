//! Okapi `okf_phpcontent` on its own test resource (`test01.phpcnt`, Okapi 1.48.0, Apache 2.0).
//!
//! Okapi's filter pulls string literals out of any PHP code with a regular-expression scan. Bilingua (like LocOS) reads
//! only static language arrays and refuses the rest, so the two meet on files that are `$var[..] = '...'` lists. That
//! resource has no `<?php` line, which LocOS refuses; `data/okapi_test01_wrapped.php` is the same bytes after a
//! `<?php` line. The text units are then the same seven Okapi finds (after Okapi's whitespace normalisation).
//! The expected texts below are what `okapi_dump.py FILE --fc okf_phpcontent` prints.
mod common;
use common::*;

const OKAPI: &[&str] = &[
    "| A New Free Game Download Every Day",
    "text/html; charset=UTF-8",
    "games, download games, online games, free games, play games, pc games, mac games, puzzle games, mahjong games, arcade games, card games, word games, big fish games",
    "Free game downloads &amp; online games at Gandalf Games - A new game every day! PC games &amp; mac games - Play puzzle games, arcade games, mahjong games, card games, word games and more!",
    "Learn More",
    "Read more...",
    "This is the text inside the heredoc data // including this",
];

fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[test]
fn same_text_units_as_okapi_phpcontent() {
    let wrapped = include_bytes!("data/okapi_test01_wrapped.php");
    let mut got: Vec<String> = texts(wrapped).iter().map(|t| norm(t)).collect();
    // The one difference: LocOS reads a `|` as the Laravel plural separator (a placeholder), Okapi as plain text.
    // LocOS wins (the default behaviour), so the first unit starts with a tag token.
    assert_eq!(got[0], "\u{27e6}x1\u{27e7} A New Free Game Download Every Day");
    got[0] = got[0].replace("\u{27e6}x1\u{27e7}", "|");
    assert_eq!(got, OKAPI);
    // the original resource is not a PHP file with an open tag: refused, as LocOS refuses it
    assert!(extract(include_bytes!("data/okapi_test01.phpcnt")).is_err());
    assert_identity(wrapped);
    // the heredoc keeps its line breaks, CRLF as in the file (Okapi dump collapses them)
    let t = texts(wrapped);
    assert_eq!(t[6], "This is the text inside\r\nthe heredoc data\r\n// including this");
    // the text after the heredoc is a comment, not a unit
    assert!(!t.iter().any(|x| x.contains("not this")));
}

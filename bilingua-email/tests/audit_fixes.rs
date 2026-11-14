//! Regression tests for the 0.9.1 security audit.
use bilingua_email::Email;
use bilingua_model::*;
use std::time::{Duration, Instant};

/// A multipart/mixed message with one text/html part per page.
fn mail(pages: &[String]) -> Vec<u8> {
    let mut m = String::from("Subject: hello there\nMIME-Version: 1.0\nContent-Type: multipart/mixed; boundary=BB\n\n");
    for p in pages {
        m.push_str("--BB\nContent-Type: text/html; charset=utf-8\nContent-Transfer-Encoding: 8bit\n\n");
        m.push_str(p);
        m.push('\n');
    }
    m.push_str("--BB--\n");
    m.into_bytes()
}

/// Finding optional-end-stack-scan-quadratic, the email half: the HTML filter's markup allowance (ten `<` per
/// allowed segment) was given to every HTML part afresh. The parts of a message now share one allowance.
#[test]
fn html_parts_share_one_markup_allowance() {
    let limits = Limits::new().with_max_units(1000); // 10 000 `<` for the message
    let opts = ExtractOptions::new().with_limits(limits);
    let page = format!("<p>Hello{}world</p>", "<br>".repeat(4000));
    // one part and two parts fit, as before
    for parts in [1, 2] {
        let doc = Email.extract(&mail(&vec![page.clone(); parts]), &opts).unwrap_or_else(|e| panic!("{parts} parts: {e:?}"));
        assert_eq!(doc.units.len(), 1 + parts);
        assert_eq!(Email.merge(&doc).unwrap(), mail(&vec![page.clone(); parts]));
    }
    // three parts hold 12 000 `<` between them: each passes alone, the message does not
    match Email.extract(&mail(&vec![page.clone(); 3]), &opts) {
        Err(Error::Limit(e)) => assert_eq!(e.kind, LimitKind::Units, "{e:?}"),
        other => panic!("expected a limit error, got {:?}", other.map(|d| d.units.len())),
    }
    // a part that is too large on its own is refused by the HTML filter, as before
    let big = format!("<p>Hello{}world</p>", "<br>".repeat(10_001));
    assert!(matches!(Email.extract(&mail(&[big]), &opts), Err(Error::Limit(_))));
}

/// The pages of the two time-complexity findings as HTML parts of a message, at sizes that took seconds in a release
/// build (and far longer in a debug build) before the fixes.
#[test]
fn hostile_html_parts_open_in_linear_time() {
    let n = 20_000;
    let inline = format!("<p>{}{}Hello</p>", "<rt>".repeat(n), "</rp>".repeat(n));
    let server = format!("<html><body><a {}{}>x</a><p>Hello world</p></body></html>", "a=1 ".repeat(5000), "<% %> ".repeat(5000));
    let t = Instant::now();
    let doc = Email.extract(&mail(&[inline.clone(), inline, server]), &ExtractOptions::default()).unwrap();
    assert!(doc.units.len() >= 4, "{}", doc.units.len());
    let t = t.elapsed();
    assert!(t < Duration::from_secs(20), "took {t:?}");
}

//! A generated file of more than 5 MB. The real timing (release build, against the LocOS filter) is
//! `examples/xslt_tool.rs`; this test keeps the size and the round trip honest in every build.

mod common;

use bilingua_model::*;
use bilingua_xslt::{Limits, Xslt};
use common::*;

fn generate(min_bytes: usize) -> String {
    let words = ["fresh", "bread", "daily", "order", "pickup", "store", "table", "coffee", "sweet", "morning"];
    let mut s = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<xsl:stylesheet version=\"1.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\">\r\n");
    let mut i = 0usize;
    while s.len() < min_bytes {
        let w = |k: usize| words[(i * 7 + k * 3) % words.len()];
        s.push_str(&format!("  <xsl:template match=\"item{i}\">\r\n    <div class=\"c{i}\" title=\"Box {} {{@id}}\">\r\n", w(0)));
        s.push_str(&format!("      <h2>Our {} <b>{}</b> is ready, <xsl:value-of select=\"name{i}\"/>!</h2>\r\n", w(1), w(2)));
        s.push_str(&format!("      <p>Caf\u{e9} {} \u{65e5}\u{672c} &amp; {{0}} <a href=\"{{@u}}\" title=\"Open {}\">link {}</a> done.<!-- c{i} --></p>\r\n", w(3), w(4), w(5)));
        s.push_str(&format!("      <xsl:if test=\"count(x) &gt; {i}\"><xsl:text>A long text about {}</xsl:text></xsl:if>\r\n    </div>\r\n  </xsl:template>\r\n", w(1)));
        i += 1;
    }
    s.push_str("</xsl:stylesheet>\r\n");
    s
}

#[test]
fn five_megabytes_round_trip() {
    let data = generate(5_200_000).into_bytes();
    assert!(data.len() > 5_000_000);
    let limits = Limits::new().with_max_units(10_000_000).with_max_xml_elements(10_000_000);
    let mut d = Xslt.extract_with(&data, &opts(), &limits).unwrap();
    assert!(d.units.len() > 30_000);
    assert_eq!(Xslt.merge(&d).unwrap(), data);
    for u in &mut d.units {
        let mut t = vec![Inline::text("[")];
        t.extend(u.source());
        t.push(Inline::text("]"));
        u.segments[0].target = t;
    }
    let out = Xslt.merge(&d).unwrap();
    let back = Xslt.extract_with(&out, &opts(), &limits).unwrap();
    assert_eq!(back.units.len(), d.units.len());
}

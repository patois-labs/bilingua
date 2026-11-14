//! Speed: one generated page of at least 5 MB per dialect family, extracted and merged with every unit translated.
//! Run in release for the real number:
//!   cargo test -p bilingua-serverpages --release --test speed -- --nocapture
//! Set BILINGUA_SP_SPEED_DIR to a folder to keep the generated files (tests/tools/speed_locos.py reads them).
use bilingua_model::*;
use bilingua_serverpages::{Jsp, Phtml};
use std::time::Instant;

fn generate_php() -> String {
    let mut s = String::with_capacity(6_000_000);
    s.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head><title><?= $title ?></title><meta name=\"description\" content=\"Lumen Bakery generated page\"></head>\n<body>\n");
    let mut i = 0;
    while s.len() < 5_300_000 {
        match i % 7 {
            0 => s.push_str(&format!("<p>Welcome back, <?= htmlspecialchars($user) ?>! You have <b><?php echo $n; ?></b> new messages ({i}).</p>\n")),
            1 => s.push_str(&format!("<?php if ($show{i}): ?>\n<li>Click <a href=\"<?= url('x{i}') ?>\" title=\"Open the inbox {i}\">here</a> to continue with order number {i} today.</li>\n<?php endif; ?>\n")),
            2 => s.push_str(&format!("<h2>\u{65e5}\u{672c}\u{8a9e}\u{306e}\u{898b}\u{51fa}\u{3057} {i}</h2><p>It's a long sentence with &amp; entities, \"quotes\" and caf\u{e9} number {i}. Another sentence follows it.</p>\n")),
            3 => s.push_str(&format!("<img src=\"p{i}.png\" alt=\"Picture number {i} of the product\"><br>Caption for picture {i}<br/>\n")),
            4 => s.push_str(&format!("<script>var n{i} = <?= json_encode($n) ?>; if (a < b) {{ x(); }}</script>\n")),
            5 => s.push_str(&format!("<table><tr><td>Cell {i}</td><td><?= $cell{i} ?> total</td></tr></table>\n")),
            _ => s.push_str(&format!("<!-- comment {i} --><div class=\"x\"><span translate=\"no\">Brand {i}</span> is our name, say hello to <em>everyone</em> now.</div>\n")),
        }
        i += 1;
    }
    s.push_str("</body>\n</html>\n");
    s
}

fn generate_jsp() -> String {
    let mut s = String::with_capacity(6_000_000);
    s.push_str("<%@ page contentType=\"text/html; charset=UTF-8\" %>\n<html lang=\"en\"><body>\n");
    let mut i = 0;
    while s.len() < 5_300_000 {
        match i % 4 {
            0 => s.push_str(&format!("<p class=\"${{cls}}\">Welcome back, ${{user.name}}! Today is <%= date %>, item {i}.</p>\n")),
            1 => s.push_str(&format!("<% for (int i = 0; i < 3; i++) {{ %><li>Item number {i} of the list</li><% }} %>\n")),
            2 => s.push_str(&format!("<%-- comment {i} --%><h3>Heading {i}</h3><p>Click <a href=\"/x/{i}\" title=\"Go to {i}\">this link</a> please.</p>\n")),
            _ => s.push_str(&format!("<p>Price: \\${{not.el}} only, \u{e9}t\u{e9} {i}.</p><img src=\"a{i}.png\" alt=\"Alt text {i}\">\n")),
        }
        i += 1;
    }
    s.push_str("</body></html>\n");
    s
}

fn run<F: Filter>(name: &str, f: F, text: &str, ext: &str) {
    assert!(text.len() >= 5_000_000);
    if let Ok(d) = std::env::var("BILINGUA_SP_SPEED_DIR") {
        std::fs::write(std::path::Path::new(&d).join(format!("big.{ext}")), text).unwrap();
    }
    let opts = ExtractOptions::new().with_limits(Limits::new().with_max_units(10_000_000).with_max_input_bytes(1 << 30).with_max_lines(100_000_000).with_max_xml_elements(100_000_000));
    let t0 = Instant::now();
    let mut doc = f.extract(text.as_bytes(), &opts).unwrap();
    let t_extract = t0.elapsed();
    let n = doc.units.len();
    let t1 = Instant::now();
    for (i, u) in doc.units.iter_mut().enumerate() {
        let mut t = vec![Inline::text(format!("T{i} \"q\" translated "))];
        t.extend(u.source().into_iter().filter(|x| matches!(x, Inline::Tag { .. })));
        u.segments[0].target = t;
    }
    let out = f.merge(&doc).unwrap();
    let t_merge = t1.elapsed();
    let untouched = f.merge(&f.extract(text.as_bytes(), &opts).unwrap()).unwrap();
    assert_eq!(untouched, text.as_bytes());
    println!(
        "Bilingua {name}: {:.1} MB, {} units, extract {} ms, translate+merge {} ms, extract+merge {} ms",
        text.len() as f64 / 1e6,
        n,
        t_extract.as_millis(),
        t_merge.as_millis(),
        (t_extract + t_merge).as_millis()
    );
    let again = f.extract(&out, &opts).unwrap();
    assert_eq!(again.units.len(), n);
}

#[test]
fn large_php_page() {
    run("phtml", Phtml, &generate_php(), "phtml");
}

#[test]
fn large_jsp_page() {
    run("jsp", Jsp, &generate_jsp(), "jsp");
}

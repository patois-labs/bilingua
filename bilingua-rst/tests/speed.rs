//! Speed: one generated file of more than 5 MB, extracted and merged with every unit translated. Run in release
//! for the real number:
//!   cargo test -p bilingua-rst --release --test speed -- --nocapture
//! Set BILINGUA_RST_SPEED_OUT to a path to keep the generated file (tests/tools/speed_locos.py times LocOS on it).
mod common;
use bilingua_model::*;
use bilingua_rst::Rst;
use std::time::Instant;

pub fn generate() -> String {
    let mut s = String::with_capacity(5_600_000);
    s.push_str(".. meta::\n   :description: A generated manual for the speed test.\n\n");
    let mut i = 0usize;
    while s.len() < 5_300_000 {
        let t = format!("Chapter {i}: Using the bakery tools");
        s.push_str(&format!("{t}\n{}\n\n", "=".repeat(t.len())));
        s.push_str(&format!(
            "This is a *simple* paragraph about order {i} with ``code`` and a :ref:`label <x{i}>` role, spanning\ntwo lines with |sub| and a link_ and `Docs <https://example.com/{i}>`_. See also **the guide** [1]_.\n\n"
        ));
        s.push_str(&format!("Sub-section {i}\n--------------\n\n"));
        s.push_str(&format!(".. note:: Remember to save your work before you leave {i}.\n\n   A second note paragraph with more words.\n\n"));
        s.push_str(&format!(".. code-block:: python\n\n   print(\"hello {i}\")\n   x = [1, 2, 3]\n\n"));
        s.push_str(&format!("Example::\n\n    literal text {i}\n    not translated\n\n"));
        s.push_str(&format!("- First item {i}\n- Second item with **bold** text\n\n  Nested paragraph in the item.\n\n1. Numbered one\n#. Numbered two\n\n"));
        s.push_str(&format!("Term {i}\n   Definition of the term with `a link <http://x.org/{i}>`__.\n\n"));
        s.push_str(&format!(
            "+------------------------------------------+------------------------------------------+\n| Header one                               | Header two                               |\n+==========================================+==========================================+\n| Cell {i}                                  | Another cell                             |\n+------------------------------------------+------------------------------------------+\n\n"
        ));
        s.push_str(&format!("{b}  {c}
Name{p}  Value
{b}  {c}
Alpha{p2}  First value {i}
Beta{p}  Second value
{b}  {c}

", b = "=".repeat(24), c = "=".repeat(26), p = " ".repeat(20), p2 = " ".repeat(19)));
        s.push_str(&format!(".. _target{i}: http://example.com/{i}\n.. |sub| replace:: substituted\n.. [1] The footnote text {i}.\n\n:Author: Jane Doe\n\n"));
        i += 1;
    }
    s
}

#[test]
fn large_file() {
    let text = generate();
    assert!(text.len() > 5_000_000, "{}", text.len());
    let data = text.as_bytes().to_vec();
    if let Ok(p) = std::env::var("BILINGUA_RST_SPEED_OUT") {
        std::fs::write(p, &data).unwrap();
    }
    let limits = Limits::new().with_max_units(10_000_000).with_max_lines(100_000_000);
    let opts = ExtractOptions::new().with_limits(limits);
    let t0 = Instant::now();
    let mut doc = Rst.extract(&data, &opts).unwrap();
    let t_extract = t0.elapsed();
    let n = doc.units.len();
    assert!(n > 30_000, "{n}");
    assert_eq!(Rst.merge(&doc).unwrap(), data, "an untouched merge is the original");
    let t1 = Instant::now();
    for (i, u) in doc.units.iter_mut().enumerate() {
        let mut t = vec![Inline::text(format!("T{i} "))];
        t.extend(u.source());
        u.segments[0].target = t;
    }
    let out = Rst.merge(&doc).unwrap();
    let t_merge = t1.elapsed();
    assert!(out.len() > 5_000_000);
    let back = Rst.extract(&out, &opts).unwrap();
    assert_eq!(back.units.len(), n);
    println!(
        "rst speed: {:.1} MB, {n} units; extract {:.2} s, translated merge {:.2} s",
        data.len() as f64 / 1e6,
        t_extract.as_secs_f64(),
        t_merge.as_secs_f64()
    );
}

//! Speed: one generated file of at least 5 MB, extracted and merged with every unit translated.
//! Run in release for the real number:
//!   cargo test -p bilingua-mdx --release --test speed -- --nocapture
//! Set BILINGUA_MDX_SPEED_OUT to a path to keep the generated file (tests/tools/speed_locos.py reads it).
mod common;
use bilingua_mdx::Mdx;
use bilingua_model::*;
use std::time::Instant;

pub fn generate() -> String {
    let mut s = String::with_capacity(6_000_000);
    s.push_str("---\ntitle: Lumen Bakery handbook\ndescription: \"Generated chapters for the speed test\"\n---\n\nimport { Kbd, Note } from '../components/ui.js'\nexport const meta = {\n  author: 'Ada'\n}\n\n");
    let mut i = 0;
    while s.len() < 5_300_000 {
        match i % 8 {
            0 => s.push_str(&format!("## Chapter {i}: getting started with the oven\n\n")),
            1 => s.push_str(&format!("Press <Kbd>Ctrl</Kbd> + <Kbd>S</Kbd> to save order number {i} and read the {{props.count}} messages that **wait** for you.\n\n")),
            2 => s.push_str(&format!("- Use `{{braces}}` in code freely ({i}).\n- A literal \\{{ brace and a [link](https://example.org/page_{i}) stay.\n- \u{65e5}\u{672c}\u{8a9e}\u{306e}\u{30c6}\u{30ad}\u{30b9}\u{30c8} {i} \u{3067}\u{3059}\u{3002}\n\n")),
            3 => s.push_str(&format!("<Note title=\"Before you begin {i}\" type=\"info\">\nMake sure the oven is *hot* first, entry {i}.\n</Note>\n\n")),
            4 => s.push_str(&format!("| Item | Count |\n|---|---|\n| Loaf {i} | {i} |\n\n")),
            5 => s.push_str(&format!("```js\nconst x{i} = <Hidden>Not translated</Hidden>\n```\n\n")),
            6 => s.push_str(&format!("The <Abbr title=\"Application interface {i}\">API</Abbr> is stable, see [ref{i}] and ![alt text {i}](/img/{i}.png).\n\n[ref{i}]: https://example.org/{i} \"Title {i}\"\n\n")),
            _ => s.push_str(&format!("{{/* comment {i} */}}\n\n<Chart\n  data={{[1, 2, {{a: '}}'}}]}}\n  label=\"Monthly sales {i}\"\n/>\n\n> Quoted line number {i} with a very ordinary sentence in it.\n\n")),
        }
        i += 1;
    }
    s
}

#[test]
fn large_file() {
    let text = generate();
    assert!(text.len() >= 5_000_000);
    if let Ok(p) = std::env::var("BILINGUA_MDX_SPEED_OUT") {
        std::fs::write(p, &text).unwrap();
    }
    let opts = ExtractOptions { limits: Some(Limits { max_units: 10_000_000, max_lines: 100_000_000, ..Limits::default() }), ..Default::default() };
    let t0 = Instant::now();
    let mut doc = Mdx.extract(text.as_bytes(), &opts).unwrap();
    let t_extract = t0.elapsed();
    let n = doc.units.len();
    let t1 = Instant::now();
    for (i, u) in doc.units.iter_mut().enumerate() {
        u.segments[0].target = vec![Inline::text(format!("T{i} \"q\" translated"))];
    }
    let out = Mdx.merge(&doc).unwrap();
    let t_merge = t1.elapsed();
    let untouched = Mdx.merge(&Mdx.extract(text.as_bytes(), &opts).unwrap()).unwrap();
    assert_eq!(untouched, text.as_bytes());
    println!(
        "Bilingua mdx: {:.1} MB, {} units, extract {} ms, translate+merge {} ms, extract+merge {} ms",
        text.len() as f64 / 1e6,
        n,
        t_extract.as_millis(),
        t_merge.as_millis(),
        (t_extract + t_merge).as_millis()
    );
    // The translation replaced every line by plain text, so the attributes that sat in tags it dropped are gone
    // (6075 `<Abbr title>` here); everything else reads back.
    let again = Mdx.extract(&out, &opts).unwrap();
    assert!(again.units.len() <= n && again.units.len() * 100 >= n * 90, "{} units read back of {n}", again.units.len());
    // with the tags kept (echo plus a mark) every unit and attribute survives
    let mut doc2 = Mdx.extract(text.as_bytes(), &opts).unwrap();
    for u in doc2.units.iter_mut() {
        let mut t = vec![Inline::text("\u{ab}")];
        t.extend(u.source());
        u.segments[0].target = t;
    }
    let out2 = Mdx.merge(&doc2).unwrap();
    assert_eq!(Mdx.extract(&out2, &opts).unwrap().units.len(), n);
}

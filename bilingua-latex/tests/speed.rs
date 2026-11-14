//! Speed: one generated file of at least 5 MB, extracted and merged with every unit translated. Run in release:
//!   cargo test -p bilingua-latex --release --test speed -- --nocapture
//! Set BILINGUA_LATEX_SPEED_OUT to a path to keep the generated file (tests/tools/speed_locos.py times LocOS on it).
use bilingua_latex::Latex;
use bilingua_model::*;
use std::time::Instant;

pub fn generate() -> String {
    let mut s = String::with_capacity(5_500_000);
    s.push_str("\\documentclass{article}\n\\usepackage[utf8]{inputenc}\n\\title{Lumen Bakery generated}\n\\begin{document}\n\\maketitle\n");
    let mut i = 0usize;
    while s.len() < 5_200_000 {
        s.push_str(&format!(
            "\\section{{Chapter {i} on \\emph{{fresh}} bread}}\\label{{sec:{i}}}\n\
Fresh bread number {i} is ready at \\textbf{{six}} every morning, see Section~\\ref{{sec:{i}}} and \\cite[p.~3]{{baker{i}}}. \
Costs are 10\\% of R\\&D; math $E = mc^{{2}}$ stays. % comment {i}\n\
The second sentence of paragraph {i} has a footnote\\footnote{{Note {i} with {{braces}}.}} and a link \\href{{https://lumen.example/{i}}}{{the shop}}.\n\n\
\\begin{{itemize}}\n  \\item First item {i} with \\textit{{style}}.\n  \\item[Label {i}] Second item.\n\\end{{itemize}}\n\
\\begin{{equation}}\n  a_{{{i}}}^2 + b^2 = c^2\n\\end{{equation}}\n\
\\begin{{tabular}}{{|l|r|}}\n\\hline\nName {i} & Value \\\\\nApple & Red fruit \\\\ \\hline\n\\end{{tabular}}\n\n"
        ));
        i += 1;
    }
    s.push_str("\\end{document}\n");
    s
}

#[test]
fn large_file() {
    let data = generate().into_bytes();
    assert!(data.len() >= 5_000_000, "{}", data.len());
    if let Ok(p) = std::env::var("BILINGUA_LATEX_SPEED_OUT") {
        std::fs::write(p, &data).unwrap();
    }
    let limits = Limits::new().with_max_units(10_000_000).with_max_lines(100_000_000);
    let opts = ExtractOptions::new().with_limits(limits).with_target_lang("de-DE");
    let f = Latex::default();
    let t0 = Instant::now();
    let mut doc = f.extract(&data, &opts).unwrap();
    let t_extract = t0.elapsed();
    let n = doc.units.len();
    assert!(n > 20_000, "{n}");
    assert_eq!(f.merge(&doc).unwrap(), data);
    let t1 = Instant::now();
    for (i, u) in doc.units.iter_mut().enumerate() {
        let mut t = vec![Inline::text(format!("T{i} q "))];
        t.extend(u.source());
        u.segments[0].target = t;
    }
    let out = f.merge(&doc).unwrap();
    let t_merge = t1.elapsed();
    assert!(out.len() > 1_000_000);
    let back = f.extract(&out, &opts).unwrap();
    assert_eq!(back.units.len(), n);
    println!(
        "latex speed: {:.1} MB, {n} units, extract {:.0} ms, translate+merge {:.0} ms, total {:.0} ms",
        data.len() as f64 / 1e6,
        t_extract.as_secs_f64() * 1e3,
        t_merge.as_secs_f64() * 1e3,
        (t_extract + t_merge).as_secs_f64() * 1e3
    );
}

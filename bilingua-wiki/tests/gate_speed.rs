//! Gate: adversarial 1 MB+ inputs for every hand-written scanner, timed at 1.1 MB and 4.4 MB in release (a quarter in
//! debug); a quadratic path would grow x16, a linear one x4. Run in release for the real numbers:
//!   cargo test -p bilingua-wiki --release --test gate_speed -- --nocapture
mod common;
use bilingua_model::*;
use common::*;
use std::time::{Duration, Instant};

fn rep(unit: &str, bytes: usize) -> String {
    unit.repeat(bytes / unit.len() + 1)
}

type Gen = Box<dyn Fn(usize) -> String>;

fn cases() -> Vec<(&'static str, bool, Gen)> {
    let line = |u: &'static str| -> Gen { Box::new(move |n| rep(u, n)) };
    vec![
        ("mw unclosed ''", false, line("''a ")),
        ("mw ''' runs", false, line("'''x''")),
        ("mw [[a only", false, line("[[a")),
        ("mw [[a| nested", false, Box::new(|n| format!("{}{}", rep("[[a|", n), rep("]]", n / 2)))),
        ("mw ]]a only", false, line("]]a")),
        ("mw {{a only", false, line("{{a")),
        ("mw {{a| unclosed", false, line("{{a|b=")),
        ("mw }}a only", false, line("}}a")),
        ("mw {{{ }}} nest", false, Box::new(|n| format!("{}{}", rep("{{{", n / 2), rep("}}}", n / 2)))),
        ("mw [// label", false, line("[//a ")),
        ("mw [http: no close", false, line("[http://x ")),
        ("mw http run", false, line("http://a")),
        ("mw <nowiki> unclosed", false, line("<nowiki>a")),
        ("mw </nowiki no >", false, Box::new(|n| format!("<nowiki>{}", rep("</nowiki ", n)))),
        ("mw <pre + spaces", false, line("<pre                                                            ")),
        ("mw every verbatim open", false, line("<nowiki><pre><code><math><source><gallery><html><ce><chem>a")),
        ("mw <!-- unclosed", false, Box::new(|n| format!("<!--{}", rep("a-", n)))),
        ("mw <a attrs", false, line("<a b ")),
        ("mw & entities", false, line("&#12345&x")),
        ("mw __A runs", false, line("__ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789ABCDEF_")),
        ("mw ~~ runs", false, line("~~a")),
        ("mw [[Category: long", false, line("[[Category:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")),
        ("mw table lines", false, Box::new(|n| format!("{{|\n{}|}}\n", rep("| a || b [[c|d]] || {{e}} ||\n", n)))),
        ("mw header cells", false, Box::new(|n| format!("{{|\n{}|}}\n", rep("! a !! b !! c\n", n)))),
        ("mw ; terms", false, line(";a [[b]] c\n")),
        ("mw = lines", false, line("=====a\n")),
        ("mw one long paragraph", false, line("word [[x|y]] '''b''' ")),
        ("mw templates with params", false, line("{{Quote|text=a b|author=c}} ")),
        ("dw [[a unclosed", true, line("[[a")),
        ("dw ((a unclosed", true, line("((a")),
        ("dw {{ unclosed", true, line("{{a")),
        ("dw ** unclosed", true, line("**a")),
        ("dw // unclosed", true, line("//a")),
        ("dw __ unclosed", true, line("__a")),
        ("dw '' unclosed", true, line("''a")),
        ("dw %% unclosed", true, Box::new(|n| format!("%%{}", rep("a%", n)))),
        ("dw <code> unclosed", true, line("<code>a")),
        ("dw <code + text", true, line("<code aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")),
        ("dw ~~A runs", true, line("~~ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789ABCDEF:")),
        ("dw <a@ runs", true, line("<aaaaaaaa@")),
        ("dw table cells", true, line("| a | b [[c|d]] | e |\n")),
        ("dw quotes", true, line(">> a //b//\n")),
        ("dw lists", true, line("  * a **b**\n")),
        ("dw one long paragraph", true, line("word [[x|y]] **b** ")),
    ]
}

fn time(f: &dyn Filter, data: &[u8]) -> (Duration, Option<usize>) {
    let limits = Limits::new().with_max_units(10_000_000).with_max_lines(100_000_000).with_max_unit_chars(10_000_000);
    let opts = ExtractOptions::new().with_limits(limits);
    let t = Instant::now();
    let r = f.extract(data, &opts);
    let n = match r {
        Ok(mut doc) => {
            let n = doc.units.len();
            for u in doc.units.iter_mut().filter(|u| u.translate) {
                let mut t = vec![Inline::text("Z ")];
                t.extend(u.source());
                u.segments[0].target = t;
            }
            f.merge(&doc).unwrap();
            Some(n)
        }
        Err(_) => None,
    };
    // the default limits too: they must refuse fast
    let _ = f.extract(data, &ExtractOptions::default());
    (t.elapsed(), n)
}

#[test]
fn adversarial_inputs_are_linear() {
    let mut bad = Vec::new();
    // 1.1 MB and 4x that in release; a quarter of both in debug (a workspace run). Linear is x4, quadratic x16: the
    // x8 bound holds on a busy machine, where a x2 step showed x4 from noise alone.
    let small = if cfg!(debug_assertions) { 275_000 } else { 1_100_000 };
    for (name, dwk, gen) in cases() {
        let f: Box<dyn Filter> = if dwk { Box::new(dw()) } else { Box::new(mw()) };
        let a = gen(small);
        let b = gen(4 * small);
        assert!(a.len() >= small);
        let (ta, na) = time(f.as_ref(), a.as_bytes());
        let (tb, _) = time(f.as_ref(), b.as_bytes());
        let ratio = tb.as_secs_f64() / ta.as_secs_f64().max(1e-3);
        println!(
            "{name:28} {:.2} MB {:7.0} ms  {:.2} MB {:7.0} ms  x{ratio:.1}  units {na:?}",
            a.len() as f64 / 1e6,
            ta.as_secs_f64() * 1e3,
            b.len() as f64 / 1e6,
            tb.as_secs_f64() * 1e3
        );
        if tb > Duration::from_millis(300) && ratio > 8.0 {
            bad.push(format!("{name}: x{ratio:.1}"));
        }
        if tb > Duration::from_secs(if cfg!(debug_assertions) { 120 } else { 30 }) {
            bad.push(format!("{name}: {tb:?} at the larger size"));
        }
    }
    assert!(bad.is_empty(), "{bad:?}");
}

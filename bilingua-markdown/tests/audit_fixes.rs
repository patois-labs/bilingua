//! Regression tests for run 2 of the security review (Markdown).

use bilingua_markdown::MarkdownFilter;
use bilingua_model::*;
use std::time::Instant;

fn locos() -> ExtractOptions {
    ExtractOptions::new().with_profile("locos")
}

/// Seconds `f` takes.
fn secs(f: impl FnOnce()) -> f64 {
    let t = Instant::now();
    f();
    t.elapsed().as_secs_f64()
}

/// G3-5: many text lines and then many images that make no unit of their own. Each image used to scan every unit
/// for the one on its line before anchoring on the last one before it.
#[test]
fn locos_note_images_linear() {
    let n = 40_000;
    let lines: String = (0..n).map(|i| format!("line {i}\n\n")).collect();
    let attack = format!("{lines}{}", "![]()\n\n".repeat(n));
    let control = format!("{lines}{}", "`x`\n\n".repeat(n));
    let c = secs(|| {
        let _ = MarkdownFilter.extract(control.as_bytes(), &locos());
    });
    let a = secs(|| {
        let _ = MarkdownFilter.extract(attack.as_bytes(), &locos());
    });
    assert!(a < c * 5.0 + 3.0, "images {a:.2} s, control {c:.2} s");
    // the images still land on the unit of their line, else on the nearest one before
    let doc = MarkdownFilter.extract(b"Intro\n\nSee ![a](x.png) here\n\n![](y.png)\n", &locos()).unwrap();
    let counts: Vec<usize> = doc.units.iter().map(|u| u.context.extra.get(profile::A11Y_IMAGES).map_or(0, |n| n.parse().unwrap())).collect();
    assert_eq!(counts[0], 0, "{counts:?}");
    assert_eq!(counts.iter().sum::<usize>(), 2, "{counts:?}");
}

/// G3-6: a target that repeats a source URL back to back with no blank: for each match the repair used to count
/// to the end of the run. It now looks the end of the run up.
#[test]
fn autolinks_glued_repeat_linear() {
    let url = "https://example.com/x";
    let n = 15_000;
    let mut doc = MarkdownFilter.extract(format!("See {url} now\n").as_bytes(), &locos()).unwrap();
    let id = doc.units[0].segments[0].id.clone();
    let mut run = |target: String| {
        doc.set_target(&id, vec![Inline::text(target)]).unwrap();
        let t = Instant::now();
        let out = MarkdownFilter.merge(&doc).unwrap();
        (t.elapsed().as_secs_f64(), String::from_utf8(out).unwrap())
    };
    let (c, _) = run(format!("{url} ").repeat(n));
    let (a, out) = run(format!("見て{}", url.repeat(n)));
    assert!(a < c * 5.0 + 3.0, "glued {a:.2} s, control {c:.2} s");
    assert!(out.starts_with(&format!("見て<{url}>")), "{:.80}", out);
    // the repair is unchanged for an ordinary glued URL
    let (_, one) = run(format!("{url}を見て"));
    assert_eq!(one, format!("<{url}>を見て\n"));
}

/// G3-7: a plain front-matter value whose translation holds a line break a YAML 1.1 reader knows (NEL, LS, PS), a
/// tab after a colon, or a control character is written in double quotes, so it stays one value.
#[test]
fn native_front_matter_unicode_break_stays_one_value() {
    let src = "---\ntitle: Hello\n---\n\nBody text\n";
    for t in ["Hola\u{2028}evil:\tyes", "Hola\u{85}evil: yes", "Hola\u{2029}evil:x", "Hola:\tevil", "Hola\tadi\u{f3}s", "Hola\t#x", "Hola\u{1}x"] {
        let mut doc = MarkdownFilter.extract(src.as_bytes(), &ExtractOptions::new()).unwrap();
        let id = doc.units.iter().find(|u| plain_text(&u.source()) == "Hello").unwrap().segments[0].id.clone();
        doc.set_target(&id, vec![Inline::text(t)]).unwrap();
        let out = String::from_utf8(MarkdownFilter.merge(&doc).unwrap()).unwrap();
        let line = out.lines().nth(1).unwrap();
        assert!(line.starts_with("title: \"") && line.ends_with('"'), "{t:?} written as {line:?}");
        assert!(!line.contains(['\u{85}', '\u{2028}', '\u{2029}']), "{line:?}");
        assert_eq!(out.lines().count(), src.lines().count(), "{out:?}");
    }
    // an ordinary value stays plain
    let mut doc = MarkdownFilter.extract(src.as_bytes(), &ExtractOptions::new()).unwrap();
    let id = doc.units.iter().find(|u| plain_text(&u.source()) == "Hello").unwrap().segments[0].id.clone();
    doc.set_target(&id, vec![Inline::text("Hola mundo")]).unwrap();
    assert!(String::from_utf8(MarkdownFilter.merge(&doc).unwrap()).unwrap().starts_with("---\ntitle: Hola mundo\n"));
}

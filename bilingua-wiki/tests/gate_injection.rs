//! Gate: a translation can never create markup. Hostile translated words go into every unit of well-formed pages in
//! both dialects; the merged page must read back with the same units, the same text between them, and no code in any
//! unit that the source did not have (the `<nowiki>` a translation needs is the only new code).
mod common;
use bilingua_model::*;
use common::*;
use proptest::prelude::*;

const MW_PAGES: &[&str] = &[
    "== Heading one ==\nA paragraph with '''bold''' and ''italic'' and [[Target|a label]] here.\n\n* item one\n*item two\n# numbered\n; term : definition text\n:indented words\n\n{| class=\"x\"\n|+ The caption\n|-\n! Head one !! Head two\n|-\n| cell one || cell two\n|style=\"a\"| cell three\n|cell four\n|}\n\n{{Quote|text=Quoted words|author=A}}\nEnd [https://x.org link words] and https://example.com/a more.\n",
    "Plain text line\ncontinued line\n\n=== Sub ===\nWords with <ref>a note</ref> and &nbsp; and {{tpl}} words.\n----\n[[File:a.png|thumb|A caption here]]\n",
    "{|\n|a\nparagraph in a cell\n|}\n",
    ";Term only\n;Term:def\n",
    "Words {{Note|Remember this}} here and [[File:b.png|thumb|left|A caption with [[Link|words]] in it]] and [[a|b]].\n* [[x|y]] ''it'' '''bo''' ~~~~ __TOC__ &amp; <b>t</b> z\n{|\n!h1!!h2\n|c1||c2\n|}\n",
];

const DW_PAGES: &[&str] = &[
    "====== Heading ======\nA paragraph with **bold** and //italic// and __under__ and [[ns:page|a label]] here.\n\n  * item one\n  - item two\n\n^ Head one ^ Head two ^\n| cell one | cell two |\n|cell three|cell four|\n\n> quoted words\n>no space quote\n\nText with ((a footnote)) and {{img.png|a caption}} and https://x.org/a here.\n",
    "Plain line\nnext line\n\n== Small ==\nWords ''mono'' and \\\\ break and words.\n",
    "^h1^h2^\n|c1|c2|\n[[a|b]] {{m.png|cap}} ((fn **b** x)) //i// __u__ <del>d</del> https://x.org z\n",
];

const MW_EVIL: &[&str] = &[
    "[[X]]", "]]", "[[", "{{T}}", "{{", "}}", "{{{1}}}", "{|", "|}", "|", "||", "!!", "!", "=", "== H ==", "*", "#", ":", ";", "-", "----",
    "+", "}", "{", "[", "]", "'", "''", "'''", "~", "~~~", "~~~~", "_", "__TOC__", "__", "<", ">", "<nowiki>", "</nowiki>", "<pre>",
    "<b>", "</b>", "<!--", "-->", "&", "&amp;", "[http://e.vil x]", "[//e.vil x]", "http://e.vil", "\n", "\n*", "\n=", "\n{|", "\n ",
    " ", "words", "\u{65e5}\u{672c}", "Category:", "[[Category:X]]", "#REDIRECT [[X]]",
];

const DW_EVIL: &[&str] = &[
    "[[X]]", "]]", "[[", "{{x.png}}", "{{", "}}", "((", "))", "(", ")", "**", "*", "//", "/", "__", "_", "''", "'", "%%", "%", "\\\\", "\\",
    "~~", "~~NOTOC~~", "~", "|", "^", ":::", ">", "> ", "=", "==", "== H ==", "-", "----", "  * ", "<", "<code>", "</code>", "<html>",
    "<nowiki>", "</nowiki>", "<del>", "http://e.vil", "https://e.vil/x", "\n", "\n>", "\n  * ", "\n^", "  ", " ", "words", "\u{65e5}\u{672c}",
    "{{tag>x}}", "<WRAP>",
];

fn raws(doc: &Document) -> Vec<String> {
    match &doc.skeleton {
        Skeleton::Text { parts, .. } => parts
            .iter()
            .map(|p| match p {
                SkelPart::Raw(s) => s.clone(),
                SkelPart::Unit(_) => "\u{1}".into(),
            })
            .collect(),
        _ => vec![],
    }
}

fn code_set(u: &Unit) -> Vec<String> {
    let mut v: Vec<String> = u
        .codes
        .iter()
        .filter(|(k, _)| !k.starts_with('~'))
        // white space at a label edge moves between the text and the code: not markup
        .map(|(_, c)| {
            let t = |o: &Option<String>| o.as_deref().map(|s| s.trim().to_string());
            format!("{:?}|{:?}|{:?}", t(&c.open), t(&c.close), t(&c.standalone))
        })
        // the inert wrapper and the separator escapes a translation needs, and a character entity the translator
        // typed, are the only new codes
        .filter(|s| !s.contains("<nowiki") && !s.contains("Some(\"&") && !s.contains("{{!}}") && !s.contains("{{=}}") && !s.contains("%%|%%") && !s.contains("%%^%%") && !s.contains("%%:::%%"))
        .collect();
    v.sort();
    v
}

/// Translates unit `ui` with `pieces` (hostile words) interleaved with its own tags in source order.
fn attack(f: &dyn Filter, page: &str, ui: usize, pieces: &[&str]) -> Result<(), String> {
    let doc = extract(f, page.as_bytes()).unwrap();
    let mut d2 = doc.clone();
    let src = d2.units[ui].source();
    // every text of the source becomes hostile words around a letter, so hostile text touches every tag on both
    // sides and every label keeps a letter; text is also added before and after the whole unit
    let mut k = 0usize;
    let mut next = || {
        let p = pieces[k % pieces.len()];
        k += 1;
        p
    };
    let mut t = vec![Inline::text(format!("{}w{}", next(), next()))];
    for i in &src {
        match i {
            Inline::Text(_) => t.push(Inline::text(format!("{}w{}", next(), next()))),
            tag => t.push(tag.clone()),
        }
    }
    t.push(Inline::text(format!("{}w{}", next(), next())));
    let id = d2.units[ui].segments[0].id.clone();
    d2.set_target(&id, merge_text(t)).map_err(|e| e.to_string())?;
    let out = f.merge(&d2).map_err(|e| e.to_string())?;
    let back = extract(f, &out).map_err(|e| format!("re-extract: {e}"))?;
    let show = String::from_utf8_lossy(&out).into_owned();
    if back.units.len() != doc.units.len() {
        return Err(format!("unit count {} -> {}\n{show}", doc.units.len(), back.units.len()));
    }
    if raws(&back) != raws(&doc) {
        return Err(format!("text between units changed\n{show}"));
    }
    for (j, (a, b)) in doc.units.iter().zip(&back.units).enumerate() {
        if j != ui && a.source() != b.source() {
            return Err(format!("unit {j} changed\n{show}"));
        }
        if j == ui {
            let (want, got) = (code_set(a), code_set(b));
            if got.iter().any(|c| !want.contains(c)) {
                return Err(format!("unit {j} got codes {got:?}, source had {want:?}\n{show}"));
            }
        }
    }
    Ok(())
}

fn run(dw: bool, page_i: usize, ui: usize, picks: &[usize]) -> Result<(), String> {
    let (pages, evil): (&[&str], &[&str]) = if dw { (DW_PAGES, DW_EVIL) } else { (MW_PAGES, MW_EVIL) };
    let page = pages[page_i % pages.len()];
    let f: Box<dyn Filter> = if dw { Box::new(bilingua_wiki::DokuWiki) } else { Box::new(bilingua_wiki::MediaWiki::with_template_params("Quote:text,Note:1")) };
    let n = extract(f.as_ref(), page.as_bytes()).unwrap().units.len();
    let pieces: Vec<&str> = picks.iter().map(|p| evil[p % evil.len()]).collect();
    attack(f.as_ref(), page, ui % n, &pieces)
}

#[test]
fn every_single_fragment_in_every_unit() {
    let mut bad = Vec::new();
    for dw in [false, true] {
        let (pages, evil): (&[&str], &[&str]) = if dw { (DW_PAGES, DW_EVIL) } else { (MW_PAGES, MW_EVIL) };
        let f: Box<dyn Filter> = if dw { Box::new(bilingua_wiki::DokuWiki) } else { Box::new(bilingua_wiki::MediaWiki::with_template_params("Quote:text,Note:1")) };
        for page in pages {
            let n = extract(f.as_ref(), page.as_bytes()).unwrap().units.len();
            for ui in 0..n {
                for (a, ea) in evil.iter().enumerate() {
                    for picks in [vec![a], vec![a, 0], vec![0, a], vec![a, a], vec![a, 1, a]] {
                        let pieces: Vec<&str> = picks.iter().map(|p| evil[*p]).collect();
                        if let Err(e) = attack(f.as_ref(), page, ui, &pieces) {
                            bad.push(format!("dw={dw} unit {ui} {ea:?} {picks:?}: {e}"));
                        }
                    }
                }
            }
        }
    }
    // one example per fragment and unit kind keeps the report short
    let mut seen = std::collections::BTreeSet::new();
    let firsts: Vec<String> = bad.iter().filter(|b| seen.insert(b.split(" [").next().unwrap_or("").to_string())).take(60).map(|b| b.chars().take(500).collect()).collect();
    assert!(bad.is_empty(), "{} injections, first:\n{}", bad.len(), firsts.join("\n----\n"));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(5000))]
    #[test]
    fn random_hostile_translations_create_no_markup(dw in any::<bool>(), p in 0usize..8, ui in 0usize..64, picks in proptest::collection::vec(0usize..200, 1..10)) {
        if let Err(e) = run(dw, p, ui, &picks) {
            prop_assert!(false, "{}", e);
        }
    }
}

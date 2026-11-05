//! Gate: the cases where the LocOS filter lets a translation create markup (see the gate report), each written inert
//! by Bilingua. Every build reads back with the source's units and codes.
mod common;
use bilingua_model::*;
use bilingua_wiki::{DokuWiki, MediaWiki};
use common::*;

fn case(f: &dyn Filter, src: &str, unit: usize, tr: &str, want: &str) {
    let before = texts(f, src.as_bytes());
    let out = build_str(f, src.as_bytes(), &[(unit, tr)]);
    assert_eq!(out, want, "{tr:?}");
    let after = extract(f, out.as_bytes()).unwrap();
    assert_eq!(after.units.len(), before.len(), "{out}");
}

#[test]
fn markup_across_a_tag_edge() {
    let mw = MediaWiki::default();
    case(&mw, "A {{tpl}} b.\n", 0, "A {\u{27e6}x1\u{27e7}} b.", "A <nowiki>{</nowiki>{{tpl}}<nowiki>}</nowiki> b.\n");
    case(&mw, "An ''it'' word.\n", 0, "An \u{27e6}g1\u{27e7}it\u{27e6}/g1\u{27e7}' word.", "An ''it''<nowiki>'</nowiki> word.\n");
    case(
        &mw,
        "See https://example.com/a now.\n",
        0,
        "See [\u{27e6}x1\u{27e7} evil label] now.",
        "See <nowiki>[</nowiki>https://example.com/a evil label] now.\n",
    );
    case(&DokuWiki, "An //it// word.\n", 0, "An x/\u{27e6}g1\u{27e7}it\u{27e6}/g1\u{27e7} word.", "An x<nowiki>/</nowiki>//it// word.\n");
}

#[test]
fn table_lines_and_terms() {
    let mw = MediaWiki::default();
    case(&mw, "{|\n|cell one\n|}\n", 0, "}x", "{|\n|<nowiki>}</nowiki>x\n|}\n");
    case(&mw, "{|\n|cell one\n|}\n", 0, "+x", "{|\n|<nowiki>+</nowiki>x\n|}\n");
    case(&mw, "{|\n|a\nmore words\n|}\n", 1, "|new cell", "{|\n|a\n<nowiki>|</nowiki>new cell\n|}\n");
    case(&mw, "{|\n! Head one !! Head two\n|}\n", 0, "A !! B!", "{|\n! A <nowiki>!!</nowiki> B<nowiki>!</nowiki> !! Head two\n|}\n");
    case(&mw, "; Term : definition\n", 0, "Ter:m [x", "; Ter<nowiki>:</nowiki>m <nowiki>[</nowiki>x : definition\n");
    case(&mw, ";Term only\n", 0, "a:b", ";a<nowiki>:</nowiki>b\n");
    // (a cell of only `:::` has no letter, so it is no unit when read again: the count check does not apply)
    assert_eq!(build_str(&DokuWiki, b"^ H ^\n| a cell |\n", &[(0, ":::")]), "^ %%:::%% ^\n| a cell |\n");
}

#[test]
fn separators_inside_inert_markup_stay_plain() {
    let mw = MediaWiki::default();
    case(&mw, "{|\n| a cell\n|}\n", 0, "{|x", "{|\n| <nowiki>{|</nowiki>x\n|}\n");
    let q = MediaWiki::with_template_params("Quote:text,Note:1");
    case(&q, "{{Quote|text=Quoted words}}\n", 0, "=x}", "{{Quote|text=<nowiki>=</nowiki>x<nowiki>}</nowiki>}}\n");
    // a parameter translated in place is escaped like one on its own line
    case(&q, "Words {{Note|Remember this}} here.\n", 0, "Mots \u{27e6}x1\u{27e7}a=b|c\u{27e6}x2\u{27e7} ici.", "Mots {{Note|a{{=}}b{{!}}c}} ici.\n");
}

#[test]
fn labels_keep_their_end() {
    let mw = MediaWiki::default();
    case(&mw, "See [https://x.org link words] now.\n", 0, "Voir \u{27e6}g1\u{27e7}a]b\u{27e6}/g1\u{27e7}.", "Voir [https://x.org a<nowiki>]</nowiki>b].\n");
    case(&mw, "A [[File:a.png|thumb|A caption]] b.\n", 0, "Un \u{27e6}g1\u{27e7}x|y\u{27e6}/g1\u{27e7} b.", "Un [[File:a.png|thumb|x<nowiki>|</nowiki>y]] b.\n");
    case(&DokuWiki, "A {{m.png|cap words}} b.\n", 0, "Un \u{27e6}g1\u{27e7}x}y\u{27e6}/g1\u{27e7} b.", "Un {{m.png|x<nowiki>}</nowiki>y}} b.\n");
}

#[test]
fn urls_and_switches_in_the_words() {
    let mw = MediaWiki::default();
    case(&mw, "Plain words.\n", 0, "Mail mailto:a@b.c or http://e.vil __notoc__", "Mail <nowiki>mailto:</nowiki>a@b.c or <nowiki>http://</nowiki>e.vil <nowiki>__</nowiki>notoc__\n");
    case(&DokuWiki, "Plain words.\n", 0, "Go www.e.vil", "Go <nowiki>www.</nowiki>e.vil\n");
    // a DokuWiki URL from the source is ended before translated words that would extend it
    case(&DokuWiki, "See https://example.com/a now.\n", 0, "\u{27e6}x1\u{27e7}/evil now", "https://example.com/a<nowiki></nowiki>/evil now\n");
}

#[test]
fn long_underscore_runs_are_linear() {
    // a scan for the closing `__` at every `__` would be quadratic over such a run
    let mw = MediaWiki::default();
    let src = "Plain words.\n";
    let tr = format!("x {}", "__ABCDEFGHIJ_".repeat(40_000));
    let t = std::time::Instant::now();
    let out = build_str(&mw, src.as_bytes(), &[(0, &tr)]);
    assert!(out.len() > tr.len());
    assert!(t.elapsed() < std::time::Duration::from_secs(20), "{:?}", t.elapsed());
}

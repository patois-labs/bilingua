//! The LocOS AsciiDoc suite (`tests/verify_asciidoc_filter.py`) ported: its 14 test functions become the tests below
//! (13 in full; the 14th, "surfaces", is API / CLI / HTTP and is covered here by the extension, PGP and `:lang:`
//! parts only). Where Bilingua differs from LocOS by decision (encoding kept on merge, no `:lang:` stamp when
//! nothing is translated) the expectation says so.
mod common;
use bilingua_adoc::Adoc;
use bilingua_model::*;
use common::*;
use regex::Regex;
use std::collections::BTreeMap;

const DOC: &str = r##"= User Guide for {product-name}
Jane Doe <jane@example.com>
v1.2, 2026-10-01
:product-name: Acme Widget
:toc:
:description: This is a long description \
  that wraps onto a second line.

// A line comment that must stay untouched.

This is the *first* paragraph with a {product-name} reference.
It continues on a second line with _italic_ text and `inline code`.

== Getting Started ==

NOTE: Read the link:install.html[installation guide] before you begin.

.Supported platforms
* Windows with kbd:[Ctrl+C] support
* macOS, see <<mac-setup,the macOS section>>
** Nested item with https://example.com[our website^]
* [x] A checked item
. First step
. Second step +
continued after a hard break
<1> A callout explanation

CPU:: The brain of the computer.
Hard drive::
Permanent storage for data.

[source,python]
----
print("Hello, world")  # code stays
----

....
Literal block stays.
....

++++
<p>Raw passthrough stays</p>
++++

////
Comment block stays.
////

```js
fenced code stays
```

[source]
A source paragraph stays.

[NOTE]
====
An admonition block body is translated.
====

include::partials/legal.adoc[]

 An indented literal paragraph stays.

[cols="1,2"]
|===
|Name |Description

|Widget
|A small device with a \| pipe inside.

2+^.^|Spanning cell text
a|An AsciiDoc cell with *bold*
l|literal cell stays
|===

[format=csv]
|===
csv,cell stays
|===

image::diagram.png[Architecture diagram]

Footnote here.footnote:[A translatable footnote.] Water is H~2~O.

Two Line Title
--------------

Final paragraph with [.role]#highlighted text# and pass:[<u>raw</u>] and +{literal}+.
"##;

const TRANSLATED: &[&str] = &[
    "User Guide for",
    "This is the",
    "Getting Started",
    "Read the",
    "Supported platforms",
    "Windows with",
    "macOS, see",
    "Nested item with",
    "A checked item",
    "First step",
    "Second step",
    "A callout explanation",
    "CPU",
    "The brain of the computer.",
    "Hard drive",
    "Permanent storage for data.",
    "An admonition block body is translated.",
    "Name",
    "Description",
    "Widget",
    "A small device with a",
    "Spanning cell text",
    "An AsciiDoc cell with",
    "Footnote here.",
    "Two Line Title",
    "Final paragraph with",
];
const KEPT: &[&str] = &[
    "Jane Doe",
    "v1.2",
    "Acme Widget",
    "line comment",
    "Hello, world",
    "Literal block stays",
    "Raw passthrough stays",
    "Comment block stays",
    "fenced code stays",
    "source paragraph stays",
    "partials/legal",
    "indented literal",
    "literal cell stays",
    "csv,cell stays",
    "Architecture diagram",
];

/// The mapping "FR " + text for every unit.
fn fr_all(data: &[u8]) -> Vec<(usize, String)> {
    units(data).into_iter().enumerate().map(|(i, t)| (i, format!("FR {t}"))).collect()
}

fn run(data: &[u8], m: &[(usize, String)]) -> String {
    let v: Vec<(usize, &str)> = m.iter().map(|(i, s)| (*i, s.as_str())).collect();
    build_str(data, &v)
}

fn toks(t: &str) -> Vec<String> {
    Regex::new("\u{27e6}/?[a-z]?\\d+\u{27e7}").unwrap().find_iter(t).map(|m| m.as_str().to_string()).collect()
}

#[derive(Debug, PartialEq)]
enum C {
    P(String),
    G(String, String),
}

fn map_of(u: &Unit) -> BTreeMap<String, C> {
    u.codes
        .iter()
        .filter(|(k, _)| !k.starts_with('~'))
        .map(|(k, c)| {
            let v = match (&c.standalone, &c.open, &c.close) {
                (Some(s), _, _) => C::P(s.clone()),
                (None, Some(o), Some(cl)) => C::G(o.clone(), cl.clone()),
                _ => panic!(),
            };
            (k.clone(), v)
        })
        .collect()
}

fn p(s: &str) -> C {
    C::P(s.into())
}
fn g(a: &str, b: &str) -> C {
    C::G(a.into(), b.into())
}

#[test]
fn registry() {
    assert_eq!(Adoc.extensions(), &["adoc", "asciidoc", "asc"]);
    assert_eq!(Adoc.id(), "adoc");
    assert!(!Adoc.sniff(DOC.as_bytes()));
}

#[test]
fn identity() {
    let crlf_doc = crlf(DOC);
    let bom_doc = [b"\xef\xbb\xbf".as_slice(), DOC.as_bytes()].concat();
    for data in [DOC.as_bytes().to_vec(), crlf_doc.as_bytes().to_vec(), bom_doc, DOC.trim_end_matches('\n').as_bytes().to_vec()] {
        assert_identity(&data);
    }
    for data in [&b""[..], b"\n\n"] {
        assert!(units(data).is_empty());
        let doc = extract(data).unwrap();
        assert_eq!(Adoc.merge(&doc).unwrap(), data);
    }
    assert_eq!(units(crlf_doc.as_bytes()), units(DOC.as_bytes()));
}

#[test]
fn units_are_the_translatable_blocks() {
    let doc = extract(DOC.as_bytes()).unwrap();
    let texts: Vec<String> = doc.units.iter().map(token_text).collect();
    let joined = texts.join("\n");
    for want in TRANSLATED {
        assert!(texts.iter().any(|t| t.contains(want)), "{want}: {texts:?}");
    }
    for kept in KEPT {
        assert!(!joined.contains(kept), "{kept}: {texts:?}");
    }
    assert!(texts[0].starts_with("User Guide for"));
    assert!(texts.iter().any(|t| t == "Getting Started") && !texts.iter().any(|t| t.ends_with("==")));
    assert!(texts.iter().any(|t| t == "Supported platforms"));
    assert!(texts.iter().any(|t| t == "A checked item") && texts.iter().any(|t| t == "A callout explanation"));
    assert!(texts.iter().any(|t| t == "CPU") && texts.iter().any(|t| t == "Hard drive"));
    // #733: the wrapped header description is the LAST unit (appended), joined
    let meta: Vec<(String, String)> =
        doc.units.iter().filter_map(|u| u.context.style.clone().map(|s| (token_text(u), s))).collect();
    assert_eq!(meta, vec![("This is a long description that wraps onto a second line.".to_string(), "meta:description".to_string())]);
    assert_eq!(doc.units.last().unwrap().context.style.as_deref(), Some("meta:description"));
    let ids: Vec<&str> = doc.units.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(ids[0], "u1");
    assert_eq!(ids.len(), doc.units.len());
}

#[test]
fn tokens_are_the_inline_syntax() {
    let doc = extract(DOC.as_bytes()).unwrap();
    let by: BTreeMap<String, &Unit> =
        doc.units.iter().map(|u| (token_text(u).split('\u{27e6}').next().unwrap().trim().to_string(), u)).collect();
    let first = by["This is the"];
    assert_eq!(
        token_text(first),
        "This is the \u{27e6}g1\u{27e7}first\u{27e6}/g1\u{27e7} paragraph with a \u{27e6}x2\u{27e7} reference. It continues on a second line with \u{27e6}g3\u{27e7}italic\u{27e6}/g3\u{27e7} text and \u{27e6}x4\u{27e7}."
    );
    assert_eq!(
        map_of(first),
        BTreeMap::from([("1".into(), g("*", "*")), ("2".into(), p("{product-name}")), ("3".into(), g("_", "_")), ("4".into(), p("`inline code`"))])
    );
    let link = by["Read the"];
    assert!(token_text(link).contains("\u{27e6}g1\u{27e7}installation guide\u{27e6}/g1\u{27e7}"));
    assert_eq!(map_of(link)["1"], g("link:install.html[", "]"));
    assert_eq!(map_of(by["Windows with"])["1"], p("kbd:[Ctrl+C]"));
    assert_eq!(map_of(by["macOS, see"])["1"], g("<<mac-setup,", ">>"));
    assert_eq!(map_of(by["Nested item with"])["1"], g("https://example.com[", "^]"));
    assert_eq!(map_of(by["Second step"])["1"], p(" +\n"));
    let foot = map_of(by["Footnote here."]);
    assert_eq!(foot["1"], g("footnote:[", "]"));
    assert_eq!(foot["2"], g("~", "~"));
    let fin = map_of(by["Final paragraph with"]);
    assert_eq!(fin["1"], g("[.role]#", "#"));
    assert_eq!(fin["2"], p("pass:[<u>raw</u>]"));
    assert_eq!(fin["3"], p("+{literal}+"));
    assert_eq!(map_of(by["A small device with a"])["1"], p("\\|"));
    // resource-style placeholders in prose are left as text (AsciiDoc has none of its own); {0} is an attribute
    assert_eq!(units(b"Use %s or {0} here.\n")[0], "Use %s or \u{27e6}x1\u{27e7} here.");
}

#[test]
fn translated_text_is_written_in_place() {
    let m = fr_all(DOC.as_bytes());
    let out = run(DOC.as_bytes(), &m);
    for want in [
        "= FR User Guide for {product-name}\nJane Doe <jane@example.com>",
        "== FR Getting Started ==",
        "NOTE: FR Read the link:install.html[installation guide] before",
        ".FR Supported platforms\n* FR Windows with kbd:[Ctrl+C] support",
        "* [x] FR A checked item",
        "<1> FR A callout explanation",
        ". FR Second step +\ncontinued after a hard break",
        "FR CPU:: FR The brain of the computer.\nFR Hard drive::\nFR Permanent",
        "FR This is the *first* paragraph with a {product-name} reference. It continues",
        "2+^.^|FR Spanning cell text\na|FR An AsciiDoc cell with *bold*\nl|literal",
        "FR Two Line Title\n--------------",
        ":toc:\n:description: FR This is a long description that wraps onto a second line.\n\n// A line",
    ] {
        assert!(out.contains(want), "{want}\n{out}");
    }
    for kept in KEPT {
        assert!(out.contains(kept), "{kept}");
    }
    assert!(!out.contains('\u{27e6}'));
    // a table separator inside a translated cell is escaped; markup tokens re-inserted
    let texts = units(DOC.as_bytes());
    let cell = texts.iter().position(|t| t == "Widget").unwrap();
    let out2 = build_str(DOC.as_bytes(), &[(cell, "Truc | machin")]);
    assert!(out2.contains("|Truc \\| machin\n"), "{out2}");
    let para = texts.iter().position(|t| t.starts_with("This is the")).unwrap();
    let out3 = build_str(DOC.as_bytes(), &[(para, "C'est le \u{27e6}g1\u{27e7}premier\u{27e6}/g1\u{27e7} (\u{27e6}x2\u{27e7})\n\nfin")]);
    assert!(out3.contains("C'est le *premier* ({product-name}) fin\n"), "{out3}");
    // a tag that is not in the source is refused (LocOS dropped unknown tokens)
    let mut doc = extract(DOC.as_bytes()).unwrap();
    let id = doc.units[para].segments[0].id.clone();
    let err = doc.set_target(&id, vec![Inline::tag("99", TagKind::Standalone, "x")]);
    assert!(matches!(err, Err(Error::InventedTag(_))));
    // CRLF file: untouched line endings stay CRLF
    let c = crlf(DOC);
    let outc = build(c.as_bytes(), &fr_all(c.as_bytes()).iter().map(|(i, s)| (*i, s.as_str())).collect::<Vec<_>>());
    assert!(outc.windows(2).any(|w| w == b"\r\n"));
    let bare = outc.iter().enumerate().filter(|(i, b)| **b == b'\n' && (*i == 0 || outc[*i - 1] != b'\r')).count();
    assert_eq!(bare, 0);
}

const META_DOC: &str = "// leading comment
= Getting started with the dashboard
Jane Doe <jane@example.com>
:description: Learn how to set up {product} in five minutes.
:keywords: setup, onboarding
:page-layout: post
:toc: left
:robots: noindex
:description-extra: not a meta
:keywords-x: nope
:imagesdir: images

Open the dashboard and click *New project*.

:description: A body attribute entry, not the header's.
";

#[test]
fn header_meta_733() {
    let doc = extract(META_DOC.as_bytes()).unwrap();
    let body: Vec<String> = doc.units.iter().filter(|u| u.context.style.is_none()).map(token_text).collect();
    assert_eq!(body, vec!["Getting started with the dashboard", "Open the dashboard and click \u{27e6}g1\u{27e7}New project\u{27e6}/g1\u{27e7}."]);
    let n = body.len();
    let meta: Vec<(usize, String, String)> = doc
        .units
        .iter()
        .enumerate()
        .filter_map(|(i, u)| u.context.style.clone().map(|s| (i, token_text(u), s)))
        .collect();
    assert_eq!(
        meta,
        vec![
            (n, "Learn how to set up \u{27e6}x1\u{27e7} in five minutes.".to_string(), "meta:description".to_string()),
            (n + 1, "setup, onboarding".to_string(), "meta:keywords".to_string())
        ]
    );
    // body indexes are what a document without the metadata gives
    let plain = META_DOC.replace(":description: Learn", ":page-x: Learn").replace(":keywords: setup", ":page-y: setup");
    assert_eq!(units(plain.as_bytes()), body);
    assert_identity(META_DOC.as_bytes());
    let mapping = [
        (n, "Apprenez \u{27e6}x1\u{27e7}\nen cinq minutes \\"),
        (n + 1, "installation, prise en main"),
        (0, "Bien demarrer"),
    ];
    let out = build_str(META_DOC.as_bytes(), &mapping);
    assert!(out.contains("= Bien demarrer\n"), "{out}");
    assert!(out.contains(":description: Apprenez {product} en cinq minutes {backslash}\n:keywords: "), "{out}");
    assert!(out.contains(":keywords: installation, prise en main\n:page-layout: post\n"), "{out}");
    for kept in [":robots: noindex", ":description-extra: not a meta", ":keywords-x: nope"] {
        assert!(out.contains(kept), "{kept}");
    }
    assert!(out.contains(":description: A body attribute entry, not the header's.\n"));
    assert_eq!(units(out.as_bytes()).len(), doc.units.len());
    // a wrapped value: joined into one unit, written back on one line
    let wrapped = "= T\n:keywords: one, \\\n  two, \\\n  three\n:toc:\n\nBody.\n";
    let wd = extract(wrapped.as_bytes()).unwrap();
    let got: Vec<(String, String)> = wd.units.iter().map(|u| (token_text(u), u.context.style.clone().unwrap_or_default())).collect();
    assert_eq!(
        got,
        vec![("T".into(), "".into()), ("Body.".into(), "".into()), ("one, two, three".into(), "meta:keywords".into())]
    );
    assert_identity(wrapped.as_bytes());
    assert_eq!(build_str(wrapped.as_bytes(), &[(2, "un, deux")]), "= T\n:keywords: un, deux\n:toc:\n\nBody.\n");
    // a value with a hard line break, a letterless value and a body-only file: not offered
    for src in ["= T\n:description: a + \\\nb\n\nx\n", "= T\n:keywords: 123, {x}\n\nx\n", "Para.\n\n:description: later\n"] {
        assert!(extract(src.as_bytes()).unwrap().units.iter().all(|u| u.context.style.is_none()), "{src}");
    }
    // a merge with a target language stamps :lang: at the end of the header
    let loc = {
        let v: Vec<(usize, &str)> = mapping.to_vec();
        String::from_utf8(build_for(META_DOC.as_bytes(), &v, Some("fr-FR"))).unwrap()
    };
    assert!(loc.contains(":imagesdir: images\n:lang: fr-FR\n\nOpen"), "{loc}");
}

#[test]
fn lang_is_written_when_something_is_translated() {
    let first = [(0usize, "Guide")];
    let out = String::from_utf8(build_for(DOC.as_bytes(), &first, Some("fr-FR"))).unwrap();
    assert!(out.contains(":description: This is a long description \\\n  that wraps onto a second line.\n"));
    assert!(out.contains("that wraps onto a second line.\n:lang: fr-FR\n\n// A line"), "{}", &out[..400]);
    let has = "= Title\n:lang: en\n:toc:\n\nBody text.\n";
    let t = [(0usize, "Titre")];
    assert_eq!(String::from_utf8(build_for(has.as_bytes(), &t, Some("de"))).unwrap(), "= Titre\n:lang: de\n:toc:\n\nBody text.\n");
    let bare = "Just a paragraph.\n";
    let t = [(0usize, "Un paragraphe.")];
    assert_eq!(String::from_utf8(build_for(bare.as_bytes(), &t, Some("ja"))).unwrap(), ":lang: ja\n\nUn paragraphe.\n");
    // a malformed locale changes nothing
    assert_eq!(String::from_utf8(build_for(bare.as_bytes(), &t, Some("bad value\n:x:"))).unwrap(), "Un paragraphe.\n");
    // a plain merge never stamps, and neither does a merge where nothing is translated (lead decision 11-04)
    assert_eq!(build(has.as_bytes(), &[]), has.as_bytes());
    assert_eq!(build_for(has.as_bytes(), &[], Some("de")), has.as_bytes());
    assert_eq!(build_for(DOC.as_bytes(), &[], Some("fr-FR")), DOC.as_bytes());
}

#[test]
fn encodings_binary_and_pgp() {
    let data: Vec<u8> = "Caf\u{e9} cr\u{e8}me paragraph.\n".chars().map(|c| c as u32 as u8).collect();
    let doc = extract(&data).unwrap();
    assert_eq!(token_text(&doc.units[0]), "Caf\u{e9} cr\u{e8}me paragraph.");
    assert!(matches!(&doc.skeleton, Skeleton::Text { encoding, .. } if encoding == "windows-1252"));
    // LocOS converts to UTF-8 on build; Bilingua keeps the file's encoding (B6)
    let out = build(&data, &[(0, "Caf\u{e9} traduit.")]);
    let want: Vec<u8> = "Caf\u{e9} traduit.\n".chars().map(|c| c as u32 as u8).collect();
    assert_eq!(out, want);
    let bin = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x81\x8d\x00".repeat(20);
    assert!(matches!(extract(&bin), Err(Error::Encoding(_))));
    match extract(b"-----BEGIN PGP SIGNATURE-----\n\niQEz\n-----END PGP SIGNATURE-----\n") {
        Err(Error::Parse { msg, .. }) => assert!(msg.contains("OpenPGP")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn caps() {
    let many: String = (0..20).map(|i| format!("* item {i}\n")).collect();
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_units(5));
    assert!(matches!(Adoc.extract(many.as_bytes(), &o), Err(Error::Limit(e)) if e.kind == LimitKind::Units));
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_unit_chars(50));
    let long = format!("{}\n", "word ".repeat(30));
    assert!(matches!(Adoc.extract(long.as_bytes(), &o), Err(Error::Limit(e)) if e.kind == LimitKind::UnitChars));
}

#[test]
fn hostile_inputs_stay_fast_and_exact() {
    let cases: Vec<Vec<u8>> = vec![
        format!("{}\n", "*".repeat(20000)).into_bytes(),
        format!("{}x\n", "* ".repeat(10000)).into_bytes(),
        format!("{}\n", "_a ".repeat(8000)).into_bytes(),
        format!("{}\n", "`x ".repeat(8000)).into_bytes(),
        format!("{}\n", "link:".repeat(5000)).into_bytes(),
        format!("{}\n", "{".repeat(20000)).into_bytes(),
        format!("{}\n", "<<".repeat(10000)).into_bytes(),
        format!("|===\n{}\n|===\n", "|".repeat(20000)).into_bytes(),
        format!("----\n{}", "never closed\n".repeat(2000)).into_bytes(),
        format!("{}\n", "[[".repeat(10000)).into_bytes(),
        format!("text {}\n", "+".repeat(20000)).into_bytes(),
        format!("{}\n", "a\\".repeat(10000)).into_bytes(),
        b"include::/etc/passwd[]\ninclude::https://evil.example/x.adoc[]\nok text\n".to_vec(),
    ];
    for data in &cases {
        let t0 = std::time::Instant::now();
        assert_identity(data);
        assert!(t0.elapsed().as_secs() < 20, "{:?}", t0.elapsed());
    }
    assert_eq!(units(cases.last().unwrap()), vec!["ok text"]);
}

#[test]
fn block_syntax_guard() {
    let src = "= T\n\nFirst para.\n\nSecond para.\n\nThird para.\n\n* item\n";
    assert_eq!(units(src.as_bytes()), ["T", "First para.", "Second para.", "Third para.", "item"]);
    let out = build_str(src.as_bytes(), &[(1, "== Pas un titre"), (2, "// pas un commentaire"), (3, "* pas une liste")]);
    assert!(out.contains("\n{empty}== Pas un titre\n"), "{out}");
    assert!(out.contains("\n{empty}// pas un commentaire\n"), "{out}");
    assert!(out.contains("\n{empty}* pas une liste\n"), "{out}");
    assert!(out.contains("* item"));
    assert_eq!(units(out.as_bytes()).len(), 5);
    assert!(!build_str(src.as_bytes(), &[(1, "Premier paragraphe.")]).contains("{empty}"));
}

#[test]
fn cjk_constrained_marks_are_unconstrained() {
    let src = "Run `make` then *bold* and _it_ and #hi# and +pass+ and [.path]_x_ and **two** end.\n";
    let unit = units(src.as_bytes())[0].clone();
    let t = toks(&unit);
    assert_eq!(t.len(), 12, "{unit}");
    let ja: String = t.iter().map(|x| format!("\u{3042}{x}")).collect::<String>() + "\u{3002}";
    let out = build_str(src.as_bytes(), &[(0, &ja)]);
    assert_eq!(
        out,
        "\u{3042}``make``\u{3042}**\u{3042}**\u{3042}__\u{3042}__\u{3042}##\u{3042}##\u{3042}++pass++\u{3042}[.path]__\u{3042}__\u{3042}**\u{3042}**\u{3002}\n"
    );
    assert_eq!(toks(&units(out.as_bytes())[0]).len(), 12);
    // punctuation and spaces are boundaries: the single form stays
    let ja2 = format!("\u{ff08}{}\u{3001}{}x{}\u{3002}", t[0], t[1], t[2]);
    assert!(build_str(src.as_bytes(), &[(0, &ja2)]).starts_with("\u{ff08}`make`\u{3001}*x*\u{3002}"));
    // connector punctuation is a word character too (full-width low line)
    let ja4 = format!("\u{ff08}{}\u{ff3f}\u{3002}", t[0]);
    assert!(build_str(src.as_bytes(), &[(0, &ja4)]).starts_with("\u{ff08}``make``\u{ff3f}\u{3002}"));
    // a pair whose translated text starts with a space cannot stay constrained
    let sp = format!("Lancez {} puis {} gras{}", t[0], t[1], t[2]);
    assert_eq!(build_str(src.as_bytes(), &[(0, &sp)]), "Lancez `make` puis ** gras**\n");
    // a Latin translation with spaces keeps the source form byte for byte
    let fr = unit.replace("Run", "Lancez").replace(" end", " fin");
    assert_eq!(build_str(src.as_bytes(), &[(0, &fr)]), src.replace("Run", "Lancez").replace(" end", " fin"));
}

#[test]
fn cjk_links_become_link_macros() {
    let src = ":uri-js: https://js.example.org\n\nSee https://brew.sh/[brew.sh] and {uri-js}[JS] or https://example.org now, mail mailto:team@example.invalid[us] and link:https://x.org[X] and {uri-js}.\n";
    let unit = units(src.as_bytes())[0].clone();
    let t = toks(&unit);
    assert_eq!(t.len(), 9, "{unit}");
    let zh = format!(
        "\u{8bf7}{}brew.sh{}\u{548c}{}[JS]\u{6216}{}\u{73b0}\u{5728}\u{ff0c}{}\u{6211}\u{4eec}{}\u{548c}{}X{}\u{548c}{}\u{3002}",
        t[0], t[1], t[2], t[3], t[4], t[5], t[6], t[7], t[8]
    );
    let out = build_str(src.as_bytes(), &[(0, &zh)]);
    let body = out.split_once("\n\n").unwrap().1;
    assert_eq!(
        body,
        "\u{8bf7}link:https://brew.sh/[brew.sh]\u{548c}link:{uri-js}[JS]\u{6216}link:https://example.org[]\u{73b0}\u{5728}\u{ff0c}mailto:team@example.invalid[\u{6211}\u{4eec}]\u{548c}link:https://x.org[X]\u{548c}link:{uri-js}[]\u{3002}\n"
    );
    // a bare URL runs on over ASCII ",.)" too when a letter follows; a non-URL attribute is never a link
    let src2 = ":ver: 2.0\n\nGet https://example.org, then {ver}[beta] and https://a.org.\n";
    let t2 = toks(&units(src2.as_bytes())[0]);
    let out2 = build_str(src2.as_bytes(), &[(0, &format!("\u{83b7}\u{53d6}{},\u{7136}\u{540e}{}[beta]\u{548c}{}\u{3002}", t2[0], t2[1], t2[2]))]);
    assert!(out2.ends_with("\u{83b7}\u{53d6}link:https://example.org[],\u{7136}\u{540e}{ver}[beta]\u{548c}link:https://a.org[]\u{3002}\n"), "{out2}");
    // "," or "." then a space or the end: the URL stops there, nothing changes
    let out3 = build_str(src2.as_bytes(), &[(0, &format!("Hol {}, dann {}[beta] und {}.", t2[0], t2[1], t2[2]))]);
    assert!(out3.ends_with("Hol https://example.org, dann {ver}[beta] und https://a.org.\n"), "{out3}");
    // after a space or an allowed character nothing changes; a Latin translation keeps the source bytes
    let spaced = format!("\u{8bf7} {}brew.sh{} ({})", t[0], t[1], t[3]);
    assert!(build_str(src.as_bytes(), &[(0, &spaced)]).ends_with("\u{8bf7} https://brew.sh/[brew.sh] (https://example.org)\n"));
    let fr = unit.replace("See", "Voir").replace(" now", " maintenant");
    assert_eq!(build_str(src.as_bytes(), &[(0, &fr)]), src.replace("See", "Voir").replace(" now", " maintenant"));
}

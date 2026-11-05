//! The Localization OS suite `tests/verify_wiki_filter.py` ported: its 14 test functions, 13 in full (the 14th,
//! `test_surfaces`, drives the LocOS HTTP API and CLI and has no counterpart here; `test_registry` checks the
//! Bilingua ids and extensions instead).
mod common;
use bilingua_model::*;
use bilingua_wiki::MediaWiki;
use common::*;

const MW_KEPT: &[&str] = &[
    "{{Infobox city\n| name = Paris\n| caption = The Eiffel Tower at night\n| population = {{formatnum:2100000}}\n}}",
    "<!-- editor comment that must stay -->",
    " preformatted code line",
    "print(\"hi\")  # code stays",
    "<pre>\nPre block stays.\n</pre>",
    "__NOTOC__",
    "[[Category:Cities]]",
    "[[fr:Paris]]",
    "{| class=\"wikitable\"",
];

#[test]
fn registry() {
    assert_eq!(mw().id(), "wiki");
    assert_eq!(mw().extensions(), ["wiki", "mediawiki", "wikitext"]);
    assert_eq!(dw().id(), "dokuwiki");
    assert_eq!(dw().extensions(), ["dokuwiki"]);
    // .txt is never guessed to be DokuWiki; wikitext is only found by its extension
    assert!(!dw().sniff(DW.as_bytes()) && !mw().sniff(MW.as_bytes()));
}

#[test]
fn identity() {
    for (f, data) in [(&mw() as &dyn Filter, MW), (&dw() as &dyn Filter, DW)] {
        let variants: Vec<Vec<u8>> = vec![
            data.as_bytes().to_vec(),
            crlf(data).into_bytes(),
            [&b"\xef\xbb\xbf"[..], data.as_bytes()].concat(),
            data.trim_end_matches('\n').as_bytes().to_vec(),
        ];
        for v in variants {
            assert_identity(f, &v);
        }
    }
}

#[test]
fn units_mediawiki() {
    let f = mw();
    let t = texts(&f, MW.as_bytes());
    let want = [
        "\u{27e6}g1\u{27e7}Paris\u{27e6}/g1\u{27e7} is the \u{27e6}g2\u{27e7}capital\u{27e6}/g2\u{27e7} of \u{27e6}x3\u{27e7}.\u{27e6}x4\u{27e7}Source text here.\u{27e6}x5\u{27e7} It has many \u{27e6}g6\u{27e7}museums\u{27e6}/g6\u{27e7} and \u{27e6}g7\u{27e7}an official site\u{27e6}/g7\u{27e7}.",
        "History",
        "Early days \u{27e6}x1\u{27e7}",
        "First item with \u{27e6}x1\u{27e7} inside",
        "Nested \u{27e6}g1\u{27e7}A tall tower\u{27e6}/g1\u{27e7} item",
        "Numbered item",
        "Term",
        "The definition",
        "Indented reply",
        "Table caption",
        "Header one",
        "Header two",
        "Red cell",
        "Plain cell",
        "\u{27e6}g1\u{27e7}label\u{27e6}/g1\u{27e7} cell",
        "Use \u{27e6}x1\u{27e7} and \u{27e6}x2\u{27e7} here.",
        "\u{27e6}x1\u{27e7} is a parameter.",
        "Signed \u{27e6}x1\u{27e7}",
    ];
    assert_eq!(t, want);
    let doc = extract(&f, MW.as_bytes()).unwrap();
    let u = &doc.units;
    let pair = |n: &str, o: &str, c: &str| {
        let cd = code(&u[0], n);
        assert_eq!((cd.open.as_deref(), cd.close.as_deref()), (Some(o), Some(c)), "code {n}");
    };
    pair("1", "'''", "'''");
    pair("2", "[[capital city|", "]]");
    assert_eq!(code(&u[0], "3").standalone.as_deref(), Some("[[France]]"));
    assert_eq!(code(&u[0], "4").standalone.as_deref(), Some("<ref name=\"a\">"));
    assert_eq!(code(&u[0], "5").standalone.as_deref(), Some("</ref>"));
    pair("6", "''", "''");
    pair("7", "[https://example.com ", "]");
    assert_eq!(code(&u[3], "1").standalone.as_deref(), Some("{{lang|fr|Bonjour}}"));
    assert_eq!(code(&u[4], "1").open.as_deref(), Some("[[File:Tower.jpg|thumb|200px|"));
    assert_eq!(code(&u[15], "1").standalone.as_deref(), Some("<code>foo()</code>"));
    assert_eq!(code(&u[15], "2").standalone.as_deref(), Some("<nowiki>[[not a link]]</nowiki>"));
    assert_eq!(code(&u[16], "1").standalone.as_deref(), Some("{{{1|Default param}}}"));
    assert_eq!(code(&u[17], "1").standalone.as_deref(), Some("~~~~"));
    // redirects, file links without captions, options-only captions, pipe trick
    assert!(texts(&f, b"#REDIRECT [[Target page]]\n").is_empty());
    assert_eq!(texts(&f, b"A [[File:x.png|thumb|left|200px]] b\n"), ["A \u{27e6}x1\u{27e7} b"]);
    assert_eq!(texts(&f, b"A [[Help:Contents|]] b\n"), ["A \u{27e6}x1\u{27e7} b"]);
    assert_eq!(texts(&f, b"See [[:Category:Cities|city list]].\n"), ["See \u{27e6}g1\u{27e7}city list\u{27e6}/g1\u{27e7}."]);
    // nested link inside a file caption
    let nested = texts(&f, b"[[File:a.png|thumb|A [[b|nested]] caption]]\n");
    assert_eq!(nested[0], "\u{27e6}g1\u{27e7}A \u{27e6}g2\u{27e7}nested\u{27e6}/g2\u{27e7} caption\u{27e6}/g1\u{27e7}");
    // resource-style placeholders in prose stay text; parser functions stay whole
    assert_eq!(texts(&f, b"Use %s or {0} with {{#if: x | yes | no }} now.\n"), ["Use %s or {0} with \u{27e6}x1\u{27e7} now."]);
    // an unclosed comment protects the rest; an unclosed <nowiki> only itself
    assert_eq!(texts(&f, b"Keep this.\n<!-- open\nhidden text\n"), ["Keep this."]);
    assert_eq!(texts(&f, b"A <nowiki> b\n"), ["A \u{27e6}x1\u{27e7} b"]);
}

#[test]
fn units_dokuwiki() {
    let f = dw();
    let t = texts(&f, DW.as_bytes());
    let want = [
        "Main title",
        "This is \u{27e6}g1\u{27e7}bold\u{27e6}/g1\u{27e7} and \u{27e6}g2\u{27e7}italic\u{27e6}/g2\u{27e7} text with \u{27e6}g3\u{27e7}a link\u{27e6}/g3\u{27e7} and \u{27e6}x4\u{27e7}. Second line \u{27e6}g5\u{27e7}a footnote\u{27e6}/g5\u{27e7} with \u{27e6}g6\u{27e7}An image caption\u{27e6}/g6\u{27e7}.",
        "List item one",
        "Ordered item",
        "Head A",
        "Head B",
        "Cell \u{27e6}g1\u{27e7}y\u{27e6}/g1\u{27e7} one",
        "Cell two",
        "more",
        "\u{27e6}x1\u{27e7} stays, \u{27e6}x2\u{27e7} too, see \u{27e6}x3\u{27e7} and \u{27e6}x4\u{27e7}.",
        "quoted line",
    ];
    assert_eq!(t, want);
    let doc = extract(&f, DW.as_bytes()).unwrap();
    assert_eq!(code(&doc.units[1], "3").open.as_deref(), Some("[[ns:page|"));
    assert_eq!(code(&doc.units[1], "6").open.as_deref(), Some("{{img.png?200|"));
    assert_eq!(code(&doc.units[9], "3").standalone.as_deref(), Some("https://example.com/a//b"));
    assert_eq!(texts(&f, b"{{rss>http://x.example/feed 5|Feed}} text\n"), ["\u{27e6}x1\u{27e7} text"]);
    assert_eq!(texts(&f, b"Line one\\\\ line two\n"), ["Line one\u{27e6}x1\u{27e7} line two"]);
    assert_eq!(texts(&f, b"<code>\nnever closed\n"), ["never closed"]);
    assert_eq!(texts(&f, b"See <code> here\n"), ["See \u{27e6}x1\u{27e7} here"]);
}

#[test]
fn dokuwiki_eol_linebreak_678() {
    let f = dw();
    for nl in ["\n", "\r\n"] {
        let src = format!("First line\\\\{nl}second line\\\\ and more{nl}last line\\\\{nl}");
        let t = texts(&f, src.as_bytes());
        assert_eq!(t, ["First line\u{27e6}x1\u{27e7} second line\u{27e6}x2\u{27e7} and more last line\u{27e6}x3\u{27e7}"]);
        let doc = extract(&f, src.as_bytes()).unwrap();
        assert_eq!(code(&doc.units[0], "1").standalone.as_deref(), Some("\\\\"));
        assert_eq!(f.merge(&doc).unwrap(), src.as_bytes());
        let out = build_str(&f, src.as_bytes(), &[(0, "Erste Zeile\u{27e6}x1\u{27e7} zweite\u{27e6}x2\u{27e7} und mehr letzte\u{27e6}x3\u{27e7}")]);
        assert!(!out.contains("nowiki"), "{out}");
        assert!(out.starts_with("Erste Zeile\\\\ zweite\\\\ und mehr letzte\\\\"), "{out}");
        assert!(out.ends_with(nl), "{out:?}");
    }
    // `\\` glued to a word stays text, and a translator's own `\\` is still written inert
    assert_eq!(texts(&f, b"a \\\\b\nnext\n"), ["a \\\\b next"]);
    let out = build_str(&f, b"one\ntwo\n", &[(0, "eins\\\\ zwei")]);
    assert!(out.contains("<nowiki>\\\\</nowiki>"), "{out}");
}

#[test]
fn dokuwiki_break_glued_701() {
    let f = dw();
    let src = b"Open Monday to Friday.\\\\ Closed on holidays.\n";
    assert_eq!(texts(&f, src), ["Open Monday to Friday.\u{27e6}x1\u{27e7} Closed on holidays."]);
    let out = build(&f, src, &[(0, "\u{6708}\u{66dc}\u{65e5}\u{304b}\u{3089}\u{3002}\u{27e6}x1\u{27e7}\u{795d}\u{65e5}\u{306f}\u{4f11}\u{696d}\u{3002}")]);
    assert_eq!(out, "\u{6708}\u{66dc}\u{65e5}\u{304b}\u{3089}\u{3002}\\\\ \u{795d}\u{65e5}\u{306f}\u{4f11}\u{696d}\u{3002}\n".as_bytes());
    let back = texts(&f, &out);
    assert_eq!(back.len(), 1);
    assert!(back[0].contains('\u{27e6}'));
    let fr = build(&f, src, &[(0, "Ouvert du lundi au vendredi.\u{27e6}x1\u{27e7} Ferm\u{e9} les jours f\u{e9}ri\u{e9}s.")]);
    assert_eq!(fr, "Ouvert du lundi au vendredi.\\\\ Ferm\u{e9} les jours f\u{e9}ri\u{e9}s.\n".as_bytes());
    assert_eq!(build(&f, src, &[(0, "Fin\u{27e6}x1\u{27e7}")]), b"Fin\\\\\n");
}

#[test]
fn translated() {
    let f = mw();
    let out = prefixed(&f, MW.as_bytes());
    assert!(
        out.contains("FR '''Paris''' is the [[capital city|capital]] of [[France]].<ref name=\"a\">Source text here.</ref> It has many"),
        "{out}"
    );
    assert!(out.contains("== FR History ==\n=== FR Early days <!-- note --> ==="));
    assert!(out.contains("* FR First item with {{lang|fr|Bonjour}} inside"));
    assert!(out.contains("; FR Term : FR The definition"));
    assert!(out.contains("! FR Header one !! FR Header two"));
    assert!(out.contains("| style=\"color:red\" | FR Red cell || FR Plain cell"));
    for kept in MW_KEPT {
        assert!(out.contains(kept), "{kept}");
    }
    assert!(!out.contains('\u{27e6}'));
    // translated words reorder around the tokens; a cell's pipe is escaped
    let out2 = build_str(
        &f,
        MW.as_bytes(),
        &[
            (0, "\u{27e6}g2\u{27e7}La capitale\u{27e6}/g2\u{27e7} de \u{27e6}x3\u{27e7} est \u{27e6}g1\u{27e7}Paris\u{27e6}/g1\u{27e7}.\nFin"),
            (12, "Rouge | vif"),
        ],
    );
    assert!(out2.contains("[[capital city|La capitale]] de [[France]] est '''Paris'''. Fin\n<!--"), "{out2}");
    assert!(out2.contains("| style=\"color:red\" | Rouge {{!}} vif || Plain cell"), "{out2}");
    // an unknown token is dropped
    let mut doc = extract(&f, MW.as_bytes()).unwrap();
    let id = doc.units[1].segments[0].id.clone();
    doc.units[1].segments[0].target = vec![Inline::text("x "), Inline::tag("9", TagKind::Standalone, "?"), Inline::text(" y")];
    let _ = id;
    let out3 = String::from_utf8(f.merge(&doc).unwrap()).unwrap();
    assert!(out3.contains("== x  y =="), "{out3}");
    // DokuWiki
    let d = dw();
    let dout = prefixed(&d, DW.as_bytes());
    assert!(dout.contains("====== FR Main title ======"));
    assert!(dout.contains("  * FR List item one\n  - FR Ordered item"));
    assert!(dout.contains("^ FR Head A ^ FR Head B ^\n| FR Cell [[x|y]] one | FR Cell two |\n| ::: | FR more |"), "{dout}");
    assert!(dout.contains("  indented code stays\n<code python>\nprint(\"x\")\n</code>"));
    assert!(dout.contains("<?php echo 1; ?>") && dout.contains("~~NOTOC~~"));
    let dout2 = build_str(&d, DW.as_bytes(), &[(7, "A | B ^ C")]);
    assert!(dout2.contains("| A %%|%% B %%^%% C |"), "{dout2}");
    let crlf_in = crlf(MW);
    let outc = prefixed(&f, crlf_in.as_bytes());
    assert!(outc.contains("\r\n") && !outc.replace("\r\n", "").contains('\n'));
}

#[test]
fn templates() {
    let f = MediaWiki::with_template_params("Infobox_city:caption, Template:quote:text, Note:1");
    let t = texts(&f, MW.as_bytes());
    assert_eq!(t[0], "The Eiffel Tower at night");
    let src = b"Before {{note|Remember this}} after.\n\n{{Quote|author=Someone|text=To be or not}}\n\n{{Note|{{lang|fr|Bonjour}} friend}}\n{{#if:{{{x|}}}|Nope}} {{Other|text=Not listed}} plain.\n";
    assert_eq!(
        texts(&f, src),
        [
            "Before \u{27e6}x1\u{27e7}Remember this\u{27e6}x2\u{27e7} after.",
            "To be or not",
            "\u{27e6}x1\u{27e7} friend",
            "\u{27e6}x1\u{27e7} \u{27e6}x2\u{27e7} plain."
        ]
    );
    let out = build_str(
        &f,
        src,
        &[(0, "Avant \u{27e6}x1\u{27e7}Souviens-toi\u{27e6}x2\u{27e7} apres."), (1, "Etre ou ne pas etre"), (2, "\u{27e6}x1\u{27e7} ami")],
    );
    assert!(
        out.starts_with("Avant {{note|Souviens-toi}} apres.\n\n{{Quote|author=Someone|text=Etre ou ne pas etre}}\n\n{{Note|{{lang|fr|Bonjour}} ami}}\n"),
        "{out}"
    );
    assert_eq!(build(&f, src, &[]), src);
    let mw_out = build_str(&f, MW.as_bytes(), &[(0, "La tour Eiffel la nuit")]);
    assert!(mw_out.contains("| caption = La tour Eiffel la nuit\n"));
    // nothing listed: protected
    assert!(texts(&mw(), b"{{Quote|text=To be}}\n").is_empty());
}

#[test]
fn gate_inert_markup() {
    let f = mw();
    let d = dw();
    let src = b"== Intro ==\nThis is a '''bold''' [[Page|link]] text.\n\n* item one\n";
    let out = build_str(
        &f,
        src,
        &[
            (1, "* Da \u{27e6}g1\u{27e7}fett\u{27e6}/g1\u{27e7} {{Delete}} [[Category:Spam]] <span>x</span> ''y'' ~~~~"),
            (2, "#REDIRECT [[Evil]]"),
        ],
    );
    assert_eq!(
        out,
        "== Intro ==\n<nowiki>*</nowiki> Da '''fett''' <nowiki>{{</nowiki>Delete<nowiki>}}</nowiki> <nowiki>[[</nowiki>Category:Spam<nowiki>]]</nowiki> <nowiki><</nowiki>span>x<nowiki><</nowiki>/span> <nowiki>''</nowiki>y<nowiki>''</nowiki> <nowiki>~~~~</nowiki>\n\n* <nowiki>#</nowiki>REDIRECT <nowiki>[[</nowiki>Evil<nowiki>]]</nowiki>\n"
    );
    let dsrc = b"====== Head ======\nSome **bold** text.\n";
    let dout = build_str(&d, dsrc, &[(1, "> **x** {{a.png}} <html>h</html> ~~NOTOC~~ https://x.y")]);
    assert_eq!(
        dout,
        "====== Head ======\n<nowiki>></nowiki> <nowiki>**</nowiki>x<nowiki>**</nowiki> <nowiki>{{</nowiki>a.png<nowiki>}}</nowiki> <nowiki><</nowiki>html>h<nowiki><</nowiki>/html> <nowiki>~~</nowiki>NOTOC<nowiki>~~</nowiki> <nowiki>https://</nowiki>x.y\n",
        "Bilingua (gate): a URL typed in the translation is written inert too, so it cannot add a link; LocOS leaves it live"
    );
    let q = MediaWiki::with_template_params("Quote:text");
    let tout = build_str(&q, b"{{Quote|author=A|text=To be}}\n", &[(0, "x=y|z")]);
    assert_eq!(tout, "{{Quote|author=A|text=x{{=}}y{{!}}z}}\n");
}

#[test]
fn cjk_bare_url_699() {
    let f = mw();
    let d = dw();
    let src = b"Read https://example.org/manual first and see https://x.org.\n";
    assert_eq!(texts(&f, src), ["Read \u{27e6}x1\u{27e7} first and see \u{27e6}x2\u{27e7}"]);
    let out = build_str(&f, src, &[(0, "\u{8bf7}\u{9605}\u{8bfb}\u{27e6}x1\u{27e7}\u{624b}\u{518c}\u{ff0c}\u{53c2}\u{89c1}\u{27e6}x2\u{27e7}\u{3002}")]);
    assert_eq!(out, "\u{8bf7}\u{9605}\u{8bfb}<nowiki/>https://example.org/manual<nowiki/>\u{624b}\u{518c}\u{ff0c}\u{53c2}\u{89c1}<nowiki/>https://x.org.<nowiki/>\u{3002}\n");
    let out = build_str(&f, src, &[(0, "\u{300c}\u{27e6}x1\u{27e7}\u{300d}\u{3068}\u{27e6}x2\u{27e7}")]);
    assert_eq!(out, "\u{300c}https://example.org/manual<nowiki/>\u{300d}\u{3068}<nowiki/>https://x.org.\n");
    let out = build_str(&f, src, &[(0, "Lisez\u{27e6}x1\u{27e7} puis \u{27e6}x2\u{27e7}")]);
    assert_eq!(out, "Lisez<nowiki/>https://example.org/manual puis https://x.org.\n");
    let out = build_str(&f, src, &[(0, "Lisez \u{27e6}x1\u{27e7} d'abord, voir (\u{27e6}x2\u{27e7})")]);
    assert_eq!(out, "Lisez https://example.org/manual d'abord, voir (https://x.org.)\n");
    let dsrc = b"Read https://example.org/manual first.\n";
    let dunit = texts(&d, dsrc).remove(0);
    let dzh = dunit.replace("Read ", "\u{8bf7}\u{9605}\u{8bfb}").replace(" first.", "\u{624b}\u{518c}\u{3002}");
    assert!(!String::from_utf8(build(&d, dsrc, &[(0, &dzh)])).unwrap().contains("<nowiki/>"));
}

#[test]
fn encodings() {
    let f = mw();
    let data: Vec<u8> = b"Caf\xe9 cr\xe8me paragraph.\n".to_vec();
    let doc = extract(&f, &data).unwrap();
    assert_eq!(plain_text(&doc.units[0].source()), "Caf\u{e9} cr\u{e8}me paragraph.");
    assert!(matches!(&doc.skeleton, Skeleton::Text { encoding, .. } if encoding == "windows-1252"));
    assert_eq!(build(&f, &data, &[(0, "Caf\u{e9} traduit.")]), b"Caf\xe9 traduit.\n");
    let bin = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x81\x8d\x00".repeat(20);
    assert!(matches!(extract(&dw(), &bin), Err(Error::Encoding(_))));
}

#[test]
fn caps() {
    let lim = |units: usize, chars: usize| ExtractOptions { limits: Some(Limits { max_units: units, max_unit_chars: chars, ..Limits::default() }), ..Default::default() };
    let mw_many: String = (0..20).map(|i| format!("* item {i}\n")).collect();
    let dw_many: String = (0..20).map(|i| format!("  * item {i}\n")).collect();
    let braces = format!("{}\n", "{{".repeat(300));
    for (f, many) in [(&mw() as &dyn Filter, mw_many.as_bytes()), (&dw() as &dyn Filter, dw_many.as_bytes()), (&mw() as &dyn Filter, braces.as_bytes())] {
        assert!(matches!(f.extract(many, &lim(5, 100_000)), Err(Error::Limit(_))), "segment or markup cap");
    }
    let long = format!("{}\n", "word ".repeat(30));
    assert!(matches!(mw().extract(long.as_bytes(), &lim(50_000, 50)), Err(Error::Limit(e)) if e.kind == LimitKind::UnitChars));
}

#[test]
fn hostile() {
    let rep = |s: &str, n: usize| s.repeat(n);
    let mut mw_cases: Vec<String> = vec![
        rep("{{", 10000) + "\n",
        rep("}}", 10000) + "\n",
        rep("{{a|", 5000) + "x" + &rep("}}", 5000) + "\n",
        rep("[[", 10000) + "\n",
        rep("[[a|", 5000) + "\n",
        rep("''", 10000) + "x\n",
        rep("'''a ", 8000) + "\n",
        rep("<!--", 10000) + "\n",
        rep("<nowiki>", 5000) + "\n",
        format!("<pre {}\n", "a".repeat(20000)),
        format!("{{|\n{}\n|}}\n", "|".repeat(20000)),
        format!("; {}:\n", "[".repeat(10000)),
        rep("=", 20000) + "\n",
        format!("[http://{}\n", "a".repeat(20000)),
        format!("text {}\n", rep("<a ", 8000)),
    ];
    let dw_cases: Vec<String> = vec![
        rep("%%", 10000) + "x\n",
        rep("**", 10000) + "\n",
        rep("//", 10000) + "\n",
        rep("((", 10000) + "\n",
        rep("^", 20000) + "\n",
        rep("| [[", 5000) + "\n",
        rep("<code", 5000) + "\n",
        rep("~~NOTOC", 5000) + "\n",
    ];
    let t0 = std::time::Instant::now();
    let mut jobs: Vec<(&dyn Filter, String)> = Vec::new();
    let (m, d) = (mw(), dw());
    for c in mw_cases.drain(..) {
        jobs.push((&m, c));
    }
    for c in &dw_cases {
        jobs.push((&d, c.clone()));
    }
    for (f, data) in jobs {
        let doc = extract(f, data.as_bytes()).unwrap_or_else(|e| panic!("{e}: {}", &data[..data.len().min(30)]));
        assert_eq!(f.merge(&doc).unwrap(), data.as_bytes());
        let t = texts(f, data.as_bytes());
        let echo: Vec<(usize, &str)> = t.iter().enumerate().map(|(i, s)| (i, s.as_str())).collect();
        assert_eq!(build(f, data.as_bytes(), &echo), data.as_bytes());
    }
    assert!(t0.elapsed().as_secs() < 60, "{:?}", t0.elapsed());
}

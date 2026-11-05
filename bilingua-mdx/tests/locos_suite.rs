//! The LocOS MDX suite (`tests/verify_mdx_filter.py`) ported case by case. The suite has 15 test functions; the 13
//! that test the filter are here (identity, walk, tokens, translate, attrs, markdown parity, cjk emphasis, cjk
//! autolinks, block start, table pipe, line breaks, a11y, hostile). `surfaces` and `end_to_end` test LocOS's own
//! registry, upload page, API and CLI and have no equivalent in this crate (the facade contract test covers the
//! registry side).
mod common;
use bilingua_mdx::{Mdx, MdxOptions};
use bilingua_model::locos::to_locos_text;
use bilingua_model::*;
use common::*;

fn s(b: &[u8]) -> String {
    String::from_utf8(b.to_vec()).unwrap()
}

#[test]
fn identity() {
    let src = guide();
    let crlf = s(&src).replace('\n', "\r\n").into_bytes();
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend_from_slice(&src);
    let mut trimmed = src.clone();
    while trimmed.ends_with(b"\n") {
        trimmed.pop();
    }
    for v in [src.clone(), crlf, bom, trimmed, b"".to_vec(), b"   \n\n".to_vec()] {
        assert_eq!(Mdx.merge(&extract(&v).unwrap()).unwrap(), v);
        assert_identity(&v);
    }
    // BOM does not hide a first-line import
    assert_eq!(texts(b"\xef\xbb\xbfimport X from 'x'\n\nHello\n"), ["Hello"]);
}

#[test]
fn walk() {
    let t = texts(&guide());
    assert_eq!(t[0], "Getting started", "{t:?}"); // front matter like md (#731: the value)
    assert!(t.contains(&"Make sure the server is **running** first.".to_string()));
    let joined = t.join("\n");
    for gone in ["import", "export", "author", "Ada", "props.items", "comment the translator", "Not translated", "Hidden", "Chart", "data="] {
        assert!(!joined.contains(gone), "{gone}: {t:?}");
    }
    // ESM block runs to the blank line, even with a non-keyword line inside
    assert_eq!(texts(b"export const a = {\n  b: 'Hello world'\n}\nstill esm text\n\nReal prose.\n"), ["Real prose."]);
    // a word that merely starts with "import" is prose
    assert_eq!(texts(b"Importing data is easy.\n"), ["Importing data is easy."]);
    assert_eq!(texts(b"import\n"), ["import"]); // no space: not ESM (mdxjs-esm)
    // ESM cannot interrupt a paragraph: a wrapped prose line starting "export " stays prose
    assert_eq!(
        texts(b"When the run ends you can\nexport the file as PDF\nor keep it.\n\nexport const x = 1\n"),
        ["When the run ends you can", "export the file as PDF", "or keep it."]
    );
    // ... but after a heading, a rule or a flow tag it is ESM again
    assert_eq!(texts(b"# Title\nimport A from 'a'\n\nBody.\n"), ["Title", "Body."]);
    assert_eq!(texts(b"<Note />\nexport const y = 2\n\nBody.\n"), ["Body."]);
    // multi-line flow tag and flow expression
    assert_eq!(texts(b"<Tabs\n  items={['a', 'b']}\n>\n  <Tab>\n  Tab one text.\n  </Tab>\n</Tabs>\n"), ["Tab one text."]);
    assert_eq!(texts(b"{\n  // a } in a comment\n  '}'\n}\n\nAfter.\n"), ["After."]);
    // two tags on one line, nothing else: flow
    assert!(texts(b"<A></A>\n<B /> <C />\n").is_empty());
}

#[test]
fn tokens() {
    let doc = extract(b"Press <Kbd>Ctrl</Kbd> and {count} more <Icon name=\"x\" /> now.\n").unwrap();
    let (text, map) = to_locos_text(&doc.units[0].source());
    assert_eq!(text, "Press \u{27E6}g1\u{27E7}Ctrl\u{27E6}/g1\u{27E7} and \u{27E6}x2\u{27E7} more \u{27E6}x3\u{27E7} now.");
    assert_eq!((map["1"].open.as_deref(), map["1"].close.as_deref()), (Some("<Kbd>"), Some("</Kbd>")));
    assert_eq!(map["2"].open.as_deref(), Some("{count}"));
    assert_eq!(doc.units.len(), 1);
    // unpaired open and orphan close -> points; nested pairs
    assert_eq!(texts(b"a <B>b <I>c</I> d\n")[0], "a \u{27E6}x1\u{27E7}b \u{27E6}g2\u{27E7}c\u{27E6}/g2\u{27E7} d");
    assert_eq!(texts(b"x </B> y\n")[0], "x \u{27E6}x1\u{27E7} y");
    // a line starting with inline JSX followed by text is a text line
    assert_eq!(texts(b"<Kbd>Ctrl</Kbd> copies.\n"), ["\u{27E6}g1\u{27E7}Ctrl\u{27E6}/g1\u{27E7} copies."]);
    // code spans and escapes stay text
    assert_eq!(texts(b"Use `{x}` and \\{y\\} here.\n"), ["Use `{x}` and \\{y\\} here."]);
    // multi-line inline expression: one token, the line unit extends over it
    let src = b"Total: {items\n  .length} items.\nNext line.\n";
    assert_eq!(texts(src), ["Total: \u{27E6}x1\u{27E7} items.", "Next line."]);
    let out = build_all(src, |t| t.to_uppercase().replace("\u{27E6}X1\u{27E7}", "\u{27E6}x1\u{27E7}"));
    assert_eq!(s(&out), "TOTAL: {items\n  .length} ITEMS.\nNEXT LINE.\n");
    // braces in strings / template literals inside an expression
    assert_eq!(texts(b"Say {'}' + `{`} hi.\n"), ["Say \u{27E6}x1\u{27E7} hi."]);
    // a line of only tokens has no text: not a unit
    assert_eq!(texts(b"Hi {a} <B/>\n{a} <B/> x\n"), ["Hi \u{27E6}x1\u{27E7} \u{27E6}x2\u{27E7}", "\u{27E6}x1\u{27E7} \u{27E6}x2\u{27E7} x"]);
}

#[test]
fn translate() {
    let src = b"# Hello {name}\n\nPress <Kbd>Ctrl</Kbd> + <Kbd>C</Kbd>.\n";
    assert_eq!(extract(src).unwrap().units.len(), 2);
    let out = build(src, &[(0, "Bonjour \u{27E6}x1\u{27E7}"), (1, "Appuyez \u{27E6}g2\u{27E7}C\u{27E6}/g2\u{27E7} puis \u{27E6}g1\u{27E7}Ctrl\u{27E6}/g1\u{27E7} \u{27E6}x9\u{27E7}.")]);
    assert_eq!(s(&out), "# Bonjour {name}\n\nAppuyez <Kbd>C</Kbd> puis <Kbd>Ctrl</Kbd> .\n");
    // escaping: a brace / < the translation adds; code spans untouched; existing escapes kept
    let out = build(b"Plain text.\n", &[(0, "a {b} <c> `{d}` \\{e")]);
    assert_eq!(s(&out), "a \\{b\\} \\<c> `{d}` \\{e\n");
    // the md prefix is kept
    assert_eq!(s(&build(b"> - **Bold** item\n", &[(0, "**Gras** element")])), "> - **Gras** element\n");
    // CRLF and BOM kept around a translated line
    assert_eq!(build(b"\xef\xbb\xbfOne\r\nTwo\r\n", &[(1, "Deux")]), b"\xef\xbb\xbfOne\r\nDeux\r\n");
}

const ATTR_SRC: &str = "<Note title=\"Before you begin\" type=\"info\" label={x}>\nBody text.\n</Note>\n<Image alt=\"A cat\" src=\"/cat.png\" caption='Our cat' />\nThe <Abbr title=\"Application interface\">API</Abbr> is stable.\n<Thing description=\"Not on the list\" />\n";

#[test]
fn attrs() {
    let src = ATTR_SRC.as_bytes();
    let doc = extract(src).unwrap();
    let t: Vec<String> = doc.units.iter().map(locos_text).collect();
    assert_eq!(t, ["Body text.", "The \u{27E6}g1\u{27E7}API\u{27E6}/g1\u{27E7} is stable.", "Before you begin", "A cat", "Our cat", "Application interface"]);
    let styles: Vec<Option<&str>> = doc.units.iter().map(|u| u.context.style.as_deref()).collect();
    assert!(styles[..2].iter().all(|s| s.is_none()));
    assert!(styles[2..].iter().all(|s| *s == Some("a11y:mdx-attr")));
    let out = build(src, &[(2, "Avant \"tout\""), (3, "Un chat"), (5, "Interface d'application")]);
    let o = s(&out);
    assert!(o.contains("title=\"Avant &quot;tout&quot;\""), "{o}");
    assert!(o.contains("alt=\"Un chat\"") && o.contains("caption='Our cat'"));
    assert!(o.contains("<Abbr title=\"Interface d'application\">API</Abbr>"), "{o}");
    // attribute inside a TRANSLATED line
    let o = s(&build(src, &[(1, "L'\u{27E6}g1\u{27E7}API\u{27E6}/g1\u{27E7} est stable."), (5, "Interface")]));
    assert!(o.contains("L'<Abbr title=\"Interface\">API</Abbr> est stable.\n"), "{o}");
    // the translation dropped the token holding a translated attribute: no overlap damage
    let o = s(&build(src, &[(1, "Plus de balise."), (5, "Interface")]));
    assert!(o.contains("\nPlus de balise.\n<Thing") && !o.contains("Interface"), "{o}");
    // a duplicated token carries the translated attribute both times
    let o = s(&build(src, &[(1, "\u{27E6}g1\u{27E7}API\u{27E6}/g1\u{27E7} \u{27E6}g1\u{27E7}API\u{27E6}/g1\u{27E7}"), (5, "I")]));
    assert_eq!(o.matches("<Abbr title=\"I\">API</Abbr>").count(), 2, "{o}");
    // single-quoted value with a quote in the translation
    assert!(s(&build(src, &[(4, "Notre 'chat'")])).contains("caption='Notre &#39;chat&#39;'"));
    // an attribute of an untranslated line still builds
    assert!(s(&build(src, &[(5, "Interface")])).contains("<Abbr title=\"Interface\">API</Abbr> is stable."));
    // allowlist
    let m = MdxOptions { attrs: vec!["description".into()], ..Default::default() };
    let t: Vec<String> = extract_with(src, &m).unwrap().units.iter().map(locos_text).collect();
    assert_eq!(t, ["Body text.", "The \u{27E6}g1\u{27E7}API\u{27E6}/g1\u{27E7} is stable.", "Not on the list"]);
}

#[test]
fn markdown_parity() {
    // The units LocOS's md filter makes of this plain Markdown, and what the md and mdx builds write.
    let src = b"# Title\n\nSome *prose* here.\n\n- item one\n- item two\n\n---\n\n| a | b |\n|---|---|\n| c | d |\n\n```\ncode\n```\n\n> quoted line\n1. first\n![alt](x.png)\n";
    let want = ["Title", "Some *prose* here.", "item one", "item two", "| a | b |", "| c | d |", "quoted line", "first", "![alt](x.png)"];
    assert_eq!(texts(src), want);
    let out = build_all(src, |t| format!("X {t}"));
    assert_eq!(
        s(&out),
        "# X Title\n\nX Some *prose* here.\n\n- X item one\n- X item two\n\n---\n\nX | a | b |\n|---|---|\nX | c | d |\n\n```\ncode\n```\n\n> X quoted line\n1. X first\nX ![alt](x.png)\n"
    );
}

#[test]
fn cjk_emphasis() {
    let src = b"Choose __Advanced__ options.\n\nClick **\"Save\"** now.\n\nThe _quick_ fix.\n\nPress **(Ctrl+S)** to save.\n\nUse my_var and *it* here.\n";
    let tr = [
        (0, "__詳細__オプションを選択。"),
        (1, "**「保存」**をクリック。"),
        (2, "_素早い_解決策。"),
        (3, "**（Ctrl+S）**を押す。"),
        (4, "my_varと*斜体*を使う。"),
    ];
    let want = "**詳細**オプションを選択。\n\n「**保存**」をクリック。\n\n*素早い*解決策。\n\n（**Ctrl+S**）を押す。\n\nmy_varと*斜体*を使う。\n";
    assert_eq!(s(&build(src, &tr)), want);
    // a source line already in the target spelling is left alone
    let cjk = "日本__語__です\n";
    assert_eq!(s(&build(cjk.as_bytes(), &[(0, "日本__語__です")])), cjk);
}

#[test]
fn cjk_autolinks() {
    let src = b"Hosted at https://github.com/example/project and mirrored on https://x.org/p.\n\nRead [the manual](https://example.org/manual) and `https://a.b` now.\n";
    let tr = [(0, "托管在 https://github.com/example/project，镜像在、https://x.org/p。"), (1, "阅读[手册](https://example.org/manual)和`https://a.b`。")];
    let mdx = s(&build(src, &tr));
    assert!(mdx.contains("在 [https://github.com/example/project](https://github.com/example/project)，"), "{mdx}");
    assert!(mdx.contains("、[https://x.org/p](https://x.org/p)。"), "{mdx}");
    assert!(mdx.contains("阅读[手册](https://example.org/manual)和`https://a.b`。\n"), "{mdx}");
    // spaces around the URL, or only trailing ASCII punctuation: unchanged
    let fr = "Hébergé sur https://github.com/example/project, miroir https://x.org/p.";
    assert!(s(&build(src, &[(0, fr)])).starts_with(&format!("{fr}\n")));
    // a www. URL becomes [www.x](http://www.x)
    assert_eq!(s(&build(b"See www.example.org now.\n", &[(0, "参见www.example.org。")])), "参见[www.example.org](http://www.example.org)。\n");
    // a URL used as inline link text stays as written
    let l = "打开[https://x.org](https://x.org)。";
    assert_eq!(s(&build(b"Open [https://x.org](https://x.org) now.\n", &[(0, l)])), format!("{l}\n"));
}

#[test]
fn block_start() {
    let src = b"1984\\. That was the year.\n\n\\# Not a heading\n\n- Item one\n\n## Heading two\n\nPlain line.\n\n> Quoted line.\n\nKeep 1984\\. here.\n";
    let tr = [
        (0, "1984. Das war das Jahr."),
        (1, "# Keine Überschrift"),
        (2, "1. Punkt eins"),
        (3, "1. Überschrift zwei"),
        (4, "- Einfache Zeile."),
        (5, "> Zitierte Zeile."),
        (6, "Behalte 1984\\. hier."),
    ];
    let want = "1984\\. Das war das Jahr.\n\n\\# Keine Überschrift\n\n- 1\\. Punkt eins\n\n## 1. Überschrift zwei\n\n\\- Einfache Zeile.\n\n> \\> Zitierte Zeile.\n\nBehalte 1984\\. hier.\n";
    assert_eq!(texts(src).len(), 7);
    assert_eq!(s(&build(src, &tr)), want);
    // an escape the engine kept, a number with no delimiter, a source that already starts with a marker-looking run
    let keep = [(0, "1984\\. Das war das Jahr."), (1, "2024 war ein gutes Jahr"), (4, "-5 Grad draußen.")];
    let got = s(&build(src, &keep));
    assert!(got.starts_with("1984\\. Das war das Jahr.\n\n2024 war ein gutes Jahr\n"), "{got}");
    assert!(got.contains("\n-5 Grad draußen.\n"), "{got}");
    // only ASCII digits open an ordered list in CommonMark
    for year in ["١٩٨٤. كانت تلك السنة.", "１９８４。それは年でした。", "１９８４. x"] {
        let got = s(&build(src, &[(0, year)]));
        assert!(got.starts_with(&format!("{year}\n")), "{year}: {got}");
    }
}

#[test]
fn table_pipe_escape() {
    let src = b"| Operator | Meaning |\n|---|---|\n| Yes \\| No | Answer with yes or no only |\n| a \\|\\| b | Run b only when a fails |\n| Plain | A cell with no pipe |\n";
    let tr = [
        (0, "| Opérateur | Signification |"),
        (1, "| Oui | Non | Répondez par oui ou par non uniquement |"),
        (2, "| a || b | Exécuter b uniquement si a échoue |"),
        (3, "| Simple | Une cellule sans tuyau |"),
    ];
    let want = "| Opérateur | Signification |\n|---|---|\n| Oui \\| Non | Répondez par oui ou par non uniquement |\n| a \\|\\| b | Exécuter b uniquement si a échoue |\n| Simple | Une cellule sans tuyau |\n";
    assert_eq!(texts(src).len(), 4);
    assert_eq!(s(&build(src, &tr)), want);
    // the engine kept the backslash and dropped the pipe
    let got = s(&build(src, &[(1, "| Oui \\ Non | Répondez par oui ou non |"), (2, "| a \\ \\ b | Exécuter b |")]));
    assert!(got.contains("| Oui \\| Non | Répondez par oui ou non |\n"), "{got}");
    assert!(got.contains("| a \\| \\| b | Exécuter b |\n"), "{got}");
    // a lone backslash the source also has is not touched
    let bsrc = b"| A | B |\n|---|---|\n| C:\\ dir \\| x | y |\n";
    assert!(s(&build(bsrc, &[(1, "| C:\\ dossier \\| x | y |")])).ends_with("| C:\\ dossier \\| x | y |\n"));
    // kept escapes, an engine that moved the escaped pipe, the same cell count: unchanged
    let got = s(&build(src, &[(1, "| Oui \\| Non | Répondez par oui ou par non |"), (2, "| Répondez | a \\|\\| b |")]));
    assert!(got.contains("| Oui \\| Non | Répondez par oui ou par non |\n"), "{got}");
    assert!(got.contains("| Répondez | a \\|\\| b |\n"), "{got}");
    // a source with no escaped pipe is never touched, even when the row gains a cell
    let plain = b"| A | B |\n|---|---|\n| x | y |\n";
    assert!(s(&build(plain, &[(1, "| x | y | z |")])).ends_with("| x | y | z |\n"));
}

#[test]
fn line_breaks_714() {
    let src = b"# Getting started\n\n| A | B |\n|---|---|\n| Yes | Answer here |\n\n- Item one\n\nPlain para.\n";
    let tr = [(0, "Erste\nSchritte"), (1, "| A | B |"), (2, "| Ja | Antwort \r\n hier |"), (3, "Punkt\n- eins"), (4, "Absatz\nzwei.\n")];
    let want = "# Erste Schritte\n\n| A | B |\n|---|---|\n| Ja | Antwort hier |\n\n- Punkt - eins\n\nAbsatz zwei.\n";
    let got = build(src, &tr);
    assert_eq!(s(&got), want);
    assert_eq!(extract(&got).unwrap().units.len(), 5);
    assert_eq!(Mdx.merge(&extract(src).unwrap()).unwrap(), src);
}

#[test]
fn a11y() {
    let src = b"Intro.\n\n![](a/empty.png)\n\n<img src=\"/x/logo.png\" />\n<Image alt=\"\" src=\"y.png\"/>\n<img alt={t} src=\"z\"/>\n";
    let doc = extract(src).unwrap();
    let mut kinds: Vec<(String, String)> = doc
        .warnings
        .iter()
        .filter(|w| w.kind == "missing_alt")
        .map(|w| (w.part.clone(), ["empty.png", "y.png", "logo.png"].iter().find(|n| w.message.contains(**n)).unwrap().to_string()))
        .collect();
    kinds.sort();
    let want: Vec<(String, String)> = [("empty", "empty.png"), ("empty", "y.png"), ("missing", "logo.png")].iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    assert_eq!(kinds, want);
    assert_eq!(doc.warnings.len(), 3);
}

#[test]
fn hostile() {
    // unclosed brace / tag: read as text, round trip intact
    let src = b"Open { never closed.\nA <Tag never closed\n";
    assert_eq!(texts(src), ["Open { never closed.", "A <Tag never closed"]);
    assert_eq!(Mdx.merge(&extract(src).unwrap()).unwrap(), src);
    // a flood of unclosed braces is refused, not rescanned forever
    let flood = "x {\n".repeat(400_000);
    assert!(matches!(extract(flood.as_bytes()), Err(Error::Limit(_))));
    // segment cap
    let mut o = opts();
    o.limits = Some(Limits::new().with_max_units(5));
    let many: String = (0..10).map(|i| format!("Line {i}.\n\n")).collect();
    assert!(matches!(Mdx.extract(many.as_bytes(), &o), Err(Error::Limit(_))));
    // one huge line
    let huge = "a".repeat(Limits::default().max_unit_chars + 10);
    assert!(matches!(extract(huge.as_bytes()), Err(Error::Limit(_))));
    // binary refused; Windows-1252 converted
    let bin: Vec<u8> = [0u8, 1, 2, 0xff].iter().copied().cycle().take(200).collect();
    assert!(matches!(extract(&bin), Err(Error::Encoding(_))));
    let cp = b"Caf\xe9 <B>cr\xe8me</B>.\n";
    let doc = extract(cp).unwrap();
    assert_eq!(locos_text(&doc.units[0]), "Café \u{27E6}g1\u{27E7}crème\u{27E6}/g1\u{27E7}.");
    assert!(matches!(&doc.skeleton, Skeleton::Text { encoding, .. } if encoding.contains("1252")));
    assert_eq!(Mdx.merge(&doc).unwrap(), cp);
}

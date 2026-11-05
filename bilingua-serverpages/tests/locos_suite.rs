//! The LocOS suite `tests/verify_server_page_filter.py`, case by case (its 13 test functions, 12 numbered checks and
//! the #672 extra), on the same fixture pages (`tests/data/suite.*`, written by `tools/make_fixtures.py` from the
//! suite's literals).
mod common;
use bilingua_model::*;
use bilingua_serverpages::*;
use common::*;

const X1: &str = "\u{27E6}x1\u{27E7}";
const X2: &str = "\u{27E6}x2\u{27E7}";
const X3: &str = "\u{27E6}x3\u{27E7}";

fn fixture(name: &str) -> String {
    String::from_utf8(std::fs::read(data_dir().join(name)).unwrap()).unwrap()
}

fn samples() -> Vec<(&'static str, String)> {
    vec![
        ("phtml", fixture("suite.phtml")),
        ("jsp", fixture("suite.jsp")),
        ("aspx", fixture("suite.aspx")),
        ("erb", fixture("suite.erb")),
        ("shtml", fixture("suite.shtml")),
    ]
}

fn units(fmt: &str, src: &str) -> Vec<String> {
    texts_str(fmt, src)
}

/// Builds with a mapping keyed by unit index, like the LocOS `_build(fmt, src, {"0": ...})`.
fn bld(fmt: &str, src: &str, m: &[(usize, &str)]) -> String {
    build_str(fmt, src, m)
}

#[test]
fn registry() {
    // 1) the ten extensions, the right dialect for each
    let want: &[(&str, &[&str])] = &[
        ("phtml", &["phtml"]),
        ("jsp", &["jsp", "jspf"]),
        ("aspx", &["aspx", "ascx"]),
        ("erb", &["erb"]),
        ("shtml", &["shtml", "shtm", "stm"]),
    ];
    let mut all = Vec::new();
    for (fmt, exts) in want {
        let f = filter(fmt);
        assert_eq!(f.id(), *fmt);
        assert_eq!(f.extensions(), *exts);
        all.extend(exts.iter().copied());
    }
    for e in ["phtml", "jsp", "jspf", "aspx", "ascx", "erb", "shtml", "shtm", "stm"] {
        assert!(all.contains(&e), "{e}");
    }
    // .php routes by content: a page template is a PHP template; PHP code with no page text is not one
    assert!(php_has_page_text(b"<p>Hello <?= $n ?></p>\n").unwrap());
    assert!(!php_has_page_text(b"<?php\nfunction f() { return 1; }\n").unwrap());
    assert!(!php_has_page_text(b"<?php\nreturn ['welcome' => 'Welcome back'];\n").unwrap());
    let tpl = "<p>Hello <?= $n ?></p>\n";
    assert_eq!(units("phtml", tpl), vec![format!("Hello {X1}")]);
    assert_eq!(bld("phtml", tpl, &[(0, &format!("Salut {X1}"))]), "<p>Salut <?= $n ?></p>\n");
}

#[test]
fn identity_roundtrip() {
    // 2) untouched and identity-translated round trips are byte-identical (5 dialects)
    for (fmt, src) in samples() {
        assert_identity(fmt, src.as_bytes());
    }
}

#[test]
fn units_of_every_dialect() {
    // 3) output expressions are point tokens; statements, directives, comments are not text
    let php = units("phtml", &fixture("suite.phtml"));
    assert!(php.contains(&format!("Hello {X1}, you have \u{27E6}g2\u{27E7}{X3}\u{27E6}/g2\u{27E7} new messages.")), "{php:?}");
    assert!(php.contains(&"Please sign in.".to_string()));
    assert!(php.contains(&"Open the inbox".to_string())); // quoted title attribute without code
    assert!(!php.iter().any(|u| u.contains('$') || u.contains("?>") || u.contains("EOT")), "{php:?}");
    assert!(!php.iter().any(|u| u.contains("Not text")));
    let jsp = units("jsp", &fixture("suite.jsp"));
    assert!(jsp.contains(&format!("Welcome back, {X1}! Today is {X2}.")), "{jsp:?}");
    assert!(jsp.contains(&"Item".to_string()) && !jsp.iter().any(|u| u.contains("String") || u.contains("markup")), "{jsp:?}");
    assert!(jsp.contains(&"Price: \\${not.el} only".to_string()), "{jsp:?}"); // escaped EL is literal text
    let aspx = units("aspx", &fixture("suite.aspx"));
    assert!(aspx.contains(&format!("Hi {X1}, and {X2} is ready.")), "{aspx:?}");
    assert!(aspx.contains(&"Shown only sometimes".to_string()));
    let erb = units("erb", &fixture("suite.erb"));
    assert!(erb.contains(&format!("Only {X1} today")) && erb.contains(&format!("Literal {X1} marker and {X2} end")), "{erb:?}");
    assert!(!erb.iter().any(|u| u.contains("comment")));
    let ssi = units("shtml", &fixture("suite.shtml"));
    assert_eq!(ssi, vec![format!("Last updated {X1} by the team.")]);
}

#[test]
fn translated_roundtrip() {
    // 4) code byte-identical, attribute escaped, no token leaks
    let src = fixture("suite.phtml");
    let t = texts_str("phtml", &src);
    let g2 = "\u{27E6}g2\u{27E7}";
    let g2c = "\u{27E6}/g2\u{27E7}";
    let owned: Vec<String> = t
        .iter()
        .map(|u| {
            if u.starts_with("Hello") {
                format!("Bonjour {X1}, vous avez {g2}{X3}{g2c} nouveaux messages.")
            } else if u == "Open the inbox" {
                "Ouvrir la boite \"<b>\"".to_string()
            } else {
                format!("FR {u}")
            }
        })
        .collect();
    let m: Vec<(usize, &str)> = owned.iter().enumerate().map(|(i, s)| (i, s.as_str())).collect();
    let out = bld("phtml", &src, &m);
    assert!(out.contains("Bonjour <?= htmlspecialchars($user) ?>, vous avez <b><?php echo $n; ?></b> nouveaux"), "{out}");
    assert!(out.contains("<a href=\"<?php echo url(\"a\\\"b>\"); ?>\" title=\"Ouvrir la boite &quot;&lt;b&gt;&quot;\">"), "{out}");
    assert!(out.contains("<img src=\"logo.png\" alt=\"<?= $alt ?>\">"));
    assert!(out.contains("<?php if ($user): ?>") && out.contains("<?php endif; ?>"));
    assert!(out.contains("$block = <<<EOT\n<p>Not text ?> here</p>\nEOT;"));
    assert!(out.contains("<script>var n = <?= json_encode($n) ?>; if (a <b) {}</script>"));
    assert!(!out.contains('\u{27E6}') && !out.contains('\u{27E7}'), "{out}");
}

/// Sets a target with the tags exactly as given (a hostile or buggy translation), bypassing `set_target`.
fn raw_target(fmt: &str, src: &str, unit: usize, target: Vec<Inline>) -> String {
    let f = filter(fmt);
    let mut doc = f.extract(src.as_bytes(), &opts()).unwrap();
    doc.units[unit].segments[0].target = target;
    String::from_utf8(f.merge(&doc).unwrap()).unwrap()
}

fn tag(id: &str, kind: TagKind) -> Inline {
    Inline::tag(id, kind, "")
}

#[test]
fn tokens_cannot_lose_or_inject_code() {
    // 5) dropped/duplicated tokens fixed; injected PHP, EL, ERB, SSI all inert
    let src = "<p>Hello <?= $name ?>, bye.</p>";
    let out = bld("phtml", src, &[(0, "Bonjour, au revoir.")]);
    assert_eq!(out.matches("<?= $name ?>").count(), 1, "{out}"); // dropped token restored
    let out = raw_target("phtml", src, 0, vec![Inline::text("Bonjour "), tag("1", TagKind::Standalone), Inline::text(" "), tag("1", TagKind::Standalone), Inline::text(".")]);
    assert_eq!(out.matches("<?= $name ?>").count(), 1, "{out}"); // duplicate written once
    let out = bld("phtml", src, &[(0, &format!("Hi \u{FDD0}0\u{FDD1} <?php system('id'); ?> {X1}"))]);
    assert!(!out.contains("system('id'); ?>") && out.contains("&lt;?php"), "{out}");
    assert_eq!(out.matches("<?= $name ?>").count(), 1, "{out}");
    // JSP: translated text can never become EL; ERB/ASPX "<%" is escaped as markup.
    let out = bld("jsp", "<p>Hi ${u}</p>", &[(0, &format!("Salut {X1} ${{evil}} #{{evil}}"))]);
    assert!(out.contains("\\${evil}") && out.contains("\\#{evil}") && out.contains("Salut ${u}"), "{out}");
    let out = bld("erb", "<p>Hi <%= u %></p>", &[(0, &format!("Salut {X1} <%= `id` %>"))]);
    assert!(out.contains("&lt;%= `id` %&gt;"), "{out}");
    let out = bld("shtml", "<p>Hi</p>", &[(0, "<!--#exec cmd=\"id\" -->")]);
    assert!(!out.contains("<!--#exec"), "{out}");
}

#[test]
fn refusals() {
    // 6) unsafe islands refuse the file with a line number (13 cases)
    refused("phtml", "<p>Hi</p>\n<<?= $tag ?> class='x'>Hello</<?= $tag ?>>", "tag name (line 2)");
    refused("phtml", "<div <?php echo $attrs ?>>Hello</div>", "tag or attribute name");
    refused("aspx", "<input <%= disabled %> value=\"x\">", "tag or attribute name");
    refused("phtml", "<?php echo \"<div\"; ?> class=\"x\">Hello</div>", "start of an HTML tag");
    refused("phtml", "<p>Hi <?php echo 'oops ?></p>", "string is never closed");
    refused("phtml", "<p>Hi <?php /* never ?></p>", "comment is never closed");
    refused("phtml", "<?php $x = <<<EOT\nno end\n", "heredoc is never closed");
    refused("jsp", "<p>Hi <% int x = 1; </p>", "never closed");
    refused("jsp", "<p>Hi ${user.name</p>", "never closed");
    refused("jsp", "<jsp:scriptlet>int x;", "never closed");
    refused("aspx", "<p>Hi <%-- never closed</p>", "never closed");
    refused("shtml", "<p><!--#echo var='x' </p>", "never closed");
    refused("erb", "<p>\u{FDD0} x</p>", "Remove those characters");
}

#[test]
fn accepted_edge_cases() {
    // 7) legal edge cases accepted: EOF PHP, ?> in strings/comments, %%>, <?xml
    for src in [
        "<p>Hi</p>\n<?php\nfoo();\n",
        "<p>Price <?php echo \"?>\"; ?> here</p>",
        "<p>A <?php // note ?> B</p>",
        "<p>A <?php # note ?> B <?php #[Attr] ?></p>",
        "<?xml version=\"1.0\"?>\n<p>Text</p>",
    ] {
        assert_identity("phtml", src.as_bytes());
    }
    assert_eq!(units("phtml", "<p>Price <?php echo \"?>\"; ?> here</p>"), vec![format!("Price {X1} here")]);
    assert_eq!(units("phtml", "<p>A <?php // note ?> B</p>"), vec!["A".to_string(), "B".to_string()]);
    // ERB %%> inside a tag is literal; code inside quoted attribute values is fine.
    assert_eq!(units("erb", "<p>Use <%= a %%> b %> now</p>"), vec![format!("Use {X1} now")]);
    let u = units("jsp", "<a href=\"<%= u %>\" title=\"${t}\">Go home</a>");
    assert!(u[0].ends_with("Go home\u{27E6}/g1\u{27E7}"), "{u:?}");
}

#[test]
fn language_stamp() {
    // 8) target language stamped; a dynamic lang attribute is left dynamic
    let out = build_doc("phtml", b"<html lang=\"<?= $l ?>\"><p>Hi</p></html>", &[(0, "Salut")], Some("fr-FR")).unwrap();
    assert_eq!(String::from_utf8(out).unwrap(), "<html lang=\"<?= $l ?>\"><p>Salut</p></html>");
    let out = build_doc("phtml", b"<html lang=\"en\"><p>Hi <?= $n ?></p></html>", &[(0, &format!("Marhaba {X1}"))], Some("ar")).unwrap();
    assert_eq!(String::from_utf8(out).unwrap(), "<html dir=\"rtl\" lang=\"ar\"><p>Marhaba <?= $n ?></p></html>");
}

#[test]
fn encodings() {
    // 9) UTF-8 BOM byte-identical; Windows-1252 converted and recorded (and written back as it came)
    let mut bom = vec![0xEF, 0xBB, 0xBF];
    bom.extend("<p>Café <%= x %> ouvert</p>".as_bytes());
    let doc = extract("aspx", &bom).unwrap();
    assert_eq!(filter("aspx").merge(&doc).unwrap(), bom);
    assert_eq!(units("aspx", "<p>Café <%= x %> ouvert</p>"), vec![format!("Café {X1} ouvert")]);
    let cp: Vec<u8> = vec![b'<', b'p', b'>', b'C', b'a', b'f', 0xE9, b' ', b'<', b'%', b'=', b' ', b'x', b' ', b'%', b'>', b'<', b'/', b'p', b'>'];
    let doc = extract("aspx", &cp).unwrap();
    let Skeleton::Text { encoding, .. } = &doc.skeleton else { panic!() };
    assert_eq!(encoding, "windows-1252");
    assert_eq!(filter("aspx").merge(&doc).unwrap(), cp);
    assert_eq!(locos_text(&doc.units[0]), format!("Caf\u{e9} {X1}"));
}

#[test]
fn caps() {
    // 10) segment and island caps enforced; large inputs stay linear
    let f = filter("phtml");
    let small = ExtractOptions::new().with_limits(Limits::new().with_max_units(5));
    let many: String = (0..10).map(|i| format!("<p>Line {i} <?= $v ?></p>")).collect();
    assert!(matches!(f.extract(many.as_bytes(), &small), Err(Error::Limit(_))));
    let islands = format!("<p>x</p>{}", "<?php ?>".repeat(60));
    assert!(matches!(f.extract(islands.as_bytes(), &small), Err(Error::Limit(_))));
    let t = std::time::Instant::now();
    let big = format!("<p>Hi</p><?php {}1; ?><p>Bye</p>", format!("'{}' . ", "x".repeat(200000)).repeat(20));
    assert_eq!(units("phtml", &big), vec!["Hi".to_string(), "Bye".to_string()]);
    let big2 = format!("<p>a</p>{}", "<% x %>".repeat(20000));
    extract("jsp", big2.as_bytes()).unwrap();
    assert!(t.elapsed().as_secs() < 20, "{:?}", t.elapsed());
}

#[test]
fn html_without_closed_code_is_not_a_server_page() {
    // 11) the .html filter is unchanged for code-looking content with no closed island
    for src in ["<p>Save 5 <% today</p>", "<p>Price ${x} and a <? mark</p>", "<p>a %> b</p>"] {
        assert_eq!(server_page_for_html(src).unwrap(), None, "{src}");
    }
}

#[test]
fn html_with_server_code_is_protected() {
    // 11b) #672: JSP/ASP/PHP code in .html protected, never page text; mixed refused
    let jsp = fixture("suite_html_jsp.html");
    assert_eq!(server_page_for_html(&jsp).unwrap(), Some(Dialect::Jsp));
    let t = units("jsp", &jsp);
    assert_eq!(t, vec![format!("Hello {X1}, welcome."), format!("Cost {X1} today.")]);
    assert!(!t.iter().any(|x| x.contains("<%") || x.contains("page")));
    assert_identity("jsp", jsp.as_bytes());
    let out = bld("jsp", &jsp, &[(0, &format!("Bonjour {X1}, bienvenue.")), (1, &format!("Prix {X1} aujourd'hui."))]);
    assert!(out.contains("<%@ page language=\"java\" %>") && !out.contains("&lt;%"), "{out}");
    // server pages keep quotes escaped in text (an island can emit markup at runtime)
    assert!(out.contains("Bonjour <%= user %>, bienvenue.") && out.contains("Prix ${price} aujourd&#x27;hui."), "{out}");
    assert!(!out.contains("aujourd'hui"), "{out}");
    // no JSP hint: generic <% %> (ASP.NET lexer); ${...} stays ordinary text
    let asp = "<% Response.Write(1) %>\n<p>Hi <%= name %> and ${x}.</p>\n";
    assert_eq!(server_page_for_html(asp).unwrap(), Some(Dialect::Aspx));
    assert_eq!(units("aspx", asp), vec![format!("Hi {X1} and ${{x}}.")]);
    // PHP in .html: statements split units, echoes are point tokens
    let php = "<?php if ($a): ?><p>Hi <?= $n ?>, bye.</p><?php endif; ?>\n";
    assert_eq!(server_page_for_html(php).unwrap(), Some(Dialect::Phtml));
    assert_eq!(units("phtml", php), vec![format!("Hi {X1}, bye.")]);
    let out = bld("phtml", php, &[(0, &format!("Salut {X1}, <?php system('id'); ?>"))]);
    assert_eq!(out.matches("<?php").count(), 2, "{out}"); // the two statements; the injected one is text
    assert!(out.contains("&lt;?php system"), "{out}");
    let loc = build_doc("phtml", php.as_bytes(), &[(0, &format!("Salut {X1}"))], Some("fr-FR")).unwrap();
    assert_eq!(loc.windows(2).filter(|w| *w == b"<?").count(), 3);
    // Mixed PHP and <% %> in one .html: refused with the reason, never guessed at.
    match server_page_for_html("<p>Hi <?= $n ?> and <%= x %></p>") {
        Err(Error::Parse { msg, .. }) => assert!(msg.contains("mixes PHP code and <% %> code"), "{msg}"),
        other => panic!("{other:?}"),
    }
    // An island the server-page rules cannot keep safe refuses the .html as well.
    refused("aspx", "<input <%= disabled %> value=\"x\">", "tag or attribute name");
}

#[test]
fn ingest_export_of_a_jsp() {
    // 12) a .jsp end to end: code kept, text translated, refusal stores nothing
    let src = fixture("suite.jsp");
    let t = texts_str("jsp", &src);
    assert!(!t.is_empty() && !t.iter().any(|x| x.contains("<%") || x.contains("String")), "{t:?}");
    let owned: Vec<String> = t.iter().map(|x| format!("FR {x}")).collect();
    let m: Vec<(usize, &str)> = owned.iter().enumerate().map(|(i, s)| (i, s.as_str())).collect();
    let out = bld("jsp", &src, &m);
    for code in [
        "<%@ page contentType=\"text/html; charset=UTF-8\" %>",
        "<%= date %>",
        "${user.name}",
        "<% for (int i = 0; i < 3; i++) { %>",
        "<jsp:scriptlet>String s = \"<p>no</p>\";</jsp:scriptlet>",
    ] {
        assert!(out.contains(code), "{code} in {out}");
    }
    assert!(out.contains("FR ") && !out.contains('\u{27E6}'), "{out}");
    match extract_str("phtml", "<div <?php echo $a ?>>x</div>") {
        Err(Error::Parse { msg, .. }) => assert!(msg.contains("attribute name"), "{msg}"),
        other => panic!("{other:?}"),
    }
}

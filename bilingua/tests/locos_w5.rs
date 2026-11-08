//! Wave 5 B-side (11-07): the HTML and XML family under the LocOS profile, each item with a small hand-written
//! file. The goldens in `tests/locos/{xml,xmb,xtb,ttml,html,eml,mdx,dita,docbook,hhc,hhk,hhp,xslt,php,jsp}` compare
//! whole builds with LocOS; these pin each behaviour (B-SIDE item in the test's comment).
use bilingua::profile::tokens;
use bilingua::*;

fn opts(target: Option<&str>) -> ExtractOptions {
    profile::locos().extract_options().with_granularity(Granularity::Paragraph).with_target_lang(target.map(str::to_string))
}

fn units(name: &str, data: &[u8]) -> Vec<(i64, String, String)> {
    let doc = extract(name, data, &opts(None)).unwrap_or_else(|e| panic!("{name}: {e}"));
    profile::locos_units(&doc).into_iter().map(|u| (u.p_index, u.text, u.style)).collect()
}

fn p_indexes(name: &str, data: &[u8]) -> Vec<i64> {
    units(name, data).into_iter().map(|u| u.0).collect()
}

/// Sets LocOS texts by p_index the way the adapter does (`locos_set_targets`) and merges with the profile.
fn build_with(name: &str, data: &[u8], o: &ExtractOptions, texts: &[(i64, &str)]) -> Result<(Vec<u8>, Vec<tokens::TagRepair>), Error> {
    let mut doc = extract(name, data, o)?;
    let mut repairs = Vec::new();
    for u in profile::locos_units(&doc) {
        if let Some((_, t)) = texts.iter().find(|(p, _)| *p == u.p_index) {
            let (inl, _, r) = tokens::repair_locos_text_for(&doc.format, t, &u.map);
            repairs.extend(r);
            profile::locos_set_target(&mut doc, &u.segment_id, inl)?;
        }
    }
    Ok((merge_with(&doc, &profile::locos().merge_options(&doc.format))?, repairs))
}

fn build(name: &str, data: &[u8], target: Option<&str>, texts: &[(i64, &str)]) -> Result<Vec<u8>, Error> {
    build_with(name, data, &opts(target), texts).map(|r| r.0)
}

fn s(b: Vec<u8>) -> String {
    String::from_utf8(b).unwrap()
}

const G1: &str = "\u{27E6}g1\u{27E7}";
const C1: &str = "\u{27E6}/g1\u{27E7}";
const G2: &str = "\u{27E6}g2\u{27E7}";
const C2: &str = "\u{27E6}/g2\u{27E7}";

// ---- ids that move (xml-1, xmb-2, tt-4) ------------------------------------------------------------------------

const ANDROID: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<resources>
    <string name="app" translatable="false">Harbor</string>
    <string name="hi">Hello</string>
    <string name="ref">@string/hi</string>
    <string name="blank"></string>
    <plurals name="n"><item quantity="one">%d file</item><item quantity="other">%d files</item></plurals>
    <string name="bye">Bye</string>
</resources>
"#;

/// xml-1: an Android string's p_index is its slot among every slot (skipped, reference and empty ones counted), as
/// LocOS's `XmlResourceFilter.parse` numbers it, so a skipped string never renumbers the ones after it.
#[test]
fn android_p_index_keeps_skipped_slots() {
    assert_eq!(p_indexes("strings.xml", ANDROID.as_bytes()), vec![1, 4, 5, 6]);
    let out = s(build("strings.xml", ANDROID.as_bytes(), None, &[(6, "Tschuess")]).unwrap());
    assert!(out.contains(r#"<string name="bye">Tschuess</string>"#) && out.contains(">Hello<"), "{out}");
    // Bilingua's own reading has no LocOS index
    let native = extract("strings.xml", ANDROID.as_bytes(), &ExtractOptions::new()).unwrap();
    assert!(native.units.iter().all(|u| !u.context.extra.contains_key(profile::P_INDEX)));
}

/// xml-1: Java XML properties too (an empty entry is a slot).
#[test]
fn java_xml_properties_p_index_keeps_empty_entries() {
    let x = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<properties>\n<entry key=\"a\">Shop</entry>\n<entry key=\"b\"></entry>\n<entry key=\"c\">Bye</entry>\n</properties>\n";
    assert_eq!(p_indexes("messages.xml", x.as_bytes()), vec![0, 2]);
}

const XMB: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<messagebundle>
  <msg id="1">Welcome</msg>
  <msg id="2"><ph name="INTERPOLATION"/></msg>
  <msg id="1">Welcome</msg>
  <msg id="3">Open</msg>
</messagebundle>
"#;

/// xmb-2: an XMB message keeps its position among every message (a placeholder-only one and a repeated id counted).
#[test]
fn xmb_p_index_keeps_skipped_messages() {
    assert_eq!(p_indexes("m.xmb", XMB.as_bytes()), vec![0, 3]);
    let xtb = "<?xml version=\"1.0\"?>\n<translationbundle>\n  <translation id=\"a\">A</translation>\n  <translation id=\"b\"/>\n  <translation id=\"c\">C</translation>\n</translationbundle>\n";
    assert_eq!(p_indexes("m.xtb", xtb.as_bytes()), vec![0, 2]);
}

/// xmb-1: under the profile an untouched XMB build is the XTB, as LocOS's writer always makes one; natively an
/// untouched merge is still the XMB itself.
#[test]
fn untouched_xmb_build_is_the_xtb() {
    let out = s(build("m.xmb", XMB.as_bytes(), None, &[]).unwrap());
    assert!(out.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE translationbundle>\n<translationbundle>\n"), "{out}");
    assert!(out.contains("<translation id=\"3\">Open</translation>"), "{out}");
    let doc = extract("m.xmb", XMB.as_bytes(), &ExtractOptions::new()).unwrap();
    assert_eq!(merge(&doc).unwrap(), XMB.as_bytes());
}

/// xmb-3: an XTB with a BOM, CRLF line ends, or both kinds of line end mixed, comes back unchanged untouched.
#[test]
fn xtb_variants_build_unchanged() {
    let base = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<!DOCTYPE translationbundle>\r\n<translationbundle lang='en'>\r\n  <translation id=\"a\">Hi</translation>\r\n</translationbundle>\r\n";
    for v in [base.to_string(), format!("\u{FEFF}{base}"), base.replace('\n', "\r\n"), base.replace("\r\n", "\n")] {
        assert_eq!(build("m.xtb", v.as_bytes(), None, &[]).unwrap(), v.as_bytes(), "{v:?}");
    }
}

const TTML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<tt xmlns="http://www.w3.org/ns/ttml" xml:lang="en"><body><div>
<p begin="1s" end="2s" style="s1">One</p>
<p begin="2s" end="3s"></p>
<p begin="3s" end="4s"/>
<p begin="4s" end="5s" style="s2">Four<br/>lines</p>
</div></body></tt>
"#;

/// tt-4, tt-1: a TTML caption keeps its position among every caption `<p>` (blank ones counted) and has no style
/// (LocOS `ttml_filter` sets none; Bilingua's own reading keeps the `style` attribute).
#[test]
fn ttml_p_index_and_no_style() {
    assert_eq!(units("a.ttml", TTML.as_bytes()), vec![(0, "One".into(), String::new()), (3, "Four\u{27E6}x1\u{27E7}lines".into(), String::new())]);
    let native = extract("a.ttml", TTML.as_bytes(), &ExtractOptions::new()).unwrap();
    assert_eq!(native.units[0].context.style.as_deref(), Some("s1"));
}

/// tt-2: a typed line break in a caption translation is written as `<br/>`.
#[test]
fn ttml_typed_newline_is_a_br() {
    let out = s(build("a.ttml", TTML.as_bytes(), None, &[(0, "Eins\nzwei")]).unwrap());
    assert!(out.contains(">Eins<br/>zwei</p>"), "{out}");
}

// ---- styles (em-1) ---------------------------------------------------------------------------------------------

/// em-1: the e-mail Subject and plain-text paragraphs have no style under the profile (LocOS `email_filter`).
#[test]
fn eml_units_have_no_style() {
    let m = "From: a@example.com\r\nSubject: Weekly news\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nHello team.\r\n";
    assert_eq!(units("m.eml", m.as_bytes()), vec![(0, "Weekly news".into(), String::new()), (1, "Hello team.".into(), String::new())]);
}

// ---- code pages (html-1, hh-1, hh-2) ---------------------------------------------------------------------------

/// html-1: a page in a declared legacy code page goes back in it, its meta charset kept, when every character fits;
/// a character that does not fit gives UTF-8 under a UTF-8 declaration (LocOS `encode_like`).
#[test]
fn html_keeps_its_legacy_code_page_when_the_text_fits() {
    let page = b"<html><head><meta charset=\"windows-1252\"></head><body><p>Caf\xe9</p><p>Tea</p></body></html>\n";
    let fits = build("p.html", page, None, &[(0, "Caf\u{e9} cr\u{e8}me")]).unwrap();
    assert_eq!(fits, b"<html><head><meta charset=\"windows-1252\"></head><body><p>Caf\xe9 cr\xe8me</p><p>Tea</p></body></html>\n");
    let wide = s(build("p.html", page, None, &[(0, "\u{41a}\u{430}\u{444}\u{435}")]).unwrap());
    assert_eq!(wide, "<html><head><meta charset=\"UTF-8\"></head><body><p>\u{41a}\u{430}\u{444}\u{435}</p><p>Tea</p></body></html>\n");
}

const HHC_1251: &[u8] = b"<HTML><BODY>\r\n<UL>\r\n<LI> <OBJECT type=\"text/sitemap\">\r\n<param name=\"Name\" value=\"\xca\xee\xf4\xe5\">\r\n<param name=\"Local\" value=\"a.htm\">\r\n</OBJECT>\r\n</UL>\r\n</BODY></HTML>\r\n";

/// hh-1: an undeclared contents file is read in the project's code page given as `htmlhelp.code_page` (LocOS
/// `hhp_code_page`); under the profile there is no other rule (Windows-1252, as LocOS, even for a Russian source).
#[test]
fn htmlhelp_code_page_option_decides_an_undeclared_file() {
    let o = opts(None).with_format_option("htmlhelp.code_page", "1251");
    let doc = extract("toc.hhc", HHC_1251, &o).unwrap();
    assert_eq!(profile::locos_units(&doc)[0].text, "\u{41a}\u{43e}\u{444}\u{435}");
    let ru = opts(None).with_source_lang("ru-RU");
    let doc = extract("toc.hhc", HHC_1251, &ru).unwrap();
    assert_eq!(profile::locos_units(&doc)[0].text, "\u{ca}\u{ee}\u{f4}\u{e5}");
    // Bilingua's own reading still falls back to the source language's code page
    let doc = extract("toc.hhc", HHC_1251, &ExtractOptions::new().with_source_lang("ru-RU")).unwrap();
    assert_eq!(plain_text(&doc.units[0].source()), "\u{41a}\u{43e}\u{444}\u{435}");
    assert!(extract("toc.hhc", HHC_1251, &opts(None).with_format_option("htmlhelp.code_page", "x")).is_err());
}

/// hh-1 (#797): a project file is read in the code page of its own `Language=`.
#[test]
fn hhp_is_read_in_its_own_language_code_page() {
    let hhp = b"[OPTIONS]\r\nLanguage=0x419 Russian\r\nTitle=\xd1\xef\xf0\xe0\xe2\xea\xe0\r\n";
    assert_eq!(units("p.hhp", hhp)[0].1, "\u{421}\u{43f}\u{440}\u{430}\u{432}\u{43a}\u{430}");
}

/// hh-2: a translated HTML Help file stays in its ANSI code page (a localized one in the target's), and a
/// character that code page cannot hold is refused, as HTML Help Workshop needs ANSI files.
#[test]
fn htmlhelp_keeps_the_ansi_code_page_and_refuses_what_does_not_fit() {
    let hhc = b"<HTML><HEAD><meta charset=\"windows-1252\"></HEAD><BODY><UL><LI> <OBJECT type=\"text/sitemap\"><param name=\"Name\" value=\"Caf\xe9\"></OBJECT></UL></BODY></HTML>";
    let out = build("toc.hhc", hhc, None, &[(0, "Caf\u{e9} cr\u{e8}me")]).unwrap();
    assert!(out.windows(10).any(|w| w == b"Caf\xe9 cr\xe8me"), "{}", String::from_utf8_lossy(&out));
    assert!(String::from_utf8_lossy(&out).contains("charset=\"windows-1252\""));
    let err = build("toc.hhc", hhc, None, &[(0, "\u{41a}\u{430}\u{444}\u{435}")]).unwrap_err();
    assert!(err.to_string().contains("code page"), "{err}");
    // a Russian build is written in Windows-1251
    let ru = build("toc.hhc", hhc, Some("ru-RU"), &[(0, "\u{41a}\u{430}\u{444}\u{435}")]).unwrap();
    assert!(ru.windows(4).any(|w| w == b"\xca\xe0\xf4\xe5"), "{}", String::from_utf8_lossy(&ru));
}

// ---- repeated tags (xml-2, mdx-1) ------------------------------------------------------------------------------

/// xml-2: DITA, DocBook and generic XML repair a translation's tags as LocOS's ITS writer does (a repeated pair loses
/// its tags and keeps its text, a stray close is dropped, an open pair closed at the end), and report each repair.
#[test]
fn xml_formats_repair_repeated_tags() {
    let dita = "<?xml version=\"1.0\"?>\n<topic id=\"t\"><title>T</title><body><p>Press <b>Enter</b> now.</p></body></topic>\n";
    let (out, rep) = build_with("t.dita", dita.as_bytes(), &opts(None), &[(1, &format!("Drueck {G1}Enter{C1} {G1}jetzt{C1}."))]).unwrap();
    assert!(s(out).contains("<p>Drueck <b>Enter</b> jetzt.</p>"));
    assert!(matches!(rep.as_slice(), [tokens::TagRepair::Repeated { .. }]), "{rep:?}");
    let (out, _) = build_with("t.dita", dita.as_bytes(), &opts(None), &[(1, &format!("Drueck {C1}{G1}Enter."))]).unwrap();
    assert!(s(out).contains("<p>Drueck <b>Enter.</b></p>"));
    let dbk = "<?xml version=\"1.0\"?>\n<article xmlns=\"http://docbook.org/ns/docbook\" version=\"5.0\"><para>Hit <emphasis>Go</emphasis>.</para></article>\n";
    let out = s(build("a.dbk", dbk.as_bytes(), None, &[(0, &format!("{G1}Los{C1} {G1}los{C1}"))]).unwrap());
    assert!(out.contains("<para><emphasis>Los</emphasis> los</para>"), "{out}");
    // a tag the source lacks is still refused
    assert!(build("a.dbk", dbk.as_bytes(), None, &[(0, &format!("{G2}x{C2}"))]).is_err() || !s(build("a.dbk", dbk.as_bytes(), None, &[(0, &format!("{G2}x{C2}"))]).unwrap()).contains("<emphasis>x"));
    // without the profile a repeat is still refused
    let mut d = extract("a.dbk", dbk.as_bytes(), &ExtractOptions::new()).unwrap();
    let seg = d.units[0].segments[0].clone();
    let mut twice = seg.source.clone();
    twice.extend(seg.source.iter().cloned());
    assert!(d.set_target(&seg.id, twice).is_err());
}

/// mdx-1: an MDX translation that repeats a pair token is written, each token as its JSX (LocOS `_rematerialize`).
#[test]
fn mdx_writes_a_repeated_pair() {
    let mdx = "# Hi\n\n<Note>Keep safe.</Note>\n";
    let out = s(build("a.mdx", mdx.as_bytes(), None, &[(1, &format!("{G1}Halt{C1} {G1}sicher{C1}"))]).unwrap());
    assert!(out.contains("<Note>Halt</Note> <Note>sicher</Note>"), "{out}");
}

// ---- alt-text prompts (html-3, mdx-2) --------------------------------------------------------------------------

fn inventory(name: &str, data: &[u8]) -> Vec<(i64, String, Option<String>)> {
    let doc = extract(name, data, &opts(None)).unwrap();
    let mut out = Vec::new();
    for n in profile::locos_units(&doc) {
        let u = doc.units.iter().find(|u| u.segments.iter().any(|s| s.id == n.segment_id)).unwrap();
        if let Some(c) = u.context.extra.get(profile::A11Y_IMAGES) {
            out.push((n.p_index, c.clone(), u.context.extra.get(profile::A11Y_MISSING_ALT).cloned()));
        }
    }
    out
}

/// html-3: LocOS's image inventory (count and findings, both kinds) on the unit each image belongs to.
#[test]
fn html_image_inventory_for_missing_alt_prompts() {
    let page = "<html><body><p>One <img src=\"i/a.png\"> <img src=\"b.png\" alt=\"\"></p><img src=\"c.png\" alt=\"C\"><p>Two</p></body></html>";
    assert_eq!(
        inventory("p.html", page.as_bytes()),
        vec![(0, "3".into(), Some(r#"[{"kind":"missing","where":"a.png"},{"kind":"empty","where":"b.png"}]"#.into()))]
    );
    // an unquoted alt is text, a bare alt is empty; an unquoted src names no file (LocOS reads quoted values only)
    let page = "<p>X <img src=a.png alt=Cat> <img src=b.png alt></p>";
    assert_eq!(inventory("p.html", page.as_bytes()), vec![(0, "2".into(), Some(r#"[{"kind":"empty","where":""}]"#.into()))]);
    let native = extract("p.html", page.as_bytes(), &ExtractOptions::new()).unwrap();
    assert!(native.units.iter().all(|u| !u.context.extra.contains_key(profile::A11Y_IMAGES)));
}

/// mdx-2: MDX images (Markdown and JSX) as the same inventory, not as warnings, under the profile.
#[test]
fn mdx_image_inventory_for_missing_alt_prompts() {
    let mdx = "# Pics\n\n![](img/a.png) and ![B](b.png)\n\n<img src=\"c.png\" />\n";
    let doc = extract("a.mdx", mdx.as_bytes(), &opts(None)).unwrap();
    assert!(doc.warnings.iter().all(|w| w.kind != "missing_alt"), "{:?}", doc.warnings);
    assert_eq!(
        inventory("a.mdx", mdx.as_bytes()),
        vec![(1, "3".into(), Some(r#"[{"kind":"empty","where":"a.png"},{"kind":"missing","where":"c.png"}]"#.into()))]
    );
    let native = extract("a.mdx", mdx.as_bytes(), &ExtractOptions::new()).unwrap();
    assert!(native.warnings.iter().any(|w| w.kind == "missing_alt"));
}

// ---- the rest ----------------------------------------------------------------------------------------------------

/// mdx-3: a front matter value with a tab is quoted (PyYAML reads no plain scalar with a tab).
#[test]
fn mdx_front_matter_value_with_a_tab_is_quoted() {
    let mdx = "---\ntitle: Start\n---\n\n# Hi\n";
    let out = s(build("a.mdx", mdx.as_bytes(), None, &[(0, "tab\there")]).unwrap());
    assert!(out.starts_with("---\ntitle: 'tab\there'\n---"), "{out}");
}

/// php-2: a PHP array in a legacy code page with nothing rewritten comes back as it was (LocOS returns the original).
#[test]
fn php_untouched_build_keeps_its_bytes() {
    let php = b"<?php\nreturn [\n    'a' => 'Caf\xe9',\n];\n";
    assert_eq!(build("lang.php", php, Some("de-DE"), &[]).unwrap(), php);
    assert_eq!(build("lang.php", php, None, &[(0, "Kaffee")]).unwrap(), b"<?php\nreturn [\n    'a' => 'Kaffee',\n];\n");
}

/// xs-2: a UTF-16 LE stylesheet stays UTF-16 LE with its BOM and declaration.
#[test]
fn xslt_keeps_utf16() {
    let x = "<?xml version=\"1.0\" encoding=\"UTF-16\"?>\n<xsl:stylesheet version=\"1.0\" xmlns:xsl=\"http://www.w3.org/1999/XSL/Transform\"><xsl:template match=\"/\"><h1>Report</h1></xsl:template></xsl:stylesheet>\n";
    let mut data = vec![0xFF, 0xFE];
    for u in x.encode_utf16() {
        data.extend_from_slice(&u.to_le_bytes());
    }
    let out = build("a.xsl", &data, None, &[(0, "Bericht")]).unwrap();
    assert!(out.starts_with(&[0xFF, 0xFE]), "{out:?}");
    let back: Vec<u16> = out[2..].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    let back = String::from_utf16(&back).unwrap();
    assert!(back.contains("encoding=\"UTF-16\"") && back.contains("<h1>Bericht</h1>"), "{back}");
}

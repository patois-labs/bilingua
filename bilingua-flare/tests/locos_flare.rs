//! The cases of the LocOS suite `tests/verify_flare_filter.py`, one Rust test each (the suite has 9 test functions;
//! the web/CLI ones keep the part that is about the file, the documentation-surface one has no counterpart here).
//! Unit texts are compared in LocOS form (inline codes as `⟦g1⟧...⟦/g1⟧` tokens).
mod common;
use bilingua_flare::Flare;
use bilingua_model::*;
use common::*;

const X1: &str = "\u{27E6}x1\u{27E7}";

#[test]
fn registration() {
    // LocOS also checks lazy loading and its registry: here, the filter's own identity and sniff
    assert_eq!(Flare.id(), "flprjzip");
    assert_eq!(Flare.extensions(), &["flprjzip"], "a plain .zip is not registered");
    assert!(Flare.sniff(&project()));
    assert!(!Flare.sniff(b"PK\x03\x04 not a project"));
    assert!(!Flare.sniff(&make_zip(&[("readme.txt".into(), b"hello".to_vec())])));
    assert!(!Flare.sniff(b"hello"));
    assert!(!Flare.bilingual());
}

#[test]
fn extraction() {
    let d = extract(&project());
    let texts = loc_texts(&d);
    let joined = texts.join("\n");
    // Variables are point tokens, never text: the name and the cached content are never offered.
    assert!(!joined.contains("General.ProductName"), "{texts:?}");
    assert!(!joined.contains("Acme Writer before") && joined.contains(&format!("Install {X1} before you print")), "{texts:?}");
    assert!(texts.contains(&format!("Print with {X1}")), "{texts:?}");
    // Inline MadCap elements stay inside their sentence.
    assert!(texts.iter().any(|t| t.contains("spooler") && t.contains("Other topic") && t.contains("notes and see")), "{texts:?}");
    assert!(texts.iter().any(|t| t.contains("online readers only") && t.contains("and everyone.")), "{texts:?}");
    assert!(texts.iter().any(|t| t.starts_with("Brand line: ") && t.contains("ends here.")), "{texts:?}");
    // Drop-down hotspot and body are their own units.
    assert!(texts.contains(&"More details".to_string()) && texts.contains(&"Hidden body text.".to_string()), "{texts:?}");
    // Head title, nested snippets, image alt, RTL and CJK.
    for want in ["Printing guide", "Outer snippet text.", "Acme makes writing tools", "Company logo", JA] {
        assert!(joined.contains(want), "{want}: {texts:?}");
    }
    assert!(texts.iter().any(|t| t.contains("print words") && t.starts_with("Inner snippet with")), "{texts:?}");
    assert!(texts.iter().any(|t| t.starts_with(AR)), "{texts:?}");
    let of_style = |s: &str| -> Vec<String> { d.units.iter().filter(|u| u.context.style.as_deref() == Some(s)).map(loc_text).collect() };
    // Index keywords: one unit per keyword / level, escaped colon unescaped.
    assert_eq!(of_style("flare:index-keyword"), ["Printing", "Printing", "Color", ":Colon start"]);
    // Variable set: text definitions only (not DateTime, not numeric-only).
    assert_eq!(of_style("flare:variable"), ["Acme Writer", "Write & ship"]);
    assert_eq!(of_style("flare:toc-title"), ["Getting started".to_string(), format!("Guide to {X1} printing")]);
    assert_eq!(of_style("flare:glossary-term"), ["spooler"]);
    assert_eq!(of_style("flare:glossary-definition"), ["A queue that holds print jobs."]);
    // Nothing from the condition tag set, the project file or the style sheet.
    for never in ["PrintOnly", "Default.Online", "CatapultProject", "color"] {
        assert!(!joined.contains(never), "{never}: {texts:?}");
    }
    // The member each unit came from is in its id: Topic.htm is member 1, the TOC member 9.
    assert!(d.units.iter().any(|u| u.id.starts_with("m1-")) && d.units.iter().any(|u| u.id.starts_with("m9-")));
    // Variables, snippets and keywords are real MadCap codes, not HTML ones.
    let v = d.units.iter().find(|u| loc_text(u).starts_with("Install")).unwrap();
    assert_eq!(v.codes.values().next().unwrap().label.as_deref(), Some("variable"));
    assert!(v.source().iter().any(|i| matches!(i, Inline::Tag { display, .. } if display == "<MadCap:variable/>")));
    assert_eq!(d.format, "flprjzip");
    assert_eq!(d.source_lang.as_deref(), Some("en-us"));
    let ids: std::collections::HashSet<&str> = d.units.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(ids.len(), d.units.len());
    assert_eq!(extract(&project()).units.iter().map(|u| u.id.clone()).collect::<Vec<_>>(), d.units.iter().map(|u| u.id.clone()).collect::<Vec<_>>());
}

#[test]
fn byte_identity() {
    let src = project();
    let mut d = extract(&src);
    assert_eq!(merge(&d), src, "no change gives the original bytes");
    // re-submitting the source text changes nothing
    for i in 0..d.units.len() {
        let t = loc_text(&d.units[i]);
        set(&mut d, i, &t);
    }
    assert_eq!(merge(&d), src, "re-submitting the source text must change nothing");
}

#[test]
fn translated_build() {
    let src = project();
    let mut d = extract(&src);
    for i in 0..d.units.len() {
        let t = loc_text(&d.units[i]);
        let new = if t == JA {
            "\u{4e2d}\u{6587}\u{7ffb}\u{8bd1}".to_string()
        } else if t.starts_with(AR) {
            t.replace(AR, "\u{062a}\u{0631}\u{062c}\u{0645}\u{0629}")
        } else {
            format!("T({t})")
        };
        set(&mut d, i, &new);
    }
    let out = merge(&d);
    let (before, binfo) = members(&src);
    let (after, ainfo) = members(&out);
    assert_eq!(before.iter().map(|m| &m.0).collect::<Vec<_>>(), after.iter().map(|m| &m.0).collect::<Vec<_>>(), "member names/order changed");
    assert_eq!(binfo.iter().map(|i| i.stored).collect::<Vec<_>>(), ainfo.iter().map(|i| i.stored).collect::<Vec<_>>(), "compression changed");
    let stamp = zip::DateTime::from_date_and_time(2026, 10, 1, 9, 30, 0).unwrap();
    assert!(ainfo.iter().all(|i| i.modified == Some(stamp)));
    assert_eq!(bilingua_comment(&out), b"flare fixture");
    for untouched in [
        "Project/ConditionTagSets/Default.flcts",
        "Help.flprj",
        "Content/Resources/Images/logo.png",
        "Content/Resources/Stylesheets/Styles.css",
        "Content/",
    ] {
        assert_eq!(get(&after, untouched), get(&before, untouched), "{untouched}");
    }
    let topic = text_of(&after, "Content/Topic.htm");
    for keep in [
        r#"<MadCap:variable name="General.ProductName" />"#,
        r#"<MadCap:variable name="General.ProductName">Acme Writer</MadCap:variable>"#,
        r#"MadCap:conditions="Default.PrintOnly""#,
        r#"<MadCap:conditionalText MadCap:conditions="Default.Online">online readers only</MadCap:conditionalText>"#,
        r#"<MadCap:glossaryTerm glossTerm="Glossary.Term0">"#,
        r#"<MadCap:xref href="Other.htm">"#,
        r#"<MadCap:snippetText src="../Resources/Snippets/Brand.flsnp" />"#,
        r#"<MadCap:snippetBlock src="../Resources/Snippets/Outer.flsnp" />"#,
        "<MadCap:dropDownHotspot>T(More details)</MadCap:dropDownHotspot>",
        r#"alt="T(Company logo)""#,
        "<title>T(Printing guide)</title>",
        r#"term="T(Printing);T(Printing):T(Color);T(\:Colon start)""#,
    ] {
        assert!(topic.contains(keep), "{keep}\n{topic}");
    }
    assert!(!topic.contains('\u{27E6}'));
    assert!(get(&after, "Content/Intl.htm").starts_with(&[0xEF, 0xBB, 0xBF]), "BOM lost");
    let intl = text_of(&after, "Content/Intl.htm");
    assert!(intl.contains("\u{4e2d}\u{6587}\u{7ffb}\u{8bd1}") && intl.contains("\u{062a}\u{0631}\u{062c}\u{0645}\u{0629}"));
    assert!(intl.contains("\r\n") && intl.contains(r#"<MadCap:variable name="General.ProductName" />"#));
    let inner = text_of(&after, "Content/Resources/Snippets/Inner.flsnp");
    assert!(inner.contains(r#"MadCap:conditions="Default.Print">"#) && inner.contains("T(Inner snippet with"), "{inner}");
    let outer = text_of(&after, "Content/Resources/Snippets/Outer.flsnp");
    assert!(outer.contains(r#"<MadCap:snippetBlock src="Inner.flsnp" />"#), "{outer}");
    let flvar = text_of(&after, "Project/VariableSets/General.flvar");
    assert!(flvar.contains(r#"EvaluatedDefinition="T(Acme Writer)">T(Acme Writer)</Variable>"#), "{flvar}");
    assert!(flvar.contains(r#"EvaluatedDefinition="T(Write &amp; ship)">T(Write &amp; ship)</Variable>"#), "{flvar}");
    assert!(flvar.contains(">2026</Variable>") && flvar.contains(">October 10, 2026</Variable>") && flvar.contains(r#"Comment="Do not abbreviate""#));
    let toc = text_of(&after, "Project/TOCs/Master.fltoc");
    assert!(toc.contains(r#"Title="[%=System.LinkedTitle%]""#), "{toc}");
    assert!(toc.contains(r#"Title="T(Guide to [%=General.ProductName%] printing)""#), "{toc}");
    assert!(toc.contains(r#"Title="T(Getting started)""#));
    let glo = text_of(&after, "Project/Glossaries/Glossary.flglo");
    assert!(glo.contains("<Term>T(spooler)</Term>") && glo.contains("T(A queue that holds print jobs.)"));
    well_formed(&after);
    // The translated project reads again with the same units.
    assert_eq!(extract(&out).units.len(), d.units.len());
    // A target language is declared (right to left for Arabic) when something is translated.
    d.target_lang = Some("ar-SA".into());
    let (loc, _) = members(&merge(&d));
    let t = text_of(&loc, "Content/Topic.htm");
    assert!(t.contains(r#"lang="ar-SA""#) && t.contains(r#"xml:lang="ar-SA""#) && t.contains(r#"dir="rtl""#), "{t}");
}

#[test]
fn xhtml_safe_targets() {
    // A reference-looking text, an unknown entity or a control character still gives well-formed XHTML. The target
    // is markup text, as in LocOS (`_xml_named_refs`): `&nbsp;` is a no-break space, written `&#160;`.
    let src = project();
    let mut d = extract(&src);
    let i = index_of(&d, "More details");
    set(&mut d, i, "Plus&nbsp;de d&eacute;tails &bogus; A&amp;B\u{1}");
    let j = index_of(&d, "Company logo");
    set(&mut d, j, "Logo\u{2} de l'entreprise");
    let (m, _) = members(&merge(&d));
    well_formed(&m);
    let text = text_of(&m, "Content/Topic.htm");
    assert!(text.contains("Plus&#160;de d&#233;tails &amp;bogus; A&amp;B</MadCap:dropDownHotspot>"), "{text}");
    assert!(!text.contains('\u{1}') && !text.contains('\u{2}'));
    assert!(text.contains("alt=\"Logo de l&#x27;entreprise\""), "{text}");
    // markup typed into a target is text, never an element
    let k = index_of(&d, "Hidden body text.");
    set(&mut d, k, "<script>alert(1)</script> <MadCap:variable name=\"x\"/>");
    let (m, _) = members(&merge(&d));
    well_formed(&m);
    let text = text_of(&m, "Content/Topic.htm");
    assert!(text.contains("&lt;script&gt;alert(1)&lt;/script&gt; &lt;MadCap:variable name=\"x\"/&gt;"), "{text}");
}

fn refused(bytes: &[u8], needle: &str) {
    let e = Flare.extract(bytes, &ExtractOptions::default()).expect_err(needle);
    assert!(e.to_string().to_lowercase().contains(&needle.to_lowercase()), "{needle}: {e}");
}

fn with(extra: &[(&str, &[u8])]) -> Vec<u8> {
    project_with(extra, &[])
}

#[test]
fn hostile_zips() {
    refused(b"not a zip at all", "zip");
    refused(&make_zip(&[("readme.txt".into(), b"hello".to_vec()), ("a.htm".into(), b"<p>Hi</p>".to_vec())]), "not a MadCap Flare project");
    for bad in ["../evil.htm", "/abs/evil.htm", "C:/evil.htm"] {
        let e = Flare.extract(&with(&[(bad, b"<p>x</p>")]), &ExtractOptions::default()).expect_err(bad);
        assert!(matches!(e, Error::Parse { .. } | Error::Limit(_)), "{bad}: {e}");
    }
    let e = Flare.extract(&with(&[("content/topic.htm", b"<p>x</p>")]), &ExtractOptions::default()).expect_err("dup");
    assert!(e.to_string().contains("twice") || e.to_string().contains("duplicate"), "{e}");
    // a symbolic link
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    use std::io::Write;
    for (n, d) in base_members() {
        if n.ends_with('/') {
            continue;
        }
        w.start_file(n.as_str(), zip::write::SimpleFileOptions::default()).unwrap();
        w.write_all(&d).unwrap();
    }
    w.add_symlink("Content/link.htm", "/etc/passwd", zip::write::SimpleFileOptions::default()).unwrap();
    let e = Flare.extract(&w.finish().unwrap().into_inner(), &ExtractOptions::default()).expect_err("symlink");
    assert!(matches!(e, Error::Parse { .. } | Error::Limit(_)), "{e}");
    // a billion-laughs entity declaration in a variable set
    let lol = "<?xml version=\"1.0\"?>\n<!DOCTYPE v [<!ENTITY a \"aaaa\"><!ENTITY b \"&a;&a;&a;&a;\">]>\n<CatapultVariableSet><Variable Name=\"x\">&b;</Variable></CatapultVariableSet>\n";
    let e = Flare.extract(&with(&[("Project/VariableSets/General.flvar", lol.as_bytes())]), &ExtractOptions::default()).expect_err("entities");
    assert!(matches!(&e, Error::Limit(l) if l.kind == LimitKind::XmlEntities), "{e}");
    refused(&with(&[("Project/TOCs/Master.fltoc", b"<CatapultToc><TocEntry Title='x'></CatapultToc>")]), "not well-formed");
    let e = Flare.extract(&with(&[("Content/Topic.htm", b"<p>caf\xe9</p>")]), &ExtractOptions::default()).expect_err("latin1");
    assert!(matches!(e, Error::Encoding(_)), "{e}");
    // a zip bomb: one member inflating far past the ratio limit
    let big = vec![0u8; 4 * 1024 * 1024];
    let e = Flare.extract(&with(&[("Content/Resources/big.bin", &big)]), &ExtractOptions::default()).expect_err("bomb");
    assert!(matches!(&e, Error::Limit(l) if l.kind == LimitKind::ZipRatio), "{e}");
    // too many members, too many segments
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_zip_entries(5));
    let e = Flare.extract(&project(), &o).expect_err("members");
    assert!(matches!(&e, Error::Limit(l) if l.kind == LimitKind::ZipEntries), "{e}");
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_units(5));
    let e = Flare.extract(&project(), &o).expect_err("segments");
    assert!(matches!(&e, Error::Limit(l) if l.kind == LimitKind::Units), "{e}");
    // __MACOSX resource forks are copied, never decoded
    let mac = with(&[("__MACOSX/Content/._Topic.htm", b"\x00\x05\x16\x07\xff\xfe")]);
    assert_eq!(extract(&mac).units.len(), extract(&project()).units.len());
}

#[test]
fn export_with_a_target_language() {
    // The part of LocOS's web test that is about the file: translate every unit, export for de-DE.
    let src = project();
    let mut d = extract(&src);
    d.target_lang = Some("de-DE".into());
    for i in 0..d.units.len() {
        let t = loc_text(&d.units[i]);
        set(&mut d, i, &format!("[de] {t}"));
    }
    let out = merge(&d);
    let (m, _) = members(&out);
    assert_eq!(m.iter().map(|x| x.0.clone()).collect::<Vec<_>>(), members(&src).0.iter().map(|x| x.0.clone()).collect::<Vec<_>>());
    let topic = text_of(&m, "Content/Topic.htm");
    assert!(topic.contains("[de] Printing guide") && topic.contains(r#"lang="de-DE""#), "{topic}");
    assert!(topic.contains(r#"<MadCap:variable name="General.ProductName" />"#));
    assert_eq!(get(&m, "Project/ConditionTagSets/Default.flcts"), FLCTS.as_bytes());
    assert!(text_of(&m, "Project/VariableSets/General.flvar").contains("[de] Acme Writer"));
    // with nothing translated the target language is not stamped (lead decision), unless asked
    let mut d = extract(&src);
    d.target_lang = Some("de-DE".into());
    assert_eq!(merge(&d), src);
    let out = Flare.merge_with(&d, &MergeOptions::new().with_stamp_target_always(true)).unwrap();
    assert!(text_of(&members(&out).0, "Content/Topic.htm").contains(r#"lang="de-DE""#));
}

#[test]
fn a_zip_that_is_not_a_flare_project_is_refused_plainly() {
    let e = extract_err(&make_zip(&[("readme.txt".into(), b"hello".to_vec())]));
    assert!(e.to_string().to_lowercase().contains("not a madcap flare project"), "{e}");
    // a topic with the MadCap namespace alone makes a project; a .flsnp alone too
    let ok = make_zip(&[("a.htm".into(), TOPIC.as_bytes().to_vec())]);
    assert!(Flare.extract(&ok, &ExtractOptions::default()).is_ok());
    let ok = make_zip(&[("s.flsnp".into(), BRAND.as_bytes().to_vec())]);
    assert!(Flare.extract(&ok, &ExtractOptions::default()).is_ok());
}

/// The archive comment of a zip, read from its end record.
fn bilingua_comment(z: &[u8]) -> Vec<u8> {
    let i = (0..=z.len() - 22).rev().find(|&i| z[i..].starts_with(&[0x50, 0x4b, 0x05, 0x06])).unwrap();
    let n = u16::from_le_bytes([z[i + 20], z[i + 21]]) as usize;
    z[i + 22..i + 22 + n].to_vec()
}

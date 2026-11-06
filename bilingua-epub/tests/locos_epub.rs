//! The cases of the LocOS suite `tests/verify_epub_filter.py`, one Rust test each (the suite has 13 test functions).
//! The unit texts are compared in LocOS form (inline codes as `⟦g1⟧...⟦/g1⟧` tokens).
mod common;
use bilingua_epub::Epub;
use bilingua_model::*;
use common::*;

#[test]
fn registration() {
    // LocOS also checks its web surfaces, MIME table and docs; here: the filter's own identity and sniff.
    assert_eq!(Epub.id(), "epub");
    assert_eq!(Epub.extensions(), &["epub"]);
    assert!(Epub.sniff(&epub()));
    assert!(!Epub.sniff(b"PK\x03\x04 not an epub"));
    assert!(!Epub.sniff(b"hello"));
    assert!(!Epub.bilingual());
}

#[test]
fn unit_order() {
    let d = extract(&epub());
    let texts = loc_texts(&d);
    assert_eq!(
        texts,
        vec![
            "The Little Guide",
            "A short book about & for testing.",
            // spine order: ch1 then chapter 2 (manifest order is the reverse)
            "Chapter One",
            "Chapter One",
            "Hello \u{27E6}g1\u{27E7}bold\u{27E6}/g1\u{27E7} world, you have {count} items and %1$s left.",
            "A red cover",
            "Chapter Two",
            "Second chapter text.",
            // nav (not in the spine)
            "Contents",
            "\u{27E6}g1\u{27E7}Start here\u{27E6}/g1\u{27E7}",
            "\u{27E6}g1\u{27E7}Next steps\u{27E6}/g1\u{27E7}",
            // NCX
            "The Little Guide",
            "Start here",
            "Next steps",
        ]
    );
    let styles: Vec<String> = d.units.iter().map(|u| u.context.style.clone().unwrap_or_default()).collect();
    assert_eq!(styles[0], "epub:title");
    assert_eq!(styles[1], "epub:description");
    assert_eq!(d.units[5].context.extra.get("locos-style").map(String::as_str), Some("a11y:img-alt"));
    assert_eq!(styles[13], "epub:toc-label");
    // ids are unique and stable
    let ids: std::collections::HashSet<&str> = d.units.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(ids.len(), d.units.len());
    assert_eq!(extract(&epub()).units.iter().map(|u| u.id.clone()).collect::<Vec<_>>(), d.units.iter().map(|u| u.id.clone()).collect::<Vec<_>>());
    assert_eq!(d.format, "epub");
    assert_eq!(d.source_lang.as_deref(), Some("en"));
}

#[test]
fn identity_round_trip() {
    let src = epub();
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
fn translated_round_trip() {
    let src = epub();
    let mut d = extract(&src);
    translate_all(&mut d);
    set(&mut d, 4, "Bonjour \u{27E6}g1\u{27E7}gras\u{27E6}/g1\u{27E7} monde, vous avez {count} articles et %1$s restants.");
    let out = merge(&d);
    let (m, infos) = members(&out);
    assert_eq!(infos[0].name, "mimetype");
    assert!(infos[0].stored);
    assert_eq!(infos[0].extra, 0);
    assert_eq!(m["mimetype"], b"application/epub+zip");
    let (orig, orig_infos) = members(&src);
    assert_eq!(infos.iter().map(|i| &i.name).collect::<Vec<_>>(), orig_infos.iter().map(|i| &i.name).collect::<Vec<_>>());
    for name in ["OEBPS/style.css", "OEBPS/fonts/f.otf", "OEBPS/img/cover.png", "META-INF/container.xml"] {
        assert_eq!(m[name], orig[name], "{name}");
    }
    let ch1 = member_text(&m, "OEBPS/text/ch1.xhtml");
    assert!(ch1.contains("<p>Bonjour <b>gras</b> monde, vous avez {count} articles et %1$s restants.</p>"), "{ch1}");
    assert!(ch1.contains("alt=\"FR A red cover\"") && ch1.contains("<!DOCTYPE html>"));
    assert!(ch1.contains("<title>FR Chapter One</title>"));
    let opf = member_text(&m, "OEBPS/content.opf");
    assert!(opf.contains("<dc:title id=\"title\">FR The Little Guide</dc:title>"), "{opf}");
    assert!(opf.contains("FR A short book about &amp; for testing."));
    assert!(opf.contains("<dc:creator>Ada Writer</dc:creator>"));
    assert!(opf.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    let ncx = member_text(&m, "OEBPS/toc.ncx");
    assert!(ncx.contains("<text>FR Start here</text>") && ncx.contains("<!DOCTYPE ncx"));
    assert!(member_text(&m, "OEBPS/nav.xhtml").contains("FR <a href=\"text/ch1.xhtml\">Start here</a>"));
    well_formed(&m);
    // the output reads again with the same units and tags
    let again = extract(&out);
    assert_eq!(again.units.len(), d.units.len());
    assert_eq!(loc_text(&again.units[4]), "Bonjour \u{27E6}g1\u{27E7}gras\u{27E6}/g1\u{27E7} monde, vous avez {count} articles et %1$s restants.");
    // a tag the source does not have is refused by the model; literal bracket characters in a target are text
    let mut d2 = extract(&src);
    let id = d2.units[0].segments[0].id.clone();
    assert!(matches!(d2.set_target(&id, vec![Inline::tag("9", TagKind::Open, "<x>")]), Err(Error::InventedTag(_))));
    d2.set_target(&id, vec![Inline::text("Titre \u{27E6}g9\u{27E7}X")]).unwrap();
    let (m2, _) = members(&merge(&d2));
    assert!(member_text(&m2, "OEBPS/content.opf").contains("Titre \u{27E6}g9\u{27E7}X"));
}

#[test]
fn build_localized_stamps_the_language() {
    let src = epub();
    let mut d = extract(&src);
    d.target_lang = Some("fr-FR".into());
    set(&mut d, 2, "Chapitre un");
    let (m, _) = members(&merge(&d));
    let opf = member_text(&m, "OEBPS/content.opf");
    assert!(opf.contains("<dc:language>fr-FR</dc:language>") && opf.contains("xml:lang=\"fr-FR\""), "{opf}");
    assert!(!opf.contains("page-progression-direction"));
    let ch1 = member_text(&m, "OEBPS/text/ch1.xhtml");
    assert!(ch1.contains("lang=\"fr-FR\"") && ch1.contains("xml:lang=\"fr-FR\""), "{}", &ch1[..300]);
    assert!(ch1.contains("<title>Chapitre un</title>"));
    let ch2 = member_text(&m, "OEBPS/text/chapter 2.xhtml");
    assert!(ch2.contains("lang=\"fr-FR\"") && ch2.contains("xml:lang=\"fr-FR\""), "{ch2}");
    assert!(member_text(&m, "OEBPS/toc.ncx").contains("xml:lang=\"fr-FR\""));
    assert!(member_text(&m, "OEBPS/nav.xhtml").contains("xml:lang=\"fr-FR\""));
    well_formed(&m);

    // right to left
    let mut d = extract(&src);
    d.target_lang = Some("ar".into());
    set(&mut d, 2, "Chapter Ar");
    let (m, _) = members(&merge(&d));
    assert!(member_text(&m, "OEBPS/content.opf").contains("page-progression-direction=\"rtl\""));
    assert!(member_text(&m, "OEBPS/text/ch1.xhtml").contains("dir=\"rtl\""));
    well_formed(&m);

    // an unusable locale declares nothing; the text still changes
    let mut d = extract(&src);
    d.target_lang = Some("-".into());
    set(&mut d, 0, "Titre");
    let (m, _) = members(&merge(&d));
    let opf = member_text(&m, "OEBPS/content.opf");
    assert!(opf.contains("<dc:language>en</dc:language>") && opf.contains(">Titre<"));
    // nothing translated: a target language alone leaves the file as it was (lead decision 11-04)
    let mut d = extract(&src);
    d.target_lang = Some("ar".into());
    assert_eq!(merge(&d), src);
}

#[test]
fn utf16_chapter_bom_opf_and_epub2() {
    let ch2_16: Vec<u8> = {
        let t = CH2.replace("encoding=\"UTF-8\"", "encoding=\"UTF-16\"");
        let mut v = vec![0xFF, 0xFE];
        for u in t.encode_utf16() {
            v.extend_from_slice(&u.to_le_bytes());
        }
        v
    };
    let mut opf = vec![0xEF, 0xBB, 0xBF];
    opf.extend_from_slice(OPF.replace("version=\"3.0\"", "version=\"2.0\"").replace(" properties=\"nav\"", "").as_bytes());
    let src = epub_with(&[("OEBPS/text/chapter 2.xhtml", &ch2_16), ("OEBPS/content.opf", &opf)]);
    let mut d = extract(&src);
    let texts = loc_texts(&d);
    assert!(texts.contains(&"Second chapter text.".to_string()));
    // without the nav property the nav.xhtml is not translated
    assert!(!texts.contains(&"Contents".to_string()), "{texts:?}");
    let k = texts.iter().position(|t| t == "Second chapter text.").unwrap();
    set(&mut d, k, "Texte du deuxi\u{e8}me chapitre.");
    let out = merge(&d);
    let (m, _) = members(&out);
    let ch2 = &m["OEBPS/text/chapter 2.xhtml"];
    // the UTF-16 chapter stays UTF-16 (mark and its own declaration)
    assert!(ch2.starts_with(&[0xFF, 0xFE]) || ch2.starts_with(&[0xFE, 0xFF]), "{:?}", &ch2[..8]);
    let t = to_utf8(ch2);
    assert!(t.contains("Texte du deuxi\u{e8}me chapitre."), "{t}");
    assert!(!t.contains("encoding=\"UTF-8\"") && t.contains("encoding=\"UTF-16\""), "{t}");
    // the OPF keeps its byte-order mark
    assert!(m["OEBPS/content.opf"].starts_with(&[0xEF, 0xBB, 0xBF]) || m["OEBPS/content.opf"] == opf);
    assert_eq!(merge(&extract(&src)), src);
    well_formed(&m);
    // the OPF written with a mark when it changes
    let mut d = extract(&src);
    set(&mut d, 0, "Titre");
    let (m, _) = members(&merge(&d));
    assert!(m["OEBPS/content.opf"].starts_with(&[0xEF, 0xBB, 0xBF]));
    assert!(member_text(&m, "OEBPS/content.opf").contains(">Titre<"));
}

const ENC: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container"
  xmlns:enc="http://www.w3.org/2001/04/xmlenc#">
 <enc:EncryptedData>
  <enc:EncryptionMethod Algorithm="@ALG@"/>
  <enc:CipherData><enc:CipherReference URI="@URI@"/></enc:CipherData>
 </enc:EncryptedData>
</encryption>"#;

fn enc(alg: &str, uri: &str) -> String {
    ENC.replace("@ALG@", alg).replace("@URI@", uri)
}

#[test]
fn font_obfuscation_is_fine_and_drm_is_refused() {
    let ok = enc("http://www.idpf.org/2008/embedding", "OEBPS/fonts/f.otf");
    let src = epub_with(&[("META-INF/encryption.xml", ok.as_bytes())]);
    let mut d = extract(&src);
    assert_eq!(d.units.len(), 14);
    set(&mut d, 0, "Titre");
    let (m, _) = members(&merge(&d));
    assert_eq!(m["OEBPS/fonts/f.otf"], font());
    assert_eq!(m["META-INF/encryption.xml"], ok.as_bytes());
    let aes = "http://www.w3.org/2001/04/xmlenc#aes128-cbc";
    for (alg, uri) in [(aes, "OEBPS/text/ch1.xhtml"), (aes, "OEBPS/img/cover.png"), ("http://www.idpf.org/2008/embedding", "OEBPS/text/ch1.xhtml")] {
        let drm = enc(alg, uri);
        let e = extract_err(&epub_with(&[("META-INF/encryption.xml", drm.as_bytes())]));
        assert!(e.to_string().to_lowercase().contains("encrypted"), "{e}");
    }
}

#[test]
fn mimetype_rules() {
    for bad in [None, Some(&b"application/zip"[..])] {
        let e = extract_err(&build(&[], &[], bad, true));
        assert!(e.to_string().to_lowercase().contains("mimetype"), "{e}");
    }
    // a container with mimetype NOT first is accepted and repaired on write
    let src = build(&[], &[], Some(b"application/epub+zip"), false);
    let mut d = extract(&src);
    assert_eq!(merge(&d), src);
    set(&mut d, 0, "Titre");
    let (_, infos) = members(&merge(&d));
    assert_eq!(infos[0].name, "mimetype");
    assert!(infos[0].stored && infos[0].extra == 0);
}

#[test]
fn hostile_inputs() {
    let refused = |bytes: Vec<u8>, needle: &str| {
        let e = extract_err(&bytes);
        assert!(e.to_string().to_lowercase().contains(&needle.to_lowercase()), "{needle}: {e}");
        e
    };
    refused(b"not a zip at all".to_vec(), "zip");
    refused(build(&[], &["META-INF/container.xml"], Some(b"application/epub+zip"), true), "container.xml");
    refused(build(&[], &["OEBPS/content.opf"], Some(b"application/epub+zip"), true), "content.opf");
    let bomb = OPF
        .replace("<?xml version=\"1.0\" encoding=\"UTF-8\"?>", "<?xml version=\"1.0\"?><!DOCTYPE package [<!ENTITY a \"aaaaaaaaaa\"><!ENTITY b \"&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;\">]>")
        .replace("The Little Guide", "&b;");
    assert!(matches!(extract_err(&epub_with(&[("OEBPS/content.opf", bomb.as_bytes())])), Error::Limit(l) if l.kind == LimitKind::XmlEntities));
    let xxe = OPF.replace("<?xml version=\"1.0\" encoding=\"UTF-8\"?>", "<?xml version=\"1.0\"?><!DOCTYPE package [<!ENTITY x SYSTEM \"file:///etc/passwd\">]>");
    assert!(matches!(extract_err(&epub_with(&[("OEBPS/content.opf", xxe.as_bytes())])), Error::Limit(l) if l.kind == LimitKind::XmlEntities));
    refused(epub_with(&[("OEBPS/content.opf", b"<package")]), "damaged");
    // a container.xml rootfile escaping the root is ignored -> no package document
    let esc = CONTAINER.replace("OEBPS/content.opf", "../../etc/passwd");
    refused(epub_with(&[("META-INF/container.xml", esc.as_bytes())]), "package document");
    // a spine href escaping the container is skipped, not read
    let opf = OPF.replace("href=\"text/ch1.xhtml\"", "href=\"../../../secret.xhtml\"");
    let d = extract(&epub_with(&[("OEBPS/content.opf", opf.as_bytes())]));
    assert!(!loc_texts(&d).join(" ").contains("Hello"));
    // zip bomb: one highly compressible big member
    let big = vec![0u8; 3 * 1024 * 1024];
    let e = refused(epub_with(&[("OEBPS/img/zeros.bin", &big)]), "expands");
    assert!(matches!(e, Error::Limit(l) if l.kind == LimitKind::ZipRatio));
}

#[test]
fn segment_cap() {
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_units(5));
    match Epub.extract(&epub(), &o) {
        Err(Error::Limit(l)) => assert_eq!(l.kind, LimitKind::Units),
        other => panic!("expected the unit cap, got {:?}", other.map(|d| d.units.len())),
    }
}

#[test]
fn ingest_translate_export() {
    // LocOS runs ingest -> mock translate -> export through its pipeline; the same pass at the filter level.
    let src = epub();
    let mut d = extract(&src);
    d.target_lang = Some("fr-FR".into());
    translate_all(&mut d);
    let out = merge(&d);
    let (m, infos) = members(&out);
    assert_eq!(infos[0].name, "mimetype");
    assert!(member_text(&m, "OEBPS/content.opf").contains("<dc:language>fr-FR</dc:language>"));
    assert_eq!(m["OEBPS/fonts/f.otf"], font());
    assert!(!member_text(&m, "OEBPS/text/ch1.xhtml").contains('\u{27E6}'));
    well_formed(&m);
    assert_eq!(extract(&out).units.len(), 14);
}

#[test]
fn xhtml_safe_targets() {
    // LocOS treats a target as markup text and rewrites HTML-only entities as numeric references; a Bilingua target
    // is plain text, so what must hold is the same outcome: well-formed XHTML and OPF whatever the target holds.
    let mut d = extract(&epub());
    assert_eq!(loc_text(&d.units[7]), "Second chapter text.");
    assert_eq!(d.units[5].context.extra.get("locos-style").map(String::as_str), Some("a11y:img-alt"));
    set(&mut d, 0, "Titre\u{3} du livre");
    set(&mut d, 5, "Une couverture\u{2} &nbsp;rouge");
    set(&mut d, 7, "Deuxi\u{e8}me\u{a0}texte &bogus; A&amp;B\u{1}.");
    let (m, _) = members(&merge(&d));
    well_formed(&m);
    let ch2 = member_text(&m, "OEBPS/text/chapter 2.xhtml");
    // LocOS `_xhtml_safe`: the target is markup text; an unknown named reference becomes text, a typed `&amp;` stays
    assert!(ch2.contains("Deuxi\u{e8}me\u{a0}texte &amp;bogus; A&amp;B."), "{ch2}");
    let ch1 = member_text(&m, "OEBPS/text/ch1.xhtml");
    assert!(ch1.contains("alt=\"Une couverture &amp;nbsp;rouge\""), "{ch1}");
    let opf = member_text(&m, "OEBPS/content.opf");
    assert!(opf.contains("<dc:title") && opf.contains("Titre du livre"), "{opf}");
    // the text reads back as LocOS reads its own output (minus the forbidden characters)
    let again = extract(&merge(&d));
    assert_eq!(loc_text(&again.units[7]), "Deuxi\u{e8}me\u{a0}texte &amp;bogus; A&amp;B.");
}

#[test]
fn translate_no_is_honoured() {
    // #734: an XHTML chapter honours translate="no" through the shared html walk.
    let ch = CH1.replace(
        "<p>Hello <b>bold</b>",
        "<p translate=\"no\">Colophon text stays.</p>\n<p>Hello <b translate=\"no\">bold</b>",
    );
    let src = epub_with(&[("OEBPS/text/ch1.xhtml", ch.as_bytes())]);
    let mut d = extract(&src);
    let texts = loc_texts(&d);
    assert!(!texts.iter().any(|t| t.contains("Colophon") || t.contains("bold")), "{texts:?}");
    assert_eq!(merge(&d), src);
    translate_all(&mut d);
    let (m, _) = members(&merge(&d));
    let chapter = member_text(&m, "OEBPS/text/ch1.xhtml");
    assert!(chapter.contains("<p translate=\"no\">Colophon text stays.</p>"), "{chapter}");
    assert!(chapter.contains("<b translate=\"no\">bold</b>"), "{chapter}");
}

//! Round trips on the corpus, eleven languages and their stamps, encodings kept, zip details kept, sentence
//! segments written back.
mod common;
use bilingua_epub::Epub;
use bilingua_model::profile::tokens::{from_locos_text, to_locos_text};
use bilingua_model::*;
use common::*;
use std::path::Path;

fn corpus_files() -> Vec<(String, Vec<u8>)> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/epub");
    let mut v: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "epub"))
        .map(|p| (p.file_name().unwrap().to_string_lossy().into_owned(), std::fs::read(&p).unwrap()))
        .collect();
    v.sort();
    assert!(v.len() >= 4);
    v
}

fn pseudo(doc: &mut Document, mark: &str) {
    for i in 0..doc.units.len() {
        let u = &doc.units[i];
        let tgt: Vec<Inline> = u
            .source()
            .iter()
            .map(|x| match x {
                Inline::Text(t) => Inline::text(format!("{mark}{t}")),
                o => o.clone(),
            })
            .collect();
        let id = u.segments[0].id.clone();
        doc.set_target(&id, tgt).unwrap();
    }
}

#[test]
fn every_corpus_file_round_trips() {
    for (name, bytes) in corpus_files() {
        let mut d = extract(&bytes);
        assert!(!d.units.is_empty(), "{name}");
        assert_eq!(merge(&d), bytes, "{name}: untouched merge");
        // json round trip of the skeleton keeps the merge exact
        let json = serde_json::to_string(&d).unwrap();
        let back: Document = serde_json::from_str(&json).unwrap();
        assert_eq!(merge(&back), bytes, "{name}: after JSON");
        d.target_lang = Some("de-DE".into());
        pseudo(&mut d, "~");
        let out = merge(&d);
        assert_ne!(out, bytes);
        let (m, infos) = members(&out);
        assert_eq!(infos[0].name, "mimetype", "{name}");
        assert!(infos[0].stored && infos[0].extra == 0);
        well_formed(&m);
        let again = extract(&out);
        assert_eq!(again.units.len(), d.units.len(), "{name}");
        for (a, b) in again.units.iter().zip(&d.units) {
            assert_eq!(a.id, b.id, "{name}");
            let (ta, _) = to_locos_text(&a.source());
            let (tb, _) = to_locos_text(&b.output());
            assert_eq!(ta, tb, "{name} {}", a.id);
        }
        assert!(member_text(&m, "OEBPS/content.opf").contains("<dc:language>de-DE</dc:language>"), "{name}");
    }
}

#[test]
fn members_that_did_not_change_are_copied_byte_for_byte() {
    let src = epub();
    let mut d = extract(&src);
    set(&mut d, 0, "Titre");
    let out = merge(&d);
    let raw = |bytes: &[u8], name: &str| -> (u32, u32, Vec<u8>) {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        let mut f = z.by_name(name).unwrap();
        let (crc, size) = (f.crc32(), f.compressed_size() as u32);
        let mut v = Vec::new();
        std::io::Read::read_to_end(&mut f, &mut v).unwrap();
        (crc, size, v)
    };
    for name in ["OEBPS/text/ch1.xhtml", "OEBPS/nav.xhtml", "OEBPS/toc.ncx", "OEBPS/fonts/f.otf", "OEBPS/style.css"] {
        assert_eq!(raw(&src, name), raw(&out, name), "{name}");
    }
    let (_, a) = members(&src);
    let (_, b) = members(&out);
    for (x, y) in a.iter().zip(&b) {
        assert_eq!((&x.name, x.stored), (&y.name, y.stored));
    }
    // the OPF changed in exactly the title
    let (om, _) = members(&src);
    let (nm, _) = members(&out);
    let o = member_text(&om, "OEBPS/content.opf");
    let n = member_text(&nm, "OEBPS/content.opf");
    assert_eq!(o.replace("The Little Guide", "Titre"), n);
}

#[test]
fn eleven_languages_read_translate_and_stamp() {
    let (_, bytes) = corpus_files().into_iter().find(|(n, _)| n == "languages.epub").unwrap();
    let d0 = extract(&bytes);
    let texts = loc_texts(&d0);
    for t in ["The quick brown fox.", "El veloz zorro marrón.", "素早い茶色の狐。", "敏捷的棕色狐狸。", "빠른 갈색 여우.", "الثعلب البني السريع."] {
        assert!(texts.iter().any(|x| x == t), "{t}");
    }
    // each target locale: language and direction stamped on the OPF and on every chapter
    let targets = [
        ("en", false), ("es-ES", false), ("fr", false), ("it", false), ("de", false), ("nl", false), ("pt-BR", false),
        ("ja", false), ("zh-Hant", false), ("ko", false), ("ar", true), ("he", true), ("fa-IR", true), ("ur", true), ("ku-Arab", true),
    ];
    for (loc, rtl) in targets {
        let mut d = extract(&bytes);
        d.target_lang = Some(loc.into());
        pseudo(&mut d, "<<");
        let (m, _) = members(&merge(&d));
        well_formed(&m);
        let tag = loc.to_string();
        let opf = member_text(&m, "OEBPS/content.opf");
        assert!(opf.contains(&format!("<dc:language>{tag}</dc:language>")), "{loc}: {opf}");
        assert_eq!(opf.contains("page-progression-direction=\"rtl\""), rtl, "{loc}");
        for i in 1..=11 {
            let ch = member_text(&m, &format!("OEBPS/l{i}.xhtml"));
            assert!(ch.contains(&format!("lang=\"{tag}\"")) && ch.contains(&format!("xml:lang=\"{tag}\"")), "{loc} chapter {i}");
            let html_tag = &ch[ch.find("<html").unwrap()..];
            let html_tag = &html_tag[..html_tag.find('>').unwrap()];
            assert_eq!(html_tag.contains("dir=\"rtl\""), rtl, "{loc} chapter {i}: {html_tag}");
            assert!(ch.contains("&lt;&lt;"), "targets are escaped: {ch}");
        }
        assert!(member_text(&m, "OEBPS/toc.ncx").contains(&format!("xml:lang=\"{tag}\"")));
    }
    // a right-to-left book turned back to left to right
    let mut d = extract(&bytes);
    d.target_lang = Some("ar".into());
    pseudo(&mut d, "");
    let ar = merge(&d);
    let mut back = extract(&ar);
    back.target_lang = Some("en".into());
    pseudo(&mut back, "x");
    let (m, _) = members(&merge(&back));
    assert!(member_text(&m, "OEBPS/content.opf").contains("page-progression-direction=\"ltr\""));
    assert!(member_text(&m, "OEBPS/l1.xhtml").contains("dir=\"ltr\""));
}

#[test]
fn legacy_encodings_are_kept_on_a_translated_merge() {
    let (_, bytes) = corpus_files().into_iter().find(|(n, _)| n == "legacy2.epub").unwrap();
    let mut d = extract(&bytes);
    let texts = loc_texts(&d);
    assert!(texts.contains(&"Preheat the oven to 220 degrees.".to_string()));
    assert!(texts.contains(&"The caf\u{e9} serves cr\u{e8}me br\u{fb}l\u{e9}e daily.".to_string()));
    pseudo(&mut d, "\u{e9}t\u{e9} ");
    let out = merge(&d);
    let (m, _) = members(&out);
    // UTF-16 chapter stays UTF-16 with its declaration
    let c2 = &m["OEBPS/c2.xhtml"];
    assert!(c2.starts_with(&[0xFF, 0xFE]));
    assert!(to_utf8(c2).contains("encoding=\"UTF-16\"") && to_utf8(c2).contains("\u{e9}t\u{e9} Preheat"));
    // the Windows-1252 chapter stays Windows-1252, with its declaration
    let c3 = &m["OEBPS/c3.xhtml"];
    assert!(c3.windows(12).any(|w| w == b"windows-1252"));
    assert!(c3.contains(&0xE9) && !c3.windows(2).any(|w| w == [0xC3, 0xA9]), "single-byte \u{e9}, not UTF-8");
    // the OPF keeps its byte-order mark
    assert!(m["OEBPS/content.opf"].starts_with(&[0xEF, 0xBB, 0xBF]));
    well_formed(&m);
    assert_eq!(extract(&out).units.len(), d.units.len());
}

#[test]
fn sentence_segments_are_written_back_in_order() {
    let (_, bytes) = corpus_files().into_iter().find(|(n, _)| n == "guide.epub").unwrap();
    let mut d = extract(&bytes);
    // one unit with three sentences and a code in the middle
    let ch = CH1.replace("<p>Hello <b>bold</b> world, you have {count} items and %1$s left.</p>", "<p>First sentence here. Second one has <b>bold</b> text. Third one ends.</p>");
    let d2 = Epub.extract(&epub_with(&[("OEBPS/text/ch1.xhtml", ch.as_bytes())]), &ExtractOptions::default()).unwrap();
    let _ = &mut d;
    let mut doc = d2;
    bilingua_seg::segment(&mut doc, "en", Granularity::Sentence, bilingua_seg::Rules::bundled());
    let u = doc.units.iter().find(|u| loc_text(u).starts_with("First sentence")).unwrap().clone();
    assert!(u.segments.len() >= 3, "{} segments", u.segments.len());
    for s in &u.segments {
        let id = s.id.clone();
        let (_, map) = to_locos_text(&s.source);
        let t = format!("[{}]", to_locos_text(&s.source).0);
        doc.set_target(&id, from_locos_text(&t, &map)).unwrap();
    }
    let out = merge(&doc);
    let (m, _) = members(&out);
    let ch1 = member_text(&m, "OEBPS/text/ch1.xhtml");
    assert!(ch1.contains("[First sentence here.] [Second one has <b>bold</b> text.] [Third one ends.]"), "{ch1}");
    well_formed(&m);
}

#[test]
fn the_json_form_of_a_document_merges_the_same() {
    let src = epub();
    let mut d = extract(&src);
    translate_all(&mut d);
    let direct = merge(&d);
    let back: Document = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
    assert_eq!(merge(&back), direct);
}

#[test]
fn a_skeleton_from_another_parser_is_refused() {
    let mut d = extract(&epub());
    if let Skeleton::Zip { parser, .. } = &mut d.skeleton {
        *parser = Some(ParserInfo::new("bilingua-docx", "0.1.0", 1));
    }
    assert!(matches!(Epub.merge(&d), Err(Error::ParserVersion { .. })));
}

#[test]
fn nav_in_the_spine_is_read_once_and_a_second_rendition_is_ignored() {
    // the navigation document may also be a spine item: it is then a chapter, not a second nav
    let opf = OPF.replace("<itemref idref=\"c2\"/>", "<itemref idref=\"c2\"/><itemref idref=\"nav\"/><itemref idref=\"c1\"/>");
    let d = extract(&epub_with(&[("OEBPS/content.opf", opf.as_bytes())]));
    let texts = loc_texts(&d);
    assert_eq!(texts.iter().filter(|t| *t == "Contents").count(), 1);
    assert_eq!(texts.iter().filter(|t| t.starts_with("Hello")).count(), 1);
    // two rootfiles: the first one is the default rendition
    let two = CONTAINER.replace("</rootfiles>", "<rootfile full-path=\"OEBPS/other.opf\" media-type=\"application/oebps-package+xml\"/></rootfiles>");
    let src = epub_with(&[("META-INF/container.xml", two.as_bytes()), ("OEBPS/other.opf", b"<package/>")]);
    assert_eq!(extract(&src).units.len(), 14);
}

#[test]
fn libreoffice_opens_the_merged_chapters_when_installed() {
    // LibreOffice cannot open an .epub itself, so the check is on the content documents inside the merged file:
    // each translated XHTML chapter (Arabic, right to left, among them) must load and convert to PDF.
    let candidates = [
        r"C:\Program Files\LibreOffice\program\soffice.exe",
        r"C:\Program Files (x86)\LibreOffice\program\soffice.exe",
        "/usr/bin/soffice",
        "/usr/local/bin/soffice",
        "/Applications/LibreOffice.app/Contents/MacOS/soffice",
    ];
    let Some(soffice) = candidates.iter().find(|p| Path::new(p).exists()) else {
        eprintln!("skipped: LibreOffice is not installed");
        return;
    };
    let (_, bytes) = corpus_files().into_iter().find(|(n, _)| n == "languages.epub").unwrap();
    let mut d = extract(&bytes);
    d.target_lang = Some("ar".into());
    pseudo(&mut d, "");
    let (m, _) = members(&merge(&d));
    let dir = std::env::temp_dir().join(format!("bilingua-epub-lo-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for name in ["OEBPS/l1.xhtml", "OEBPS/l11.xhtml"] {
        let file = dir.join(name.rsplit('/').next().unwrap());
        std::fs::write(&file, &m[name]).unwrap();
        let out = std::process::Command::new(soffice).args(["--headless", "--convert-to", "pdf", "--outdir"]).arg(&dir).arg(&file).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        assert!(file.with_extension("pdf").exists(), "{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn translate_no_text_is_not_offered_and_the_ids_stay_consistent() {
    // LocOS #734 (the HTML filter since D4): translate="no" text is never a unit, with or without include_locked
    let ch = CH1.replace("<p>Hello <b>bold</b>", "<p translate=\"no\">Colophon text stays.</p>\n<p>Hello <b>bold</b>");
    let src = epub_with(&[("OEBPS/text/ch1.xhtml", ch.as_bytes())]);
    let o = ExtractOptions::new().with_include_locked(true);
    let mut d = Epub.extract(&src, &o).unwrap();
    assert!(d.units.iter().all(|u| !loc_text(u).starts_with("Colophon")));
    assert_eq!(d.units.len(), extract(&src).units.len());
    let i = d.units.iter().position(|u| loc_text(u).starts_with("Hello")).unwrap();
    set(&mut d, i, "Bonjour \u{27E6}g1\u{27E7}gras\u{27E6}/g1\u{27E7}");
    let (m, _) = members(&merge(&d));
    let c = member_text(&m, "OEBPS/text/ch1.xhtml");
    assert!(c.contains("<p translate=\"no\">Colophon text stays.</p>") && c.contains("<p>Bonjour <b>gras</b></p>"), "{c}");
}

#[test]
fn custom_limits_travel_with_the_skeleton() {
    // a file read under raised limits can be merged again under the same ones
    let big = vec![b'a'; 3 * 1024 * 1024];
    let src = epub_with(&[("OEBPS/img/pad.bin", &big)]);
    let o = ExtractOptions::new().with_limits(Limits::new().with_max_zip_ratio(1_000_000));
    let mut d = Epub.extract(&src, &o).unwrap();
    set(&mut d, 0, "Titre");
    let (m, _) = members(&merge(&d));
    assert!(member_text(&m, "OEBPS/content.opf").contains(">Titre<"));
    assert_eq!(m["OEBPS/img/pad.bin"], big);
}

#[test]
fn an_apostrophe_inside_a_link_is_written_once_escaped() {
    // wild run: an apostrophe inside a link read back as the text "&#x27;". The chapter must carry a character
    // reference (or the plain character), never "&amp;#x27;", which a reader would show as text.
    let ch = CH1.replace(
        "<p>Hello <b>bold</b>",
        "<p>It looks like <a href=\"https://example.org/O%27s_page\">O'Neill's chart</a>: done.</p>\n<p>Hello <b>bold</b>",
    );
    let src = epub_with(&[("OEBPS/text/ch1.xhtml", ch.as_bytes())]);
    let mut d = extract(&src);
    let i = d.units.iter().position(|u| loc_text(u).starts_with("It looks")).unwrap();
    let t = format!("Zq{}qZ", loc_text(&d.units[i]));
    set(&mut d, i, &t);
    let (m, _) = members(&merge(&d));
    well_formed(&m);
    let c = member_text(&m, "OEBPS/text/ch1.xhtml");
    assert!(!c.contains("&amp;#"), "{c}");
    assert!(c.contains("O&#x27;Neill&#x27;s chart</a>: done.qZ</p>") || c.contains("O'Neill's chart</a>: done.qZ</p>"), "{c}");
    assert!(c.contains("href=\"https://example.org/O%27s_page\""), "{c}");
}

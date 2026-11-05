//! Opus gate findings: merge options, the format option for loaded targets, the hidden codes, the stamp scope and
//! the paths that were quadratic.

mod common;

use bilingua_model::*;
use bilingua_wordfast::{Txlf, Txml};
use common::*;
use std::time::Instant;

const DE_TXML: &[u8] = include_bytes!("data/lang_de.txml");
const DE_TXLF: &[u8] = include_bytes!("data/lang_de.txlf");
const AR16_TXML: &[u8] = include_bytes!("data/lang_ar_utf16le.txml");
const KO16_TXLF: &[u8] = include_bytes!("data/lang_ko_utf16be.txlf");

const NO_LANG_TXML: &[u8] = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<txml locale=\"en_US\" version=\"1.0\">\n<translatable blockId=\"1\"><segment segmentId=\"1\"><source>Open the file.</source></segment></translatable>\n</txml>\n";
const NO_LANG_TXLF: &[u8] = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\" xmlns:gs4tr=\"http://www.gs4tr.org/schema/xliff-ext\">\n<file original=\"a\" source-language=\"en-US\" datatype=\"plaintext\"><body>\n<trans-unit id=\"1\"><source>Open the file.</source></trans-unit>\n</body></file>\n<file original=\"b\" source-language=\"en-US\" datatype=\"plaintext\"><body>\n<trans-unit id=\"2\"><source>Close it.</source></trans-unit>\n</body></file></xliff>\n";

fn first_seg(d: &Document) -> String {
    d.units[0].segments[0].id.clone()
}

#[test]
fn merge_with_default_is_merge() {
    for (f, data) in [(&TXML as &dyn Filter, DE_TXML), (&TXLF, DE_TXLF), (&TXML, AR16_TXML), (&TXLF, KO16_TXLF)] {
        let d = extract(f, data);
        assert_eq!(f.merge_with(&d, &MergeOptions::default()).unwrap(), data);
        assert_eq!(f.merge(&d).unwrap(), data);
    }
}

#[test]
fn stamp_target_always_stamps_an_untouched_file() {
    let o = ExtractOptions::new().with_target_lang("de-DE");
    let always = MergeOptions::new().with_stamp_target_always(true);
    // TXML: the file's own en_US style
    let d = Txml::default().extract(NO_LANG_TXML, &o).unwrap();
    assert_eq!(Txml::default().merge(&d).unwrap(), NO_LANG_TXML, "nothing translated: original bytes");
    let out = String::from_utf8(Txml::default().merge_with(&d, &always).unwrap()).unwrap();
    assert!(out.contains("<txml locale=\"en_US\" version=\"1.0\" targetlocale=\"de_DE\">"), "{out}");
    assert_eq!(out.replace(" targetlocale=\"de_DE\"", ""), String::from_utf8_lossy(NO_LANG_TXML));
    // TXLF: every <file>
    let d = Txlf::default().extract(NO_LANG_TXLF, &o).unwrap();
    assert_eq!(Txlf::default().merge(&d).unwrap(), NO_LANG_TXLF);
    let out = String::from_utf8(Txlf::default().merge_with(&d, &always).unwrap()).unwrap();
    assert_eq!(out.matches("target-language=\"de-DE\"").count(), 2, "{out}");
    // an existing value is never changed, and no target_lang means no stamp
    let d = extract(&TXML, DE_TXML);
    assert_eq!(TXML.merge_with(&d, &always).unwrap(), DE_TXML);
    let mut d = Txml::default().extract(NO_LANG_TXML, &ExtractOptions::default()).unwrap();
    d.target_lang = None;
    assert_eq!(Txml::default().merge_with(&d, &always).unwrap(), NO_LANG_TXML);
}

#[test]
fn utf8_output_rewrites_the_declaration_and_reads_back() {
    let utf8 = MergeOptions::new().with_output_encoding(OutputEncoding::Utf8);
    for (f, data) in [(&TXML as &dyn Filter, AR16_TXML), (&TXLF, KO16_TXLF)] {
        let mut d = extract(f, data);
        let id = first_seg(&d);
        let src = d.units[0].source();
        d.set_target(&id, src).unwrap();
        d.units[0].segments[0].target.push(Inline::text(" X"));
        let out = f.merge_with(&d, &utf8).unwrap();
        assert!(!out.starts_with(&[0xFF, 0xFE]) && !out.starts_with(&[0xFE, 0xFF]));
        let text = std::str::from_utf8(&out).expect("UTF-8 out");
        assert!(text.starts_with("<?xml") && text[..60].to_ascii_uppercase().contains("UTF-8"), "{}", &text[..80]);
        let all = opts().with_include_locked(true);
        assert_eq!(f.extract(&out, &all).expect("reads back").units.len(), f.extract(data, &all).unwrap().units.len());
        // the kept encoding writes UTF-16 again
        let kept = f.merge(&d).unwrap();
        assert_eq!(&kept[..2], &data[..2]);
    }
}

#[test]
fn load_targets_from_format_options() {
    let off = opts().with_format_options([("wordfast.load_targets".to_string(), "false".to_string())].into());
    let on = opts().with_format_options([("wordfast.load_targets".to_string(), "yes".to_string())].into());
    let ids = |d: Document| d.units.into_iter().map(|u| u.id).collect::<Vec<_>>();
    {
        let data = DE_TXML;
        assert_eq!(ids(Txml::default().extract(data, &off).unwrap()), ids(TXML.extract(data, &opts()).unwrap()));
        assert_eq!(ids(TXML.extract(data, &on).unwrap()), ids(Txml::default().extract(data, &opts()).unwrap()));
        assert_ne!(ids(TXML.extract(data, &opts()).unwrap()), ids(Txml::default().extract(data, &opts()).unwrap()));
    }
    assert_eq!(ids(Txlf::default().extract(DE_TXLF, &off).unwrap()), ids(TXLF.extract(DE_TXLF, &opts()).unwrap()));
    assert_eq!(ids(TXLF.extract(DE_TXLF, &on).unwrap()), ids(Txlf::default().extract(DE_TXLF, &opts()).unwrap()));
}

#[test]
fn a_target_naming_a_hidden_code_splices_nothing() {
    for (f, data) in [(&TXML as &dyn Filter, DE_TXML), (&TXLF, DE_TXLF)] {
        let mut d = extract(f, data);
        // written past set_target (which would refuse the invented tags), as a hand-edited document could be
        let tags: Vec<Inline> = ["~raw", "~pre", "~post", "~t0", "~key", "~lead"]
            .iter()
            .flat_map(|id| {
                [TagKind::Standalone, TagKind::Open, TagKind::Close].map(|kind| Inline::tag(id.to_string(), kind, String::new()))
            })
            .collect();
        let mut t = vec![Inline::text("Neu ")];
        t.extend(tags);
        t.push(Inline::text("Ende"));
        d.units[0].segments[0].target = t;
        let out = f.merge(&d).unwrap();
        let s = String::from_utf8(out.clone()).unwrap();
        assert!(s.contains(">Neu Ende<"), "{s}");
        let all = opts().with_include_locked(true);
        assert_eq!(f.extract(&out, &all).unwrap().units.len(), f.extract(data, &all).unwrap().units.len());
    }
}

#[test]
fn a_tampered_splice_is_a_write_error_not_a_broken_file() {
    let mut d = extract(&TXML, DE_TXML);
    d.units[0].set_hidden("post", "</target></segment>");
    d.units[0].segments[0].target = vec![Inline::text("Neu")];
    assert!(matches!(TXML.merge(&d), Err(Error::Write(_))));
}

#[test]
fn txlf_stamp_only_touches_file_elements_of_the_root() {
    // an inline element that happens to be called <file> is a code, not an XLIFF file
    let data = b"<?xml version=\"1.0\"?>\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\" xmlns:gs4tr=\"http://www.gs4tr.org/schema/xliff-ext\">\n<file original=\"a\" source-language=\"en-US\" datatype=\"plaintext\"><body>\n<trans-unit id=\"1\"><source>Open <file/> now.</source></trans-unit>\n</body></file></xliff>\n";
    let o = ExtractOptions::new().with_target_lang("fr-FR");
    let mut d = Txlf::default().extract(data, &o).unwrap();
    let id = first_seg(&d);
    let src = d.units[0].source();
    d.set_target(&id, src).unwrap();
    d.units[0].segments[0].target.insert(0, Inline::text("Ouvrir "));
    let out = String::from_utf8(Txlf::default().merge(&d).unwrap()).unwrap();
    assert_eq!(out.matches("target-language=\"fr-FR\"").count(), 1, "{out}");
    assert!(out.contains("<file original=\"a\" source-language=\"en-US\" datatype=\"plaintext\" target-language=\"fr-FR\">"), "{out}");
}

#[test]
fn many_notes_and_segments_stay_linear() {
    let n = 3000;
    let mut s = String::from("<?xml version=\"1.0\"?>\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\" xmlns:gs4tr=\"http://www.gs4tr.org/schema/xliff-ext\"><file original=\"a\" source-language=\"en\" datatype=\"plaintext\"><body><trans-unit id=\"1\"><source>x</source><seg-source>");
    for i in 0..n {
        s.push_str(&format!("<mrk mtype=\"seg\" mid=\"{i}\">Line {i}.</mrk>"));
    }
    s.push_str("</seg-source>");
    for i in 0..n {
        s.push_str(&format!("<note>Note {i}</note>"));
    }
    s.push_str("</trans-unit></body></file></xliff>");
    let t = Instant::now();
    let d = Txlf::default().extract(s.as_bytes(), &ExtractOptions::default()).unwrap();
    assert_eq!(d.units.len(), n);
    assert_eq!(d.units[0].notes.len(), n);
    assert!(d.units[1..].iter().all(|u| u.notes.is_empty()));
    assert!(t.elapsed().as_secs() < 20, "{:?}", t.elapsed());
    // a small number of notes still goes to every segment
    let d = Txlf::default().extract(include_bytes!("data/edge_segsource.txlf"), &ExtractOptions::default()).unwrap();
    let with: Vec<usize> = d.units.iter().map(|u| u.notes.len()).collect();
    assert!(with.windows(2).all(|w| w[0] == w[1]) || with.iter().all(|n| *n == 0), "{with:?}");
}

#[test]
fn stamping_many_files_stays_linear() {
    let n = 20_000;
    let mut s = String::from("<?xml version=\"1.0\"?>\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\" xmlns:gs4tr=\"http://www.gs4tr.org/schema/xliff-ext\">");
    for i in 0..n {
        s.push_str(&format!("<file original=\"f{i}\" source-language=\"en\" datatype=\"plaintext\"><body><trans-unit id=\"{i}\"><source>Line {i} of the long file with some padding text.</source></trans-unit></body></file>\n"));
    }
    s.push_str("</xliff>");
    let o = ExtractOptions::new().with_target_lang("de");
    let mut d = Txlf::default().extract(s.as_bytes(), &o).unwrap();
    let id = first_seg(&d);
    d.set_target(&id, vec![Inline::text("Zeile")]).unwrap();
    let t = Instant::now();
    let out = Txlf::default().merge(&d).unwrap();
    assert!(t.elapsed().as_secs() < 20, "{:?}", t.elapsed());
    assert_eq!(String::from_utf8(out).unwrap().matches("target-language=\"de\"").count(), n);
}

#[test]
fn the_parser_depth_cap_holds_on_a_small_stack_whatever_the_limits() {
    // a caller may raise max_xml_depth; the crate's own 1024 cap keeps the recursive walkers inside a 1 MB stack
    let big = Limits::new().with_max_xml_depth(100_000);
    let run = move || {
        for n in [1000usize, 1018] {
            let g = format!("<xliff version=\"1.2\"><file><body><trans-unit id=\"1\"><source>{}x{}</source></trans-unit></body></file></xliff>", "<g id=\"1\">".repeat(n), "</g>".repeat(n));
            let o = opts().with_limits(big);
            let mut d = TXLF.extract(g.as_bytes(), &o).unwrap();
            assert_eq!(TXLF.merge(&d).unwrap(), g.as_bytes());
            let id = d.units[0].segments[0].id.clone();
            let src = d.units[0].source();
            d.set_target(&id, src).unwrap();
            d.units[0].segments[0].target.push(Inline::text("y"));
            assert!(TXLF.merge(&d).is_ok());
            let t = format!("<txml><translatable><segment><source>a{}x{}</source></segment></translatable></txml>", "<ut>".repeat(n), "</ut>".repeat(n));
            assert_eq!(TXML.extract(t.as_bytes(), &o).unwrap().units.len(), 1);
        }
        let g = format!("<xliff version=\"1.2\"><file><body><trans-unit id=\"1\"><source>{}x{}</source></trans-unit></body></file></xliff>", "<g id=\"1\">".repeat(5000), "</g>".repeat(5000));
        let o = opts().with_limits(big);
        assert!(matches!(TXLF.extract(g.as_bytes(), &o), Err(Error::Limit(_))));
    };
    std::thread::Builder::new().stack_size(1 << 20).spawn(run).unwrap().join().unwrap();
}

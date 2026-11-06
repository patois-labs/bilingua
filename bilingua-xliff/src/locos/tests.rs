//! The LocOS-profile XLIFF port: units, keys, writing, hostile input.
use super::*;
use bilingua_model::Filter;
use proptest::prelude::*;

fn filter(id: &str) -> Box<dyn Filter> {
    crate::filters().into_iter().find(|f| f.id() == id).unwrap()
}

fn opts(lang: Option<&str>) -> ExtractOptions {
    profile::locos().extract_options().with_target_lang(lang.map(str::to_string))
}

fn texts(doc: &Document) -> Vec<(String, String)> {
    doc.units
        .iter()
        .map(|u| (u.context.extra[profile::P_INDEX].clone(), profile::tokens::to_locos_text(&u.segments[0].source).0))
        .collect()
}

fn merge_with(id: &str, data: &[u8], lang: Option<&str>, set: &[(&str, &str)]) -> Result<Vec<u8>, Error> {
    let f = filter(id);
    let mut doc = f.extract(data, &opts(lang))?;
    for (p, t) in set {
        let seg = doc.units.iter().find(|u| u.context.extra[profile::P_INDEX] == *p).unwrap().segments[0].clone();
        let (_, map) = profile::tokens::to_locos_text(&seg.source);
        profile::locos_set_target(&mut doc, &seg.id, profile::tokens::from_locos_text(t, &map))?;
    }
    f.merge_with(&doc, &profile::locos().merge_options(id))
}

const CRLF: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\">\r\n  <file original=\"a.txt\" source-language=\"en-US\" datatype=\"plaintext\">\r\n    <body>\r\n      <trans-unit id=\"1\">\r\n        <source>Hello world</source>\r\n      </trans-unit>\r\n      <trans-unit id=\"2\">\r\n        <source>Good night</source>\r\n      </trans-unit>\r\n    </body>\r\n  </file>\r\n</xliff>\r\n";

#[test]
fn plain_xliff_is_written_like_lxml() {
    let out = merge_with("xliff", CRLF.as_bytes(), Some("fr_FR"), &[("0", "Bonjour le monde")]).unwrap();
    let want = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n<xliff xmlns=\"urn:oasis:names:tc:xliff:document:1.2\" version=\"1.2\">\r\n  <file original=\"a.txt\" source-language=\"en-US\" datatype=\"plaintext\" target-language=\"fr-FR\">\r\n    <body>\r\n      <trans-unit id=\"1\">\r\n        <source>Hello world</source>\r\n      <target>Bonjour le monde</target></trans-unit>\r\n      <trans-unit id=\"2\">\r\n        <source>Good night</source>\r\n      </trans-unit>\r\n    </body>\r\n  </file>\r\n</xliff>\r\n";
    assert_eq!(String::from_utf8(out).unwrap(), want);
}

#[test]
fn begin_end_codes_are_points_and_protected_units_keep_their_number() {
    let x = "<xliff version=\"1.2\"><file><body>\
        <trans-unit id=\"a\" approved=\"yes\"><source>Done.</source></trans-unit>\
        <trans-unit id=\"b\"><source>Delete <x id=\"name\"/> from <ph id=\"2\">%s</ph> now <bpt id=\"3\">&lt;b&gt;</bpt>bold<ept id=\"3\">&lt;/b&gt;</ept></source></trans-unit>\
        <trans-unit id=\"c\"><source><bpt id=\"1\"/><ept id=\"1\"/></source></trans-unit>\
        <trans-unit id=\"d\" translate=\"no\"><source>No.</source></trans-unit>\
        <trans-unit id=\"e\"><source>  <g id=\"1\">Last</g> one  </source><target state=\"new\">x</target></trans-unit>\
        </body></file></xliff>";
    let doc = filter("xliff").extract(x.as_bytes(), &opts(None)).unwrap();
    assert_eq!(
        texts(&doc),
        vec![
            ("1".to_string(), "Delete \u{27e6}p1\u{27e7} from \u{27e6}x2\u{27e7} now \u{27e6}x3\u{27e7}bold\u{27e6}x4\u{27e7}".to_string()),
            ("4".to_string(), "\u{27e6}g1\u{27e7}Last\u{27e6}/g1\u{27e7} one".to_string()),
        ]
    );
    let out = merge_with("xliff", x.as_bytes(), None, &[("4", "\u{27e6}g1\u{27e7}Zuletzt\u{27e6}/g1\u{27e7} eins")]).unwrap();
    let out = String::from_utf8(out).unwrap();
    assert!(out.contains("<target state=\"translated\"><g id=\"1\">Zuletzt</g> eins</target>"), "{out}");
}

const SDL: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?><xliff xmlns:sdl=\"http://sdl.com/FileTypes/SdlXliff/1.0\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\" version=\"1.2\" sdl:version=\"1.0\">\n<file original=\"a.docx\" source-language=\"en-US\" target-language=\"de-DE\"><body>\n<trans-unit id='u1'><source>Open <g id=\"pt1\">it</g>. Then save.</source><seg-source><mrk mtype=\"seg\" mid=\"1\">Open <g id=\"pt1\">it</g>.</mrk> <mrk mtype=\"seg\" mid=\"2\">Then save.</mrk></seg-source><target><mrk mtype=\"seg\" mid=\"1\"/> <mrk mtype=\"seg\" mid=\"2\"></mrk></target><sdl:seg-defs><sdl:seg id=\"1\" conf=\"Draft\"/></sdl:seg-defs></trans-unit>\n<trans-unit id='u2'><source>Locked.</source><seg-source><mrk mtype=\"seg\" mid=\"3\">Locked.</mrk></seg-source><sdl:seg-defs><sdl:seg id=\"3\" locked=\"true\"/></sdl:seg-defs></trans-unit>\n<trans-unit id='u3'><source>Line<x id=\"ph2\"/>two\u{a0}more</source><seg-source><mrk mtype=\"seg\" mid=\"4\">Line<x id=\"ph2\"/>two\u{a0}more</mrk></seg-source></trans-unit>\n</body></file></xliff>";

#[test]
fn sdlxliff_segments_are_spliced() {
    let doc = filter("sdlxliff").extract(SDL.as_bytes(), &opts(None)).unwrap();
    assert_eq!(
        texts(&doc),
        vec![
            ("1000000000".into(), "Open \u{27e6}g1\u{27e7}it\u{27e6}/g1\u{27e7}.".into()),
            ("1000000001".into(), "Then save.".into()),
            ("1000000003".into(), "Line\u{27e6}p1\u{27e7}two\u{a0}more".into()),
        ]
    );
    let out = merge_with("sdlxliff", SDL.as_bytes(), Some("de-DE"), &[("1000000000", "\u{27e6}g1\u{27e7}Offnen\u{27e6}/g1\u{27e7} & gut."), ("1000000003", "Zeile\u{27e6}p1\u{27e7}zwei")]).unwrap();
    let out = String::from_utf8(out).unwrap();
    assert!(out.contains("<target><mrk mtype=\"seg\" mid=\"1\"><g id=\"pt1\">Offnen</g> &amp; gut.</mrk> <mrk mtype=\"seg\" mid=\"2\"></mrk></target>"), "{out}");
    assert!(out.contains("<sdl:seg id=\"1\" conf=\"Translated\"/>"), "{out}");
    assert!(out.contains("<target><mrk mtype=\"seg\" mid=\"4\">Zeile<x id=\"ph2\"/>zwei</mrk></target><sdl:seg-defs><sdl:seg id=\"4\" conf=\"Translated\"/></sdl:seg-defs>"), "{out}");
    // everything else is the original's bytes
    assert!(out.starts_with("<?xml version=\"1.0\" encoding=\"utf-8\"?><xliff xmlns:sdl="));
    assert!(out.contains("<trans-unit id='u2'>"));
    // nothing written and a target language already there: the original
    assert_eq!(merge_with("sdlxliff", SDL.as_bytes(), Some("fr-FR"), &[]).unwrap(), SDL.as_bytes());
}

#[test]
fn utf16_vendor_file_goes_back_in_utf16() {
    let mut data = vec![0xFF, 0xFE];
    SDL.replace("utf-8", "utf-16").encode_utf16().for_each(|u| data.extend_from_slice(&u.to_le_bytes()));
    let out = merge_with("sdlxliff", &data, None, &[("1000000001", "Dann speichern.")]).unwrap();
    assert!(out.starts_with(&[0xFF, 0xFE]));
    let units: Vec<u16> = out[2..].chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    assert!(String::from_utf16(&units).unwrap().contains("<mrk mtype=\"seg\" mid=\"2\">Dann speichern.</mrk>"));
}

#[test]
fn hostile_files_are_refused() {
    let f = filter("sdlxliff");
    // not the vendor's namespace, XLIFF 2, a legacy encoding, a DOCTYPE, broken XML
    for bad in [
        "<xliff version=\"1.2\"><file><body><trans-unit id=\"1\"><source>a</source></trans-unit></body></file></xliff>".to_string(),
        "<xliff xmlns:sdl=\"http://sdl.com/FileTypes/SdlXliff/1.0\" version=\"2.0\"><file/></xliff>".to_string(),
        SDL.replace("utf-8", "iso-8859-1"),
        SDL.replacen("<xliff", "<!DOCTYPE xliff><xliff", 1),
        SDL.replace("</body>", ""),
    ] {
        assert!(f.extract(bad.as_bytes(), &opts(None)).is_err(), "{bad}");
    }
    // a forged skeleton nested too deeply is refused at merge, not a stack overflow
    let mut doc = filter("xliff").extract(CRLF.as_bytes(), &opts(None)).unwrap();
    doc.skeleton = Skeleton::binary(format!("<xliff>{}", "<g>".repeat(5000)).into_bytes(), bilingua_model::parser_info!());
    assert!(filter("xliff").merge_with(&doc, &profile::locos().merge_options("xliff")).is_err());
}

#[test]
fn broken_target_tokens_never_break_the_file() {
    let x = "<xliff version=\"1.2\"><file><body><trans-unit id=\"1\"><source>A <g id=\"1\">b <x id=\"2\"/></g> c</source></trans-unit></body></file></xliff>";
    for t in ["\u{27e6}g1\u{27e7}\u{27e6}g1\u{27e7}x\u{27e6}/g1\u{27e7}", "\u{27e6}/g1\u{27e7} \u{27e6}x9\u{27e7} \u{27e6}g7\u{27e7}y", "\u{27e6}x2\u{27e7}\u{27e6}x2\u{27e7}\u{27e6}g1\u{27e7}", "<&>\u{0}\u{b}"] {
        let f = filter("xliff");
        let mut doc = f.extract(x.as_bytes(), &opts(None)).unwrap();
        let seg = doc.units[0].segments[0].clone();
        let (_, map) = profile::tokens::to_locos_text(&seg.source);
        // the model refuses a target that repeats a code; what it accepts must still write a well-formed file
        if doc.set_target(&seg.id, profile::tokens::from_locos_text(t, &map)).is_err() {
            continue;
        }
        let out = f.merge_with(&doc, &profile::locos().merge_options("xliff")).unwrap();
        assert!(dom::parse(std::str::from_utf8(&out).unwrap()).is_ok(), "{t:?}");
        // the vendor splice too
        let s = SDL.as_bytes();
        let f = filter("sdlxliff");
        let mut doc = f.extract(s, &opts(None)).unwrap();
        let seg = doc.units[0].segments[0].clone();
        doc.set_target(&seg.id, vec![Inline::text(t), Inline::tag("1", TagKind::Close, ""), Inline::tag("99", TagKind::Open, "")]).unwrap_or(());
        let out = f.merge_with(&doc, &profile::locos().merge_options("sdlxliff")).unwrap();
        assert!(dom::parse(std::str::from_utf8(&out).unwrap()).is_ok(), "{t:?}");
    }
}

#[test]
fn order_bound_codes() {
    for c in ["%s", "%d", "%@", "%-5.2f", "%lld", "%I64d", "{}", "%*d"] {
        assert!(order_bound(c), "{c}");
    }
    for c in ["%1$s", "{0}", "%", "%y", "abc", "%5", "%.f"] {
        assert!(!order_bound(c), "{c}");
    }
}

#[test]
fn mxliff_text_marks_are_codes() {
    let x = "<xliff xmlns:m=\"http://www.memsource.com/mxlf/2.0\" version=\"1.2\"><file><body><trans-unit id=\"1\"><source>Click {1&gt;Save&lt;1} to keep {2} changes.</source></trans-unit></body></file></xliff>";
    let doc = filter("mxliff").extract(x.as_bytes(), &opts(None)).unwrap();
    assert_eq!(texts(&doc)[0].1, "Click \u{27e6}g1\u{27e7}Save\u{27e6}/g1\u{27e7} to keep \u{27e6}x2\u{27e7} changes.");
    let out = merge_with("mxliff", x.as_bytes(), None, &[("0", "\u{27e6}g1\u{27e7}Sichern\u{27e6}/g1\u{27e7} \u{27e6}x2\u{27e7}")]).unwrap();
    assert!(String::from_utf8(out).unwrap().contains("<target>{1>Sichern&lt;1} {2}</target>"));
}

fn piece() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z &<>'\"\u{a0}\u{e9}\r\n\t]{0,8}".prop_map(|s| dom::esc_text(&s)),
        Just("<x id=\"1\"/>".to_string()),
        Just("<ph id=\"p\">%s</ph>".to_string()),
        Just("<bpt id=\"3\">&lt;b&gt;</bpt>".to_string()),
        Just("<g id=\"g\">in</g>".to_string()),
        Just("<!-- c -->".to_string()),
        Just("<![CDATA[<&>]]>".to_string()),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn any_source_and_target_give_well_formed_xliff(src in proptest::collection::vec(piece(), 0..8), tgt in "[a-z \u{27e6}\u{27e7}/gxp0-9&<]{0,30}", seg in any::<bool>()) {
        let body = src.concat();
        let x = if seg {
            format!("<xliff xmlns:sdl=\"http://sdl.com/FileTypes/SdlXliff/1.0\" version=\"1.2\"><file><body><trans-unit id=\"1\"><source>{body}</source><seg-source><mrk mtype=\"seg\" mid=\"1\">{body}</mrk></seg-source></trans-unit></body></file></xliff>")
        } else {
            format!("<xliff version=\"1.2\"><file><body><trans-unit id=\"1\"><source>{body}</source></trans-unit></body></file></xliff>")
        };
        let id = if seg { "sdlxliff" } else { "xliff" };
        let f = filter(id);
        let mut doc = f.extract(x.as_bytes(), &opts(Some("de-DE"))).unwrap();
        if let Some(seg) = doc.units.first().map(|u| u.segments[0].clone()) {
            let (_, map) = profile::tokens::to_locos_text(&seg.source);
            let _ = profile::locos_set_target(&mut doc, &seg.id, profile::tokens::from_locos_text(&tgt, &map));
        }
        let out = f.merge_with(&doc, &profile::locos().merge_options(id)).unwrap();
        let out = String::from_utf8(out).unwrap();
        prop_assert!(dom::parse(&out).is_ok(), "{}", out);
        // the file reads back under the profile
        prop_assert!(f.extract(out.as_bytes(), &opts(None)).is_ok());
    }
}

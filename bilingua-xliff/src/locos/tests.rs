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
        .map(|u| (u.context.extra()[profile::P_INDEX].to_string(), profile::tokens::to_locos_text(&u.segments[0].source).0))
        .collect()
}

fn merge_with(id: &str, data: &[u8], lang: Option<&str>, set: &[(&str, &str)]) -> Result<Vec<u8>, Error> {
    let f = filter(id);
    let mut doc = f.extract(data, &opts(lang))?;
    for (p, t) in set {
        let seg = doc.units.iter().find(|u| &u.context.extra()[profile::P_INDEX] == *p).unwrap().segments[0].clone();
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

/// Issue 794: a comment or processing instruction inside a source is not text and takes no code number.
#[test]
fn comment_in_a_source_is_dropped() {
    let x = "<xliff version=\"1.2\"><file><body>\
        <trans-unit id=\"1\"><source>Open <!-- SECRET --> the <?pi hidden?>file<g id=\"1\">!</g></source></trans-unit>\
        </body></file></xliff>";
    let doc = filter("xliff").extract(x.as_bytes(), &opts(Some("fr_FR"))).unwrap();
    let t = texts(&doc);
    assert_eq!(t[0].1, "Open  the file\u{27e6}g1\u{27e7}!\u{27e6}/g1\u{27e7}", "{t:?}");
    let out = merge_with("xliff", x.as_bytes(), Some("fr_FR"), &[("0", "Ouvrir le fichier\u{27e6}g1\u{27e7}!\u{27e6}/g1\u{27e7}")]).unwrap();
    let out = String::from_utf8(out).unwrap();
    assert!(out.contains("<target>Ouvrir le fichier<g id=\"1\">!</g></target>"), "{out}");
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

// ---------------------------------------------------------------- token look-alikes (issue 814 item 1)

/// A source whose text holds the very tokens its codes get (`⟦g2⟧`, `⟦/g2⟧`, `⟦x1⟧`), an unknown one (`⟦7⟧`), a
/// stray bracket and the private-use characters the escape uses. Codes: `<x id="a"/>` is 1, `<g id="b">` is 2.
const LOOK: &str = "Use \u{27e6}g2\u{27e7} and \u{27e6}/g2\u{27e7} <x id=\"a\"/> <g id=\"b\">bold</g> \u{27e6}x1\u{27e7} \u{e000}\u{e001}\u{27e6}7\u{27e7} \u{27e6}";

fn look_files() -> Vec<(&'static str, String)> {
    vec![
        ("xliff", format!("<xliff version=\"1.2\"><file><body><trans-unit id=\"1\"><source>{LOOK}</source></trans-unit></body></file></xliff>")),
        (
            "sdlxliff",
            format!(
                "<xliff xmlns:sdl=\"http://sdl.com/FileTypes/SdlXliff/1.0\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\" version=\"1.2\"><file><body><trans-unit id=\"1\"><source>{LOOK}</source><seg-source><mrk mtype=\"seg\" mid=\"1\">{LOOK}</mrk></seg-source></trans-unit></body></file></xliff>"
            ),
        ),
    ]
}

/// The source's tag with this id and kind (its display and free flag as extracted).
fn tag_of(src: &[Inline], id: &str, kind: TagKind) -> Inline {
    src.iter().find(|i| matches!(i, Inline::Tag { id: t, kind: k, .. } if t == id && *k == kind)).cloned().unwrap()
}

fn set_inlines(id: &str, data: &[u8], target: impl Fn(&[Inline]) -> Vec<Inline>) -> String {
    let f = filter(id);
    let mut doc = f.extract(data, &opts(None)).unwrap();
    let seg = doc.units[0].segments[0].clone();
    profile::locos_set_target(&mut doc, &seg.id, target(&seg.source)).unwrap();
    String::from_utf8(f.merge_with(&doc, &profile::locos().merge_options(id)).unwrap()).unwrap()
}

#[test]
fn look_alikes_in_the_source_come_back_byte_for_byte() {
    for (id, x) in look_files() {
        let doc = filter(id).extract(x.as_bytes(), &opts(None)).unwrap();
        let src = &doc.units[0].segments[0].source;
        // two real codes, and every look-alike is text
        assert_eq!(src.iter().filter(|i| matches!(i, Inline::Tag { .. })).count(), 3, "{id}: {src:?}");
        let out = set_inlines(id, x.as_bytes(), |s| s.to_vec());
        let want = if id == "xliff" { format!("<target>{LOOK}</target>") } else { format!("<target><mrk mtype=\"seg\" mid=\"1\">{LOOK}</mrk></target>") };
        assert!(out.contains(&want), "{id}: {out}");
    }
}

#[test]
fn look_alikes_in_a_translation_stay_text_when_codes_move() {
    for (id, x) in look_files() {
        let out = set_inlines(id, x.as_bytes(), |s| {
            vec![
                tag_of(s, "2", TagKind::Open),
                Inline::text("gras"),
                tag_of(s, "2", TagKind::Close),
                Inline::text(" \u{27e6}x1\u{27e7} "),
                tag_of(s, "1", TagKind::Standalone),
                Inline::text(" utiliser \u{27e6}g2\u{27e7} et \u{27e6}/g2\u{27e7} \u{e000}\u{e001}\u{27e6}7\u{27e7} \u{27e6}p1\u{27e7}"),
            ]
        });
        let body = "<g id=\"b\">gras</g> \u{27e6}x1\u{27e7} <x id=\"a\"/> utiliser \u{27e6}g2\u{27e7} et \u{27e6}/g2\u{27e7} \u{e000}\u{e001}\u{27e6}7\u{27e7} \u{27e6}p1\u{27e7}";
        let want = if id == "xliff" { format!("<target>{body}</target>") } else { format!("<target><mrk mtype=\"seg\" mid=\"1\">{body}</mrk></target>") };
        assert!(out.contains(&want), "{id}: {out}");
        assert!(dom::parse(&out).is_ok(), "{id}: {out}");
    }
}

#[test]
fn mxliff_look_alikes_beside_text_marks() {
    let x = "<xliff xmlns:m=\"http://www.memsource.com/mxlf/2.0\" version=\"1.2\"><file><body><trans-unit id=\"1\"><source>Click {1&gt;Save&lt;1} \u{27e6}g1\u{27e7}\u{27e6}/g1\u{27e7} {2} \u{27e6}x2\u{27e7}.</source></trans-unit></body></file></xliff>";
    let out = set_inlines("mxliff", x.as_bytes(), |s| {
        vec![
            tag_of(s, "2", TagKind::Standalone),
            Inline::text(" \u{27e6}x2\u{27e7} \u{27e6}g1\u{27e7} "),
            tag_of(s, "1", TagKind::Open),
            Inline::text("Sichern"),
            tag_of(s, "1", TagKind::Close),
            Inline::text("\u{27e6}/g1\u{27e7}."),
        ]
    });
    assert!(out.contains("<target>{2} \u{27e6}x2\u{27e7} \u{27e6}g1\u{27e7} {1>Sichern&lt;1}\u{27e6}/g1\u{27e7}.</target>"), "{out}");
}

#[test]
fn public_writers_still_read_plain_locos_text() {
    // `target_text`, `written_texts` and `build_cat` keep their 0.9.0 behaviour: a token in the text is a code,
    // and U+E000 / U+E001 are written as they are
    let t = vec![Inline::text("\u{27e6}x1\u{27e7} \u{e000}\u{e001}"), Inline::tag("2", TagKind::Open, ""), Inline::tag("2", TagKind::Close, "")];
    assert_eq!(target_text(&t), "\u{27e6}x1\u{27e7} \u{e000}\u{e001}\u{27e6}g2\u{27e7}\u{27e6}/g2\u{27e7}");
    assert_eq!(target_text_escaped(&t), "\u{e000}\u{e001}x1\u{27e7} \u{e000}\u{e000}\u{e001}\u{27e6}g2\u{27e7}\u{27e6}/g2\u{27e7}");
    let (_, x) = look_files().remove(1);
    let texts: HashMap<i64, String> = [(SEG_BASE, "\u{27e6}g2\u{27e7}fett\u{27e6}/g2\u{27e7} \u{27e6}x1\u{27e7} \u{e000}\u{e001}".to_string())].into();
    let out = String::from_utf8(build_cat(x.as_bytes(), &texts, None).unwrap()).unwrap();
    assert!(out.contains("<mrk mtype=\"seg\" mid=\"1\"><g id=\"b\">fett</g> <x id=\"a\"/> \u{e000}\u{e001}</mrk></target>"), "{out}");
    let doc = filter("sdlxliff").extract(x.as_bytes(), &opts(None)).unwrap();
    let mut doc2 = doc.clone();
    let seg = doc.units[0].segments[0].clone();
    profile::locos_set_target(&mut doc2, &seg.id, vec![Inline::text("\u{27e6}x1\u{27e7}")]).unwrap();
    assert_eq!(written_texts(doc2.units.iter())[&SEG_BASE], "\u{27e6}x1\u{27e7}");
    assert_eq!(written_texts_escaped(doc2.units.iter())[&SEG_BASE], "\u{e000}\u{e001}x1\u{27e7}");
}

// ------------------------------------------------------------- resource limits (security review run 2)

fn timed<T>(what: &str, f: impl FnOnce() -> T) -> T {
    let start = std::time::Instant::now();
    let r = f();
    assert!(start.elapsed() < std::time::Duration::from_secs(10), "{what} took {:?}", start.elapsed());
    r
}

/// Every `<ph dataRef>` looked for its `<data>` by walking its whole unit, lower-casing each name: quadratic in the
/// unit. The data elements are now indexed once per file, and the one a placeholder names is still found.
#[test]
fn many_dataref_points_are_linear() {
    let p = 30_000;
    let phs: String = (0..p).map(|k| format!("<ph id=\"{k}\" dataRef=\"z\"/>")).collect();
    let x = format!("<xliff version=\"2.0\" xmlns=\"urn:oasis:names:tc:xliff:document:2.0\" srcLang=\"en\"><file id=\"f\"><unit id=\"u\"><segment><source>a{phs}</source></segment></unit></file></xliff>");
    let doc = timed("extract", || filter("xliff").extract(x.as_bytes(), &opts(None)).unwrap());
    assert_eq!(doc.units.len(), 1);
    // a dataRef still finds its data: markup it stands for makes the point not free
    let x = "<xliff version=\"2.0\" xmlns=\"urn:oasis:names:tc:xliff:document:2.0\" srcLang=\"en\"><file id=\"f\"><unit id=\"u\"><originalData><data id=\"d1\">&lt;b&gt;</data><data id=\"d2\">&lt;br/&gt;</data></originalData><segment><source>a<ph id=\"1\" dataRef=\"d1\"/>b<ph id=\"2\" dataRef=\"d2\"/>c</source></segment></unit></file></xliff>";
    let doc = filter("xliff").extract(x.as_bytes(), &opts(None)).unwrap();
    assert_eq!(texts(&doc)[0].1, "a\u{27e6}x1\u{27e7}b\u{27e6}p2\u{27e7}c");
}

/// A file whose text starts with a second byte order mark (UTF-8 or UTF-16) is refused with an error: the parser
/// used to skip it without counting it, so a span fell inside a character and slicing there panicked.
#[test]
fn double_bom_is_refused_not_panicked() {
    let body = "<xliff version=\"1.2\"><file><body><trans-unit id=\"1\"><source>a\u{1F600}<x id=\"1\"/></source></trans-unit></body></file></xliff>";
    let utf8 = [&[0xEF, 0xBB, 0xBF, 0xEF, 0xBB, 0xBF][..], body.as_bytes()].concat();
    let mut utf16 = vec![0xFF, 0xFE, 0xFF, 0xFE];
    body.encode_utf16().for_each(|u| utf16.extend_from_slice(&u.to_le_bytes()));
    for bytes in [utf8, utf16] {
        let r = std::thread::Builder::new()
            .stack_size(256 << 10)
            .spawn(move || filter("xliff").extract(&bytes, &opts(None)).map(|d| d.units.len()))
            .unwrap()
            .join()
            .expect("no panic");
        assert!(r.is_err(), "{r:?}");
    }
    // one mark is fine
    let one = [&[0xEF, 0xBB, 0xBF][..], body.as_bytes()].concat();
    assert_eq!(filter("xliff").extract(&one, &opts(None)).unwrap().units.len(), 1);
}

/// MXLIFF text marks: every close looked for its open by scanning all the marks still open, so many unmatched
/// opens followed by closes of another id were quadratic. Pairing still drops the opens above a matched one.
#[test]
fn many_unmatched_text_marks_are_linear() {
    let n = 45_000;
    let src = format!("{}{}", "{a>".repeat(n), "&lt;b}".repeat(n));
    let x = format!("<xliff version=\"1.2\" xmlns:m=\"http://www.memsource.com/mxlf/2.0\"><file><body><trans-unit id=\"1\"><source>x{src}</source></trans-unit></body></file></xliff>");
    let doc = timed("extract", || filter("mxliff").extract(x.as_bytes(), &opts(None)).unwrap());
    assert_eq!(doc.units.len(), 1);
    // {1> {2> <1} pairs 1 and leaves 2 a point; a later <2} has no open left
    let mut codes = Vec::new();
    let toks = text_tags(vec![Tok::Text("{1>a{2>b<1}c<2}".into())], &mut codes);
    let kinds: Vec<&str> = toks
        .iter()
        .map(|t| match t {
            Tok::Text(_) => "t",
            Tok::Open(_) => "o",
            Tok::Close(_) => "c",
            Tok::Point(..) => "p",
        })
        .collect();
    assert_eq!(kinds, ["o", "t", "p", "t", "c", "t", "p"]);
}

/// A translated target with many pair tokens that name no code (phantom opens) and text between them: the text's
/// container was searched for over the whole stack of opens for every piece. It is now read off the top.
#[test]
fn many_phantom_opens_in_a_target_are_linear() {
    let mut d = dom::parse("<xliff><target/></xliff>").unwrap();
    let tgt = d.child(d.root, "target").unwrap();
    let text = "\u{27e6}g9\u{27e7}a".repeat(80_000);
    timed("rebuild", || rebuild_target(&mut d, tgt, &text, &[]));
    assert_eq!(d.itertext(tgt), "a".repeat(80_000));
}

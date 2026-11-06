//! Bilingual files through the Filter interface: made-up files in Arabic and Japanese, the vendor dialects, the
//! GlobalLink status, free placeholders, and byte-faithful writing.

use bilingua_model::{plain_text, ExtractOptions, Filter, Inline, OriginKind, SegState, TagKind};
use bilingua_xliff::{filters, Doc, Format};

fn opts() -> ExtractOptions {
    ExtractOptions::new().with_granularity(bilingua_model::Granularity::Paragraph).with_include_locked(true)
}

fn filter(id: &str) -> Box<dyn Filter> {
    filters().into_iter().find(|f| f.id() == id).unwrap()
}

fn tgt(d: &bilingua_model::Document, seg: &str) -> String {
    plain_text(&d.find(seg).unwrap().1.target)
}

const AR_12: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\">\n  <file original=\"menu.txt\" source-language=\"en\" target-language=\"ar\" datatype=\"plaintext\">\n    <body>\n      <trans-unit id=\"u1\">\n        <source>Click <g id=\"1\">Save</g> now.</source>\n        <target state=\"translated\" state-qualifier=\"exact-match\">\u{627}\u{646}\u{642}\u{631} <g id=\"1\">\u{62D}\u{641}\u{638}</g> \u{627}\u{644}\u{622}\u{646}.</target>\n        <note from=\"ana\" annotates=\"source\">Menu item</note>\n      </trans-unit>\n      <trans-unit id=\"u2\">\n        <source>Hello <x id=\"2\" equiv-text=\"{name}\"/>!</source>\n      </trans-unit>\n      <trans-unit id=\"u3\" translate=\"no\">\n        <source>Brand</source>\n      </trans-unit>\n    </body>\n  </file>\n</xliff>\n";

#[test]
fn arabic_xliff12_reads_states_notes_origin_locks_and_free_codes() {
    let d = filter("xliff").extract(AR_12.as_bytes(), &opts()).unwrap();
    assert_eq!(d.target_lang.as_deref(), Some("ar"));
    assert_eq!(d.units.len(), 3);
    let (u1, s1) = d.find("u1").unwrap();
    assert_eq!(s1.state, SegState::Translated);
    assert!(plain_text(&s1.target).starts_with("\u{627}\u{646}\u{642}\u{631}"));
    let o = s1.origin.as_ref().unwrap();
    assert_eq!((o.kind, o.match_pct), (OriginKind::Tm, Some(100)));
    assert_eq!(u1.notes[0].text, "Menu item");
    assert_eq!(u1.notes[0].author.as_deref(), Some("ana"));
    assert!(u1.codes.contains_key("1"));
    let (u2, s2) = d.find("u2").unwrap();
    assert_eq!(s2.state, SegState::New);
    assert!(s2.source.iter().any(|i| matches!(i, Inline::Tag { id, free: true, .. } if id == "2")));
    assert!(u2.translate);
    let (u3, _) = d.find("u3").unwrap();
    assert!(!u3.translate);
    assert_eq!(u3.lock_reason.as_deref(), Some("translate=no"));
    // Not split into sentences, ids are the file's.
    assert_eq!(d.segment_count(), 3);
}

#[test]
fn untouched_merge_is_byte_identical_and_edits_patch_only_the_target() {
    let f = filter("xliff");
    let d = f.extract(AR_12.as_bytes(), &opts()).unwrap();
    assert_eq!(f.merge(&d).unwrap(), AR_12.as_bytes());
    let mut d = d;
    let src = d.find("u2").unwrap().1.source.clone();
    let mut t = vec![Inline::text("\u{645}\u{631}\u{62D}\u{628}\u{627} ")];
    t.extend(src.iter().filter(|i| matches!(i, Inline::Tag { .. })).cloned());
    t.push(Inline::text("!"));
    d.set_target("u2", t).unwrap();
    d.set_state("u2", SegState::Translated).unwrap();
    let out = String::from_utf8(f.merge(&d).unwrap()).unwrap();
    assert!(out.contains("<target state=\"translated\">\u{645}\u{631}\u{62D}\u{628}\u{627} <x id=\"2\" equiv-text=\"{name}\"/>!</target>"), "{out}");
    // Everything else is untouched.
    assert!(out.starts_with(&AR_12[..AR_12.find("<trans-unit id=\"u2\">").unwrap()]));
    let d2 = f.extract(out.as_bytes(), &opts()).unwrap();
    assert_eq!(tgt(&d2, "u2"), "\u{645}\u{631}\u{62D}\u{628}\u{627} !");
    assert_eq!(d2.find("u2").unwrap().1.state, SegState::Translated);
    // A locked unit cannot be set.
    let mut d3 = d2;
    assert!(d3.set_target("u3", vec![Inline::text("x")]).is_err());
}

const JA_SDL: &str = "\u{FEFF}<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\" xmlns:sdl=\"http://sdl.com/FileTypes/SdlXliff/1.0\" sdl:version=\"1.0\">\r\n<file original=\"a.docx\" source-language=\"en-US\" target-language=\"ja-JP\" datatype=\"x-sdlfilterframework2\">\r\n<body>\r\n<trans-unit id=\"t1\">\r\n<source>Open the file. Close it.</source>\r\n<seg-source><mrk mtype=\"seg\" mid=\"1\">Open the file.</mrk> <mrk mtype=\"seg\" mid=\"2\">Close it.</mrk></seg-source>\r\n<target><mrk mtype=\"seg\" mid=\"1\">\u{30D5}\u{30A1}\u{30A4}\u{30EB}\u{3092}\u{958B}\u{304F}\u{3002}</mrk> <mrk mtype=\"seg\" mid=\"2\"></mrk></target>\r\n<sdl:seg-defs><sdl:seg id=\"1\" conf=\"ApprovedSignOff\" origin=\"tm\" origin-system=\"Acme TM\" percent=\"87\"/><sdl:seg id=\"2\" locked=\"true\"/></sdl:seg-defs>\r\n</trans-unit>\r\n</body>\r\n</file>\r\n</xliff>\r\n";

#[test]
fn japanese_sdlxliff_with_bom_reads_conf_origin_and_lock_and_writes_back() {
    let f = filter("sdlxliff");
    let d = f.extract(JA_SDL.as_bytes(), &opts()).unwrap();
    assert_eq!(d.units.len(), 1);
    assert_eq!(d.units[0].segments.len(), 2);
    let (_, s1) = d.find("1").unwrap();
    assert_eq!(s1.state, SegState::SignedOff);
    let o = s1.origin.as_ref().unwrap();
    assert_eq!((o.kind, o.name.as_deref(), o.match_pct), (OriginKind::Tm, Some("Acme TM"), Some(87)));
    assert_eq!(f.merge(&d).unwrap(), JA_SDL.as_bytes());
    // The file locks segment 2 at the segment level, which the unit shows as locked.
    assert!(!d.units[0].translate);
    // Unlocked file: edit segment 1 through the open handle path.
    let mut doc = Doc::from_bytes(JA_SDL.as_bytes().to_vec()).unwrap();
    assert_eq!(doc.format(), Format::Sdlxliff);
    assert!(doc.seg(1).locked);
    assert!(!doc.put_target(1, vec![Inline::text("x")]));
    doc.put_target(0, vec![Inline::text("\u{958B}\u{3044}\u{3066}\u{304F}\u{3060}\u{3055}\u{3044}\u{3002}")]);
    doc.put_state(0, SegState::Translated);
    let out = String::from_utf8(doc.to_bytes()).unwrap();
    assert!(out.starts_with('\u{FEFF}'));
    assert!(out.contains("conf=\"Translated\""));
    assert!(out.contains("origin=\"interactive\""));
    assert!(out.contains("<sdl:prev-origin"), "a human edit of a TM match records the old origin: {out}");
    assert!(out.contains("\u{958B}\u{3044}\u{3066}\u{304F}\u{3060}\u{3055}\u{3044}\u{3002}"));
}

const JA_X2: &str = "<?xml version=\"1.0\"?>\n<xliff xmlns=\"urn:oasis:names:tc:xliff:document:2.0\" version=\"2.1\" srcLang=\"en\" trgLang=\"ja\">\n<file id=\"f1\">\n<unit id=\"a\">\n<notes><note category=\"developer\">Shown on the dashboard</note></notes>\n<originalData><data id=\"d1\">%s</data><data id=\"d2\">{count}</data><data id=\"d3\">&lt;b&gt;</data></originalData>\n<segment state=\"reviewed\"><source>Hi <ph id=\"1\" dataRef=\"d1\"/> and <ph id=\"2\" dataRef=\"d2\"/> <ph id=\"3\" dataRef=\"d3\"/>x</source><target>\u{3053}\u{3093}\u{306B}\u{3061}\u{306F}</target></segment>\n</unit>\n<unit id=\"b\"><segment><source>One.</source></segment><segment><source>Two.</source></segment></unit>\n</file>\n</xliff>\n";

#[test]
fn japanese_xliff2_free_codes_notes_and_ids() {
    let f = filter("xliff");
    let d = f.extract(JA_X2.as_bytes(), &opts()).unwrap();
    assert_eq!(d.source_lang.as_deref(), Some("en"));
    let (ua, sa) = d.find("a").unwrap();
    assert_eq!(sa.state, SegState::Reviewed);
    assert_eq!(ua.notes[0].kind.as_deref(), Some("developer"));
    // %s is order bound, {count} names itself, "<b>" is markup.
    let free: Vec<(&str, bool)> = sa
        .source
        .iter()
        .filter_map(|i| match i {
            Inline::Tag { id, free, .. } => Some((id.as_str(), *free)),
            _ => None,
        })
        .collect();
    assert_eq!(free, vec![("1", false), ("2", true), ("3", false)]);
    assert!(d.find("b#1").is_some() && d.find("b#2").is_some());
    assert_eq!(f.merge(&d).unwrap(), JA_X2.as_bytes());
    let mut d = d;
    d.set_target("b#1", vec![Inline::text("\u{4E00}\u{3002}")]).unwrap();
    d.set_state("b#1", SegState::Translated).unwrap();
    let out = String::from_utf8(f.merge(&d).unwrap()).unwrap();
    assert!(out.contains("<segment state=\"translated\"><source>One.</source><target>\u{4E00}\u{3002}</target></segment>"), "{out}");
}

#[test]
fn memoq_statuses_machine_origin_and_lock() {
    let x = "<?xml version=\"1.0\"?>\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\" xmlns:mq=\"MQXliff\">\n<file source-language=\"en\" target-language=\"de\" datatype=\"x\"><body>\n<trans-unit id=\"1\" mq:status=\"MachineTranslated\"><source>A</source><target>B</target></trans-unit>\n<trans-unit id=\"2\" mq:status=\"Proofread\" mq:locked=\"locked\"><source>C</source><target>D</target></trans-unit>\n<trans-unit id=\"3\" mq:status=\"PreTranslated\" mq:percent=\"92\"><source>E</source><target>F</target></trans-unit>\n</body></file></xliff>\n";
    let f = filter("mqxliff");
    let d = f.extract(x.as_bytes(), &opts()).unwrap();
    assert_eq!(d.find("1").unwrap().1.origin.as_ref().unwrap().kind, OriginKind::Mt);
    assert_eq!(d.find("1").unwrap().1.state, SegState::Draft);
    assert_eq!(d.find("2").unwrap().1.state, SegState::Reviewed);
    assert!(!d.find("2").unwrap().0.translate);
    assert_eq!(d.find("3").unwrap().1.origin.as_ref().unwrap().match_pct, Some(92));
    assert_eq!(f.merge(&d).unwrap(), x.as_bytes());
    let mut d = d;
    d.set_state("1", SegState::Confirmed).unwrap();
    let out = String::from_utf8(f.merge(&d).unwrap()).unwrap();
    assert!(out.contains("<trans-unit id=\"1\" mq:status=\"Reviewer1Confirmed\">"));
}

const GLOBALLINK: &str = "<?xml version=\"1.0\"?>\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\" xmlns:iws=\"http://www.idiominc.com/ws/asset\">\n<file source-language=\"en\" target-language=\"ar\" datatype=\"x\"><body>\n<trans-unit id=\"g1\"><source>One</source><target>\u{648}\u{627}\u{62D}\u{62F}</target><iws:segment-metadata><iws:status translation_status=\"finished\" lock_status=\"unlocked\"/></iws:segment-metadata></trans-unit>\n<trans-unit id=\"g2\"><source>Two</source><target>\u{627}\u{62B}\u{646}\u{627}\u{646}</target><iws:segment-metadata><iws:status translation_status=\"in_progress\" lock_status=\"locked\"/></iws:segment-metadata></trans-unit>\n<trans-unit id=\"g3\"><source>Three</source><iws:segment-metadata><iws:status translation_status=\"in_progress\"/></iws:segment-metadata></trans-unit>\n</body></file></xliff>\n";

#[test]
fn globallink_finished_and_locked_segments() {
    let f = filter("xliff");
    let d = f.extract(GLOBALLINK.as_bytes(), &opts()).unwrap();
    // Finished work counts as translated, a locked segment is locked, an open one is new.
    assert_eq!(d.find("g1").unwrap().1.state, SegState::Translated);
    assert!(d.find("g1").unwrap().0.translate);
    assert!(!d.find("g2").unwrap().0.translate);
    assert_eq!(d.find("g2").unwrap().0.lock_reason.as_deref(), Some("locked in WorldServer (iws:status)"));
    assert_eq!(d.find("g3").unwrap().1.state, SegState::New);
    assert_eq!(f.merge(&d).unwrap(), GLOBALLINK.as_bytes());
    // Translating g3 and accepting it marks the status finished.
    let mut d = d;
    d.set_target("g3", vec![Inline::text("\u{62B}\u{644}\u{627}\u{62B}\u{629}")]).unwrap();
    d.set_state("g3", SegState::Translated).unwrap();
    let out = String::from_utf8(f.merge(&d).unwrap()).unwrap();
    assert!(out.contains("<iws:status translation_status=\"finished\"/>"), "{out}");
    assert!(out.contains("<iws:status translation_status=\"finished\" lock_status=\"unlocked\"/>"));
    assert!(out.contains("translation_status=\"in_progress\" lock_status=\"locked\""));
}

#[test]
fn free_rule_follows_the_localization_os_rule() {
    let x = "<?xml version=\"1.0\"?>\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\"><file source-language=\"en\" target-language=\"fr\" datatype=\"x\"><body><trans-unit id=\"1\"><source><x id=\"1\"/><x id=\"START_BOLD_TEXT\"/><x id=\"3\" ctype=\"bold\"/><x id=\"4\" equiv-text=\"%d\"/><x id=\"5\" equiv-text=\"%1$d\"/><ph id=\"6\">{}</ph><ph id=\"7\">&lt;br/&gt;</ph><bpt id=\"8\">&lt;i&gt;</bpt>t<ept id=\"8\">&lt;/i&gt;</ept></source></trans-unit></body></file></xliff>";
    let d = filter("xliff").extract(x.as_bytes(), &opts()).unwrap();
    let got: Vec<(String, bool)> = d.units[0].segments[0]
        .source
        .iter()
        .filter_map(|i| match i {
            Inline::Tag { id, free, kind, .. } if *kind != TagKind::Close => Some((id.clone(), *free)),
            _ => None,
        })
        .collect();
    let want: Vec<(String, bool)> = [("1", true), ("START_BOLD_TEXT", false), ("3", false), ("4", false), ("5", true), ("6", false), ("7", true), ("8", false)]
        .iter()
        .map(|(a, b)| (a.to_string(), *b))
        .collect();
    assert_eq!(got, want);
}

#[test]
fn skeleton_is_stamped_and_a_foreign_one_is_refused() {
    let f = filter("xliff");
    let mut d = f.extract(AR_12.as_bytes(), &opts()).unwrap();
    assert_eq!(d.skeleton.parser().unwrap().name, "bilingua-xliff");
    if let bilingua_model::Skeleton::Text { parser, .. } = &mut d.skeleton {
        *parser = Some(bilingua_model::ParserInfo::new("bilingua-po", "0.1.0", 1));
    }
    assert!(f.merge(&d).is_err());
}

#[test]
fn not_utf8_or_not_xliff_is_refused() {
    let f = filter("xliff");
    assert!(f.extract(b"hello", &opts()).is_err());
    let mut bad = AR_12.as_bytes().to_vec();
    bad.push(0xFF);
    assert!(f.extract(&bad, &opts()).is_err());
}

#[test]
fn a_unit_with_two_seg_sources_merges_without_panicking() {
    // Two `seg-source` elements in one unit give segment ranges that run backwards from the last one; the merge
    // that builds the missing `<target>` used to slice them and panic.
    const TWO_SS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\">\n<file original=\"a.txt\" source-language=\"en\" target-language=\"de\" datatype=\"plaintext\">\n<body>\n<trans-unit id=\"t1\">\n<source>Hello.</source>\n<seg-source><mrk mtype=\"seg\" mid=\"1\">Hello.</mrk></seg-source><seg-source><mrk mtype=\"seg\" mid=\"2\">Hello.</mrk></seg-source>\n</trans-unit>\n</body>\n</file>\n</xliff>\n";
    for o in [ExtractOptions::new(), opts()] {
        let f = filter("xliff");
        let mut d = f.extract(TWO_SS.as_bytes(), &o).unwrap();
        assert_eq!(f.merge(&d).unwrap(), TWO_SS.as_bytes());
        let ids: Vec<String> = d.units.iter().flat_map(|u| u.segments.iter().map(|s| s.id.clone())).collect();
        assert!(!ids.is_empty());
        for id in &ids {
            d.set_target(id, vec![Inline::text("Hallo.")]).unwrap();
        }
        let out = f.merge(&d).unwrap();
        assert!(String::from_utf8(out.clone()).unwrap().contains("<target>"));
        assert!(f.extract(&out, &o).is_ok());
    }
}

// A Trados SDLXLIFF as Studio writes it: GUID trans-unit ids, numeric `mrk mid`s, one unit with two segments.
const SDL_GUID: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?><xliff xmlns:sdl=\"http://sdl.com/FileTypes/SdlXliff/1.0\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\" version=\"1.2\" sdl:version=\"1.0\">\n<file original=\"a.docx\" datatype=\"x-sdlfilterframework2\" source-language=\"en-US\" target-language=\"de-DE\"><body>\n<trans-unit id=\"0f4a8c2e-1b3d-4e5f-8a6b-7c8d9e0f1a2b\"><source>Save.</source><seg-source><mrk mtype=\"seg\" mid=\"1\">Save.</mrk></seg-source><target><mrk mtype=\"seg\" mid=\"1\"></mrk></target><sdl:seg-defs><sdl:seg id=\"1\"/></sdl:seg-defs></trans-unit>\n<trans-unit id=\"9e8d7c6b-5a4f-4e3d-8c2b-1a0f9e8d7c6b\"><source>Open it. Then save.</source><seg-source><mrk mtype=\"seg\" mid=\"2\">Open it.</mrk> <mrk mtype=\"seg\" mid=\"3\">Then save.</mrk></seg-source><target><mrk mtype=\"seg\" mid=\"2\"></mrk> <mrk mtype=\"seg\" mid=\"3\"></mrk></target><sdl:seg-defs><sdl:seg id=\"2\"/><sdl:seg id=\"3\"/></sdl:seg-defs></trans-unit>\n<trans-unit id=\"2\"><source>Close.</source><seg-source><mrk mtype=\"seg\" mid=\"4\">Close.</mrk></seg-source><target><mrk mtype=\"seg\" mid=\"4\"></mrk></target><sdl:seg-defs><sdl:seg id=\"4\"/></sdl:seg-defs></trans-unit>\n</body></file></xliff>";

#[test]
fn sdlxliff_single_segment_units_are_named_by_their_guid_unit_id() {
    let f = filter("sdlxliff");
    let mut d = f.extract(SDL_GUID.as_bytes(), &opts()).unwrap();
    let ids: Vec<(String, Vec<String>)> = d.units.iter().map(|u| (u.id.clone(), u.segments.iter().map(|s| s.id.clone()).collect())).collect();
    // one segment: the unit id; two segments: the file's own mids; a unit id that is also a mid elsewhere ("2")
    // takes a suffix so every segment id stays unique
    assert_eq!(ids, vec![
        ("0f4a8c2e-1b3d-4e5f-8a6b-7c8d9e0f1a2b".to_string(), vec!["0f4a8c2e-1b3d-4e5f-8a6b-7c8d9e0f1a2b".to_string()]),
        ("9e8d7c6b-5a4f-4e3d-8c2b-1a0f9e8d7c6b".to_string(), vec!["2".to_string(), "3".to_string()]),
        ("2".to_string(), vec!["2~2".to_string()]),
    ]);
    assert_eq!(f.merge(&d).unwrap(), SDL_GUID.as_bytes());
    d.set_target("0f4a8c2e-1b3d-4e5f-8a6b-7c8d9e0f1a2b", vec![Inline::text("Speichern.")]).unwrap();
    d.set_target("3", vec![Inline::text("Dann speichern.")]).unwrap();
    d.set_target("2~2", vec![Inline::text("Schliessen.")]).unwrap();
    let out = String::from_utf8(f.merge(&d).unwrap()).unwrap();
    assert!(out.contains("<target><mrk mtype=\"seg\" mid=\"1\">Speichern.</mrk></target>"), "{out}");
    assert!(out.contains("<mrk mtype=\"seg\" mid=\"2\"></mrk> <mrk mtype=\"seg\" mid=\"3\">Dann speichern.</mrk>"), "{out}");
    assert!(out.contains("<target><mrk mtype=\"seg\" mid=\"4\">Schliessen.</mrk></target>"), "{out}");
    let back = f.extract(out.as_bytes(), &opts()).unwrap();
    assert_eq!(tgt(&back, "0f4a8c2e-1b3d-4e5f-8a6b-7c8d9e0f1a2b"), "Speichern.");
    assert_eq!(tgt(&back, "2~2"), "Schliessen.");
}

#[test]
fn a_schema_1_sdlxliff_document_still_merges_by_mid() {
    // A document stored before skeleton schema 2 named every SDLXLIFF segment by its mid.
    let f = filter("sdlxliff");
    let mut d = f.extract(SDL_GUID.as_bytes(), &opts()).unwrap();
    match &mut d.skeleton {
        bilingua_model::Skeleton::Text { parser, .. } => parser.as_mut().unwrap().schema = 1,
        _ => unreachable!(),
    }
    let old = ["1", "2", "3", "4"];
    let mut k = 0;
    for u in &mut d.units {
        for s in &mut u.segments {
            s.id = old[k].to_string();
            k += 1;
        }
    }
    d.set_target("1", vec![Inline::text("Speichern.")]).unwrap();
    d.set_target("4", vec![Inline::text("Schliessen.")]).unwrap();
    let out = String::from_utf8(f.merge(&d).unwrap()).unwrap();
    assert!(out.contains("mid=\"1\">Speichern.</mrk>") && out.contains("mid=\"4\">Schliessen.</mrk>"), "{out}");
}

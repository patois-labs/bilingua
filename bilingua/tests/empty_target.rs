//! The empty-translation rule: an empty target is untranslated (the source is written) unless the segment is marked
//! translated to nothing (`Document::set_target_empty`); then the writer writes an empty value.

use bilingua::*;

fn opts() -> ExtractOptions {
    ExtractOptions::new().with_source_lang("en").with_target_lang("de").with_granularity(Granularity::Paragraph)
}

fn first_id(d: &Document) -> String {
    d.units[0].segments[0].id.clone()
}

#[test]
fn an_empty_translation_is_written_empty() {
    for (name, src, want, keep) in [
        ("a.json", "{\"a\": \"Hello\", \"b\": \"World\"}", "\"a\": \"\"", "World"),
        ("a.properties", "a=Hello\nb=World\n", "a=\n", "World"),
        ("a.html", "<html><body><p>Hello</p><p>World</p></body></html>", "<p></p>", "World"),
    ] {
        let mut d = extract(name, src.as_bytes(), &opts()).unwrap();
        // untranslated: the source is written
        assert_eq!(merge(&d).unwrap(), src.as_bytes(), "{name}");
        let id = first_id(&d);
        d.set_target_empty(&id).unwrap();
        let s = d.find(&id).unwrap().1;
        assert!(s.empty_target && s.has_target() && s.target.is_empty(), "{name}");
        let out = String::from_utf8(merge(&d).unwrap()).unwrap();
        assert!(out.contains(want) && out.contains(keep) && !out.contains("Hello"), "{name}: {out}");
        // the flag survives JSON, and a real target clears it
        let back: Document = serde_json::from_str(&serde_json::to_string(&d).unwrap()).unwrap();
        assert_eq!(back, d, "{name}");
        d.set_target(&id, vec![Inline::text("Hallo")]).unwrap();
        assert!(!d.find(&id).unwrap().1.empty_target, "{name}");
    }
}

#[test]
fn a_locked_unit_cannot_be_emptied() {
    let mut d = extract("a.json", b"{\"a\": \"Hello\"}", &opts()).unwrap();
    d.units[0].translate = false;
    let id = first_id(&d);
    assert!(matches!(d.set_target_empty(&id), Err(Error::Locked(_))));
    assert!(matches!(d.set_target_empty("nope"), Err(Error::UnknownSegment(_))));
}

/// rc, scc, stl and the subtitle text formats: a segment translated to nothing is written empty (it no longer
/// reads back), the others keep their text; a format that cannot hold an empty value says so with
/// `Error::Unwritable`, never with `Error::Write`.
#[test]
fn rc_scc_stl_and_subtext_write_an_empty_translation_or_refuse_it_as_unwritable() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus");
    let mut tried = 0;
    for fmt in ["rc", "scc", "stl", "subtext"] {
        let Ok(rd) = std::fs::read_dir(dir.join(fmt)) else { continue };
        for p in rd.flatten().map(|e| e.path()).filter(|p| p.is_file()) {
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            let bytes = std::fs::read(&p).unwrap();
            let Ok(mut d) = extract(&name, &bytes, &opts()) else { continue };
            let texts = |d: &Document| d.units.iter().filter(|u| u.translate).map(|u| plain_text(&u.source())).collect::<Vec<_>>();
            let before = texts(&d);
            let Some(u) = d.units.iter().find(|u| u.translate && !plain_text(&u.source()).trim().is_empty()) else { continue };
            let (uid, sid, text) = (u.id.clone(), u.segments[0].id.clone(), plain_text(&u.source()));
            for s in d.units.iter().find(|u| u.id == uid).unwrap().segments.iter().map(|s| s.id.clone()).collect::<Vec<_>>() {
                d.set_target_empty(&s).unwrap();
            }
            assert!(d.find(&sid).unwrap().1.has_target());
            tried += 1;
            match merge(&d) {
                Ok(out) => {
                    let back = extract(&name, &out, &opts()).unwrap_or_else(|e| panic!("{name}: the merged file does not read back: {e}"));
                    let after = texts(&back);
                    let n = |v: &[String]| v.iter().filter(|t| **t == text).count();
                    assert!(n(&after) < n(&before), "{name}: {text:?} is still there");
                    assert!(after.len() + 1 >= before.len(), "{name}: other units lost");
                }
                Err(Error::Unwritable { .. }) => {}
                Err(e) => panic!("{name}: {e:?}"),
            }
        }
    }
    assert!(tried >= 10, "only {tried} files tried");
}

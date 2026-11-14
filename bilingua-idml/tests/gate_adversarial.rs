//! Adversarial packages from the Opus gate: odd design maps, nested structures, alternating wrappers.
mod common;

use bilingua_idml::Idml;
use bilingua_model::*;
use common::*;
use std::time::Instant;

fn with_designmap(mut m: Vec<(String, Vec<u8>)>, refs: &[&str]) -> Vec<u8> {
    let refs: String = refs.iter().map(|s| format!("\t<idPkg:Story src=\"{s}\"/>\n")).collect();
    let dm = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Document xmlns:idPkg=\"{PKG_NS}\" DOMVersion=\"20.0\" Self=\"d\">\n\
<Languages><Language Self=\"L1\" Name=\"$ID/Arabic\"/><Language Self=\"L2\" Name=\"$ID/de_DE_2006\"/></Languages>\n{refs}</Document>\n"
    );
    for e in &mut m {
        if e.0 == "designmap.xml" {
            e.1 = dm.clone().into_bytes();
        }
    }
    zip_of(&m)
}

#[test]
fn a_story_referenced_twice_is_read_once_and_merges() {
    let pkg = Pkg::new(&[("a", psr(&csr("<Content>Hello.</Content>")))]);
    let data = with_designmap(pkg.members(), &["Stories/Story_a.xml", "Stories/Story_a.xml"]);
    let mut doc = extract(&data);
    assert_eq!(doc.units.len(), 1);
    translate(&mut doc, &[("p0", "Hallo.".into())]);
    let out = merge(&doc);
    assert!(raw_story(&out, "a").contains("Hallo."));
}

#[test]
fn a_missing_story_is_skipped() {
    let pkg = Pkg::new(&[("a", psr(&csr("<Content>Hello.</Content>")))]);
    let data = with_designmap(pkg.members(), &["Stories/Story_gone.xml", "Stories/Story_a.xml"]);
    let doc = extract(&data);
    assert_eq!(texts(&doc), vec!["Hello."]);
}

#[test]
fn story_paths_outside_stories_do_not_escape() {
    let pkg = Pkg::new(&[("a", psr(&csr("<Content>Hello.</Content>")))]);
    for bad in ["../Stories/Story_a.xml", "/Stories/Story_a.xml", "Stories/../designmap.xml", "Stories\\Story_a.xml", "C:/x.xml"] {
        let data = with_designmap(pkg.members(), &[bad, "Stories/Story_a.xml"]);
        let doc = Idml.extract(&data, &ExtractOptions::default());
        // either refused or only the real story is read
        if let Ok(doc) = doc {
            assert_eq!(texts(&doc), vec!["Hello."], "{bad}");
        }
    }
    // A design map that names a non-story member as a story: it is not translated as a story.
    let data = with_designmap(pkg.members(), &["Resources/Graphic.xml", "Stories/Story_a.xml"]);
    let doc = Idml.extract(&data, &ExtractOptions::default()).unwrap();
    assert_eq!(texts(&doc), vec!["Hello."]);
}

#[test]
fn nested_tables_extract_and_merge() {
    let inner = "<Table Self=\"t2\"><Cell Self=\"t2c\"><ParagraphStyleRange><CharacterStyleRange><Content>Inner cell.</Content></CharacterStyleRange></ParagraphStyleRange></Cell></Table>";
    let outer = format!(
        "<Content>Body.</Content><Table Self=\"t1\"><Cell Self=\"t1c\"><ParagraphStyleRange><CharacterStyleRange><Content>Outer cell.</Content>{inner}</CharacterStyleRange></ParagraphStyleRange></Cell></Table>"
    );
    let data = package(psr(&csr(&outer)));
    let mut doc = extract(&data);
    let t = texts(&doc);
    assert_eq!(t.len(), 3, "{t:?}");
    let pairs: Vec<(String, String)> = doc.units.iter().map(|u| (u.id.clone(), tok(u).to_uppercase().replace("\u{27E6}X", "\u{27E6}x"))).collect();
    let pairs: Vec<(&str, String)> = pairs.iter().map(|(a, b)| (a.as_str(), b.clone())).collect();
    translate(&mut doc, &pairs);
    let out = merge(&doc);
    let s = raw_story(&out, "a");
    assert!(s.contains("BODY.") && s.contains("OUTER CELL.") && s.contains("INNER CELL."), "{s}");
    assert_eq!(s.matches("<Table ").count(), 2);
    assert_eq!(units_of(&out).len(), 3);
}

#[test]
fn a_huge_processing_instruction_is_fine() {
    // pseudo-random letters, so the zip ratio guard does not refuse it as a bomb
    let mut x: u32 = 12345;
    let big: String = (0..2_000_000)
        .map(|_| {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12345);
            (b'a' + ((x >> 16) % 26) as u8) as char
        })
        .collect();
    let data = package(psr(&csr(&format!("<Content>Hello <?ACE {big}?>world.</Content>"))));
    let mut doc = extract(&data);
    assert_eq!(doc.units.len(), 1);
    translate(&mut doc, &[("p0", "Hallo \u{27E6}x1\u{27E7}Welt.".into())]);
    let out = merge(&doc);
    assert!(raw_story(&out, "a").contains(&big));
}

/// Alternating hyperlink sources and character ranges, nested: the stamping walk must stay linear.
#[test]
fn alternating_wrappers_and_ranges_stamp_in_time() {
    let depth = 120usize;
    let mut body = String::from("<Content>deep</Content>");
    for k in 0..depth {
        body = format!("<HyperlinkTextSource Self=\"h{k}\"><CharacterStyleRange AppliedCharacterStyle=\"{BOLD}\">{body}</CharacterStyleRange></HyperlinkTextSource>");
    }
    let body = format!("<Content>x </Content>{body}");
    let pkg = Pkg::one(psr(&csr(&body)));
    let data = with_designmap(pkg.members(), &["Stories/Story_a.xml"]);
    for lang in ["ar", "de-DE"] {
        let mut doc = extract_for(&data, lang);
        let src = doc.units[0].segments[0].source.clone();
        doc.units[0].segments[0].target = src.into_iter().map(|i| if let Inline::Text(t) = i { Inline::text(t.to_uppercase()) } else { i }).collect();
        let t0 = Instant::now();
        let out = merge(&doc);
        assert!(t0.elapsed().as_secs() < 5, "{lang}: {:?}", t0.elapsed());
        assert!(raw_story(&out, "a").contains("DEEP"));
    }
}

/// Gate oracle helper: GATE_DUMP_IN = file with one path per line, GATE_DUMP_OUT = JSON output.
#[test]
#[ignore]
fn gate_dump() {
    let list = std::fs::read_to_string(std::env::var("GATE_DUMP_IN").unwrap()).unwrap();
    let mut out = serde_json::Map::new();
    for path in list.lines().filter(|l| !l.trim().is_empty()) {
        let data = std::fs::read(path).unwrap();
        let v = match Idml.extract(&data, &ExtractOptions::default()) {
            Ok(doc) => {
                let units: Vec<serde_json::Value> = doc.units.iter().map(|u| serde_json::json!({"id": u.id, "text": tok(u)})).collect();
                let mut ok = true;
                // pseudo-translate everything and check the merge reads back with the same count
                let mut d2 = doc.clone();
                for u in &mut d2.units {
                    let src = u.segments[0].source.clone();
                    u.segments[0].target = src.into_iter().map(|i| if let Inline::Text(t) = i { Inline::text(format!("[{t}]")) } else { i }).collect();
                }
                let rt = match Idml.merge(&d2) {
                    Ok(b) => match Idml.extract(&b, &ExtractOptions::default()) {
                        Ok(d3) => format!("{}->{}", doc.units.len(), d3.units.len()),
                        Err(e) => { ok = false; format!("reextract error {e}") }
                    },
                    Err(e) => { ok = false; format!("merge error {e}") }
                };
                let same = Idml.merge(&doc).map(|b| b == data).unwrap_or(false);
                serde_json::json!({"units": units, "roundtrip": rt, "ok": ok, "identity": same})
            }
            Err(e) => serde_json::json!({"error": e.to_string()}),
        };
        out.insert(path.to_string(), v);
    }
    std::fs::write(std::env::var("GATE_DUMP_OUT").unwrap(), serde_json::to_string_pretty(&out).unwrap()).unwrap();
}

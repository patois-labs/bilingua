//! Adversarial packages from the Opus gate.

use crate::fixtures::*;
use crate::tests::*;
use crate::*;

fn page(body: &str) -> String {
    format!("<?xml version='1.0' encoding='utf-8' ?>\r\n<PageContents xmlns='{NS}' xmlns:r='{R}' xml:space='preserve'><Shapes>{body}</Shapes></PageContents>")
}

#[test]
fn shapes_nested_fifty_deep() {
    let mut body = String::new();
    for k in 0..50 {
        body.push_str(&format!("<Shape ID='{k}' Type='Group'><Section N='Character'><Row IX='0'><Cell N='LangID' V='1033'/></Row></Section><Text>Level {k}\n</Text><Shapes>"));
    }
    for _ in 0..50 {
        body.push_str("</Shapes></Shape>");
    }
    let src = Pkg { page1: page(&body), ..Default::default() }.build();
    let doc = extract(&src);
    let shape_units: Vec<&Unit> = doc.units.iter().filter(|u| u.id.starts_with("page1-")).collect();
    assert_eq!(shape_units.len(), 50);
    assert_eq!(text_of(&doc, "page1-50"), "Level 49");
    assert_same_entries(&Vsdx.merge(&doc).unwrap(), &src);
    let mut doc = extract_for(&src, Some("ar-SA"));
    for k in 1..=50 {
        set(&mut doc, &format!("page1-{k}"), &format!("مستوى {k}"));
    }
    let out = Vsdx.merge(&doc).unwrap();
    let p1 = part(&out, "visio/pages/page1.xml");
    assert_eq!(p1.matches("<Cell N='LangID' V='1025'/>").count(), 50, "{p1}");
    assert_eq!(p1.matches("<Section N='Paragraph'><Row IX='0'><Cell N='Flags' V='1'/></Row></Section>").count(), 50);
    let back = extract(&out);
    assert_eq!(back.units.len(), doc.units.len());
    assert_eq!(text_of(&back, "page1-50"), "مستوى 50");
}

#[test]
fn text_split_across_many_markers() {
    let mut t = String::new();
    for k in 0..500 {
        t.push_str(&format!("<cp IX='{}'/>w{k} ", k % 7));
    }
    let src = Pkg { page2: page(&format!("<Shape ID='7' Type='Shape'><Text>{t}\n</Text></Shape>")), ..Default::default() }.build();
    let doc = extract(&src);
    let u = doc.units.iter().find(|u| u.id == "page2-1").unwrap();
    let tags = u.source().iter().filter(|i| matches!(i, Inline::Tag { .. })).count();
    assert_eq!(tags, 499, "the first marker is before the text, outside the unit");
    assert_same_entries(&Vsdx.merge(&doc).unwrap(), &src);
    // a translation that drops every marker: they are appended, none lost
    let mut doc2 = doc.clone();
    doc2.set_target(&u.segments[0].id, vec![Inline::text("alles")]).unwrap();
    let p2 = part(&Vsdx.merge(&doc2).unwrap(), "visio/pages/page2.xml");
    assert_eq!(p2.matches("<cp IX=").count(), 500, "{p2}");
    // a translation that repeats one marker many times writes it once
    let mut doc3 = doc.clone();
    let mut tgt = u.source();
    let first = tgt.iter().find(|i| matches!(i, Inline::Tag { .. })).cloned().unwrap();
    for _ in 0..100 {
        tgt.push(first.clone());
    }
    assert!(doc3.set_target(&u.segments[0].id, tgt.clone()).is_err(), "the model refuses a doubled tag");
    let k = doc3.units.iter().position(|x| x.id == "page2-1").unwrap();
    doc3.units[k].segments[0].target = tgt; // a document edited by hand, past the model's check
    let p3 = part(&Vsdx.merge(&doc3).unwrap(), "visio/pages/page2.xml");
    assert_eq!(p3.matches("<cp IX=").count(), 500);
}

#[test]
fn page_part_referenced_twice() {
    let rels = pages_rels().replace("Target='page2.xml'", "Target='page1.xml'");
    let twice = pages().replace("<Rel r:id='rId2'/>", "<Rel r:id='rId1'/>");
    let mut parts = vec![
        ("[Content_Types].xml".to_string(), content_types(false).into_bytes()),
        ("visio/pages/pages.xml".into(), twice.into_bytes()),
        ("visio/pages/_rels/pages.xml.rels".into(), rels.into_bytes()),
        ("visio/pages/page1.xml".into(), page1().into_bytes()),
    ];
    let src = zip_of(parts.clone());
    let doc = extract(&src);
    let ids: Vec<&str> = doc.units.iter().map(|u| u.id.as_str()).collect();
    let mut dedup = ids.clone();
    dedup.sort();
    dedup.dedup();
    assert_eq!(ids.len(), dedup.len(), "{ids:?}");
    assert_eq!(ids.iter().filter(|i| i.starts_with("page1-")).count(), 4, "{ids:?}");
    assert_same_entries(&Vsdx.merge(&doc).unwrap(), &src);
    // the same Rel twice inside one Page
    parts[1].1 = pages().replace("<Rel r:id='rId2'/>", "<Rel r:id='rId1'/><Rel r:id='rId1'/>").into_bytes();
    let doc = extract(&zip_of(parts));
    assert_eq!(doc.units.iter().filter(|u| u.id.starts_with("page1-")).count(), 4);
}

#[test]
fn duplicate_page_member_is_refused() {
    let src = Pkg::default().build();
    let mut parts = members(&src).into_iter().map(|e| (e.name, e.data)).collect::<Vec<_>>();
    parts.push(("visio/pages/qage1.xml".into(), page2().into_bytes()));
    let z = zip_of(parts);
    // rename the extra member to a second visio/pages/page1.xml in both headers
    let mut v = z.clone();
    let (from, to) = (b"visio/pages/qage1.xml", b"visio/pages/page1.xml");
    let mut i = 0;
    while i + from.len() <= v.len() {
        if &v[i..i + from.len()] == from {
            v[i..i + from.len()].copy_from_slice(to);
        }
        i += 1;
    }
    refused(&v, "twice");
    // a truncated or garbled central directory is still an error, never a panic
    for cut in [v.len() - 1, v.len() - 10, v.len() - 30] {
        assert!(Vsdx.extract(&v[..cut], &ExtractOptions::default()).is_err());
    }
    let mut g = v.clone();
    let n = g.len();
    g[n - 6..n - 2].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
    assert!(Vsdx.extract(&g, &ExtractOptions::default()).is_err());
}

#[test]
fn crlf_text_keeps_marker_bytes() {
    let src = Pkg { page2: page("<Shape ID='7' Type='Shape'><Text>Start\r\nhere <fld IX='0'>a\r\nb</fld> end</Text></Shape>"), ..Default::default() }.build();
    let mut doc = extract(&src);
    assert_same_entries(&Vsdx.merge(&doc).unwrap(), &src);
    set(&mut doc, "page2-1", "Anfang\nhier ⟦x1⟧ Ende");
    let p2 = part(&Vsdx.merge(&doc).unwrap(), "visio/pages/page2.xml");
    assert!(p2.contains("<Text>Anfang\r\nhier <fld IX='0'>a\r\nb</fld> Ende</Text>"), "{p2}");
}

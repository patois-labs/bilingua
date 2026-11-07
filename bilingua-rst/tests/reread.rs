//! A translated file reads back with the same units (gate for bilingua#19): a widened table with a spanned header,
//! a footnote whose first paragraph sets the body indentation, and a letter enumerator that is not a list.
use bilingua_model::*;
use bilingua_rst::Rst;

fn doc(profile: Option<&str>, data: &str) -> Document {
    Rst.extract(data.as_bytes(), &ExtractOptions::new().with_source_lang("en").with_granularity(Granularity::Paragraph).with_profile(profile)).unwrap()
}

fn txt(u: &Unit) -> String {
    u.source().iter().map(|i| if let Inline::Text(t) = i { t.as_str() } else { "" }).collect()
}

fn texts(profile: Option<&str>, data: &str) -> Vec<String> {
    doc(profile, data).units.iter().map(txt).collect()
}

/// Every unit gets `Zq` + source + `qZ` (what bl-dump does); returns the merged text.
fn pseudo(profile: Option<&str>, data: &str) -> String {
    let mut d = doc(profile, data);
    for u in d.units.iter_mut() {
        let mut v = vec![Inline::text("Zq")];
        v.extend(u.source());
        v.push(Inline::text("qZ"));
        u.segments[0].target = v;
        u.segments[0].state = SegState::Draft;
    }
    let o = MergeOptions::new().with_profile(profile);
    String::from_utf8(Rst.merge_with(&d, &o).unwrap_or_else(|e| panic!("{e}"))).unwrap()
}

fn same_units_after_pseudo(data: &str) -> String {
    let before = texts(None, data);
    let out = pseudo(None, data);
    let after: Vec<String> = texts(None, &out).iter().map(|t| t.replace("Zq", "").replace("qZ", "")).collect();
    assert_eq!(before.len(), after.len(), "{out}");
    out
}

#[test]
fn widening_keeps_a_spanned_header_whole() {
    // `In here` spans the first two columns and crosses the end of the first one; it is code, so not translated
    let t = "=====  =====  =====\n``In here``   ``x``\n------------  -----\nyes    a      b\n=====  =====  =====\n";
    let mut d = doc(None, t);
    for u in d.units.iter_mut() {
        if txt(u) == "yes" {
            u.segments[0].target = vec![Inline::text("Si, claro")];
            u.segments[0].state = SegState::Draft;
        }
    }
    let out = String::from_utf8(Rst.merge_with(&d, &MergeOptions::new()).unwrap_or_else(|e| panic!("{e}"))).unwrap();
    assert!(out.contains("\n``In here``       ``x``\n"), "{out}");
    assert!(out.contains("Si, claro  a"), "{out}");
    assert!(out.starts_with("=========  =====  =====\n"), "{out}");
    assert!(out.contains("\n----------------  -----\n"), "{out}");
}

#[test]
fn a_footnote_paragraph_keeps_the_body_indentation() {
    let t = ".. [#note] First words of the note go on\n   over a second line, and then::\n\n     literal text (here)\n\nAfter.\n";
    let out = same_units_after_pseudo(t);
    assert!(out.starts_with(".. [#note]\n   Zq"), "{out}");
    assert!(out.contains("\n     literal text (here)\n"), "{out}");
    // the LocOS profile keeps the one line LocOS writes
    assert!(pseudo(Some("locos"), t).starts_with(".. [#note] Zq"));
}

#[test]
fn an_admonition_paragraph_keeps_the_body_indentation() {
    let t = ".. note:: Some words of the note that\n   go on over a line::\n\n      a literal block\n";
    let out = same_units_after_pseudo(t);
    assert!(out.starts_with(".. note::\n   Zq"), "{out}");
}

#[test]
fn a_one_line_footnote_stays_on_its_marker_line() {
    let t = ".. [1] One line only.\n\nAfter.\n";
    let out = same_units_after_pseudo(t);
    assert!(out.starts_with(".. [1] ZqOne line only.qZ"), "{out}");
}

#[test]
fn a_letter_enumerator_without_its_successor_is_not_a_list() {
    // docutils: `I.` needs `II.` (or `#.`) on the next line; `B.` is not it, so this is one paragraph
    let t = ".. [Ref] I. Writer, K. Other, N. Name,\n   B. Last, A Title of a Paper, 2015.\n";
    let t1 = texts(None, t);
    assert_eq!(t1.len(), 1, "{t1:?}");
    assert!(t1[0].starts_with("I. Writer") && t1[0].contains("B. Last"), "{t1:?}");
    same_units_after_pseudo(t);
    // a real letter list is still a list
    let l = "a. first item\nb. second item\n";
    assert_eq!(texts(None, l), vec!["first item", "second item"]);
    same_units_after_pseudo(l);
    // LocOS takes any enumerator line
    assert_eq!(texts(Some("locos"), t).len(), 2);
}

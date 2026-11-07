//! A simple table column that a translation outgrows is widened in the native profile (bilingua#19); the LocOS
//! profile keeps its refusal.
use bilingua_model::*;
use bilingua_rst::Rst;

const TABLE: &str = "=====  =====\nName   Value\n=====  =====\nyes    1\nno     22\n=====  =====\n";

fn doc(profile: Option<&str>, data: &str) -> Document {
    Rst.extract(data.as_bytes(), &ExtractOptions::new().with_source_lang("en").with_granularity(Granularity::Paragraph).with_profile(profile)).unwrap()
}

fn txt(u: &Unit) -> String {
    u.source().iter().map(|i| if let Inline::Text(t) = i { t.as_str() } else { "" }).collect()
}

fn merge(profile: Option<&str>, data: &str, set: &[(&str, &str)]) -> Result<String, Error> {
    let mut d = doc(profile, data);
    for u in d.units.iter_mut() {
        if let Some((_, to)) = set.iter().find(|(from, _)| txt(u) == *from) {
            u.segments[0].target = vec![Inline::text(*to)];
            u.segments[0].state = SegState::Draft;
        }
    }
    let o = MergeOptions::new().with_profile(profile);
    Rst.merge_with(&d, &o).map(|b| String::from_utf8(b).unwrap())
}

#[test]
fn first_column_grows() {
    let out = merge(None, TABLE, &[("yes", "Si, claro, siempre")]).unwrap();
    let want = "==================  =====\nName                Value\n==================  =====\nSi, claro, siempre  1\nno                  22\n==================  =====\n";
    assert_eq!(out, want);
    // the widened table reads back as the same table with the new text
    let d2 = Rst.extract(out.as_bytes(), &ExtractOptions::new()).unwrap();
    assert!(d2.units.iter().any(|u| txt(u) == "Si, claro, siempre"));
}

#[test]
fn locos_profile_still_refuses() {
    let e = merge(Some("locos"), TABLE, &[("yes", "Si, claro, siempre")]).unwrap_err();
    assert!(e.to_string().contains("does not fit"), "{e}");
}

#[test]
fn an_indented_table_and_a_later_column_are_redrawn() {
    let t = "Intro text.

  =====  =====  =====
  One    Two    Three
  =====  =====  =====
  yes    a      b
  no     c      d
  =====  =====  =====

After.
";
    let out = merge(None, t, &[("yes", "Si, claro"), ("a", "Palabra")]).unwrap();
    let want = "Intro text.

  =========  =======  =====
  One        Two      Three
  =========  =======  =====
  Si, claro  Palabra  b
  no         c        d
  =========  =======  =====

After.
";
    assert_eq!(out, want);
}

#[test]
fn a_row_that_fills_the_first_column_alone() {
    let t = "======  =====  =====\nfn      score  resp\n======  =====  =====\n**Head**\nmean    a b    c d\n======  =====  =====\n";
    let out = merge(None, t, &[("Head", "Zq\ **Headline words**\ qZ")]).unwrap_or_else(|e| panic!("{e}"));
    println!("{out}");
    let d2 = Rst.extract(out.as_bytes(), &ExtractOptions::new()).unwrap();
    let d1 = doc(None, t);
    assert_eq!(d1.units.len(), d2.units.len(), "{out}");
}

//! A segment in state `Locked` refuses a target even when its unit is open; its neighbours still take one.

use bilingua_model::*;

fn doc() -> Document {
    let segs = vec![
        Segment::new("s1", vec![Inline::text("One.")]),
        Segment::new("s2", vec![Inline::text("Two.")]).with_target(vec![Inline::text("Zwei.")]).with_state(SegState::Locked),
        Segment::new("s3", vec![Inline::text("Three.")]),
    ];
    Document::new("text", vec![Unit::from_segments("u1", segs)], Skeleton::default())
}

#[test]
fn locked_segment_in_an_open_unit_refuses_targets() {
    let mut d = doc();
    assert!(d.units[0].translate);
    assert!(matches!(d.set_target("s2", vec![Inline::text("Nein.")]), Err(Error::Locked(id)) if id == "s2"));
    assert!(matches!(d.set_target_repeating("s2", vec![Inline::text("Nein.")]), Err(Error::Locked(_))));
    assert!(matches!(d.set_target_empty("s2"), Err(Error::Locked(_))));
    assert!(matches!(d.set_targets([("s1", vec![Inline::text("Eins.")]), ("s2", vec![Inline::text("Nein.")])]), Err(Error::Locked(_))));
    let s2 = &d.units[0].segments[1];
    assert_eq!((s2.state, plain_text(&s2.target)), (SegState::Locked, "Zwei.".to_string()));
    // the open neighbours take targets (s1 was set before set_targets stopped)
    assert_eq!(plain_text(&d.units[0].segments[0].target), "Eins.");
    d.set_target("s3", vec![Inline::text("Drei.")]).unwrap();
    // unlocking it on purpose makes it editable again
    d.set_state("s2", SegState::Translated).unwrap();
    d.set_target("s2", vec![Inline::text("Zwo.")]).unwrap();
}

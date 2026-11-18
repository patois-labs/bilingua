//! Structure roles (unit model step 4): `h1`..`h6` with their level, `title`, `caption` and `figcaption`; none
//! under the LocOS profile.
use bilingua_html::Html;
use bilingua_model::*;

const PAGE: &str = "<!DOCTYPE html><html><head><title>Page title</title></head><body>\
<h1>Top heading</h1><p>Body text.</p><h2>Second</h2><h6>Sixth</h6>\
<table><caption>Table caption</caption><tr><td>Cell</td></tr></table>\
<figure><img src=\"a.png\" alt=\"A picture\"><figcaption>Figure caption</figcaption></figure>\
</body></html>";

fn rl(d: &Document, text: &str) -> (Option<Role>, Option<u8>) {
    let u = d.units.iter().find(|u| plain_text(&u.source()) == text).unwrap_or_else(|| panic!("no unit {text}"));
    (u.context.role(), u.context.level())
}

#[test]
fn headings_title_and_captions_have_roles() {
    let d = Html.extract(PAGE.as_bytes(), &ExtractOptions::default()).unwrap();
    assert_eq!(rl(&d, "Page title"), (Some(Role::Title), None));
    assert_eq!(rl(&d, "Top heading"), (Some(Role::Heading), Some(1)));
    assert_eq!(rl(&d, "Second"), (Some(Role::Heading), Some(2)));
    assert_eq!(rl(&d, "Sixth"), (Some(Role::Heading), Some(6)));
    assert_eq!(rl(&d, "Table caption"), (Some(Role::Caption), None));
    assert_eq!(rl(&d, "Figure caption"), (Some(Role::Caption), None));
    assert_eq!(rl(&d, "Body text."), (None, None));
    assert_eq!(rl(&d, "Cell"), (None, None));
    assert_eq!(rl(&d, "A picture"), (None, None));
    // what the crate filled before stays: kind and the nearest heading
    let body = d.units.iter().find(|u| plain_text(&u.source()) == "Body text.").unwrap();
    assert_eq!(body.context.kind(), Some("paragraph"));
    assert_eq!(body.context.heading(), Some("Top heading"));
    let h = d.units.iter().find(|u| plain_text(&u.source()) == "Second").unwrap();
    assert_eq!(h.context.kind(), Some("heading"));
    // an unchanged page merges to the same bytes
    assert_eq!(String::from_utf8(Html.merge(&d).unwrap()).unwrap(), PAGE);
}

#[test]
fn the_locos_profile_leaves_roles_unset() {
    let opts = ExtractOptions::default().with_profile(profile::LOCOS);
    let d = Html.extract(PAGE.as_bytes(), &opts).unwrap();
    assert!(!d.units.is_empty());
    assert!(d.units.iter().all(|u| u.context.role().is_none() && u.context.level().is_none()));
}

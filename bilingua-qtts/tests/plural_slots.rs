//! Unit model step 7: a numerus message is one unit (one source for every form), so it gets a plural group and no
//! category; no units are added (Qt numbers the forms by its own per-language rules), and nothing is filled under
//! the LocOS profile.

use bilingua_model::*;
use bilingua_qtts::QtTs;

const TS: &str = "<TS version=\"2.1\" language=\"pl\"><context><name>A</name><message numerus=\"yes\"><source>%n file(s)</source><translation type=\"unfinished\"/></message><message><source>Hello</source><translation type=\"unfinished\"/></message></context></TS>";

#[test]
fn a_numerus_message_is_one_group_without_categories() {
    let d = QtTs.extract(TS.as_bytes(), &ExtractOptions::new().with_target_lang("pl")).unwrap();
    assert_eq!(d.units.len(), 2, "no units added");
    let p = d.units[0].plural().expect("numerus slot");
    assert_eq!((p.group(), p.kind(), p.category(), p.is_added()), (d.units[0].id.as_str(), PluralKind::Cardinal, None, false));
    assert!(d.units[1].plural().is_none());
    assert_eq!(QtTs.merge(&d).unwrap(), TS.as_bytes());
    let l = QtTs.extract(TS.as_bytes(), &ExtractOptions::new().with_target_lang("pl").with_profile(profile::LOCOS)).unwrap();
    assert!(l.units.iter().all(|u| u.plural().is_none()));
}

//! Unit model step 7: a FormatJS (ICU MessageFormat) string stays one unit with its structure protected, and gets
//! a plural slot from the shared ICU parser: the group, the kind and argument of its first plural, selectordinal or
//! select argument, no category. Nothing is filled under the LocOS profile.

use bilingua_jsmodule::JsModule;
use bilingua_model::*;

const JS: &str = "export default {\n  files: '{count, plural, one {# file} other {# files}}',\n  who: '{g, select, female {her} other {their}} files',\n  hi: 'Hello {name}',\n};\n";

#[test]
fn icu_strings_get_a_slot_and_stay_one_unit() {
    let d = JsModule.extract(JS.as_bytes(), &ExtractOptions::new().with_target_lang("pl")).unwrap();
    assert_eq!(d.units.len(), 3, "no units added");
    let slot = |i: usize| d.units[i].plural().map(|p| (p.group().to_string(), p.kind(), p.arg().map(str::to_string), p.category().map(str::to_string)));
    assert_eq!(slot(0), Some((d.units[0].id.clone(), PluralKind::Cardinal, Some("count".into()), None)));
    assert_eq!(slot(1), Some((d.units[1].id.clone(), PluralKind::Select, Some("g".into()), None)));
    assert_eq!(slot(2), None);
    assert_eq!(JsModule.merge(&d).unwrap(), JS.as_bytes());
    let l = JsModule.extract(JS.as_bytes(), &ExtractOptions::new().with_profile(profile::LOCOS)).unwrap();
    assert!(l.units.iter().all(|u| u.plural().is_none()));
}

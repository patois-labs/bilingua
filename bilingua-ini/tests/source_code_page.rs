//! A legacy (not UTF-8) file is read in the code page of the project's source language (LocOS decision 22).
use bilingua_ini::IniFilter;
use bilingua_model::*;

/// "Dobro pozhalovat" in Windows-1251.
const RU_1251: &[u8] = b"[main]\r\ntitle=\xc4\xee\xe1\xf0\xee \xef\xee\xe6\xe0\xeb\xee\xe2\xe0\xf2\xfc\r\n";

fn src(doc: &Document) -> String {
    plain_text(&doc.units[0].source())
}

#[test]
fn russian_source_reads_windows_1251() {
    let opts = ExtractOptions::new().with_source_lang("ru");
    let doc = IniFilter.extract(RU_1251, &opts).unwrap();
    assert_eq!(src(&doc), "\u{414}\u{43e}\u{431}\u{440}\u{43e} \u{43f}\u{43e}\u{436}\u{430}\u{43b}\u{43e}\u{432}\u{430}\u{442}\u{44c}");
    // untouched merge is byte exact
    assert_eq!(IniFilter.merge(&doc).unwrap(), RU_1251);
}

#[test]
fn no_source_language_keeps_windows_1252() {
    let doc = IniFilter.extract(RU_1251, &ExtractOptions::new()).unwrap();
    assert!(src(&doc).starts_with('\u{c4}'));
    assert_eq!(IniFilter.merge(&doc).unwrap(), RU_1251);
}

#[test]
fn translated_text_outside_the_code_page_becomes_utf8() {
    let opts = ExtractOptions::new().with_source_lang("ru");
    let mut doc = IniFilter.extract(RU_1251, &opts).unwrap();
    let id = doc.units[0].segments[0].id.clone();
    doc.set_target(&id, vec![Inline::text("\u{4f60}\u{597d}")]).unwrap();
    let out = IniFilter.merge(&doc).unwrap();
    assert!(String::from_utf8(out).unwrap().contains("\u{4f60}\u{597d}"));
}

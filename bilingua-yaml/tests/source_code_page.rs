//! A legacy (not UTF-8) file is read in the code page of the project's source language (LocOS decision 22).
use bilingua_model::*;
use bilingua_yaml::Yaml;

/// A Russian greeting in Windows-1251.
const RU_1251: &[u8] = b"title: \xc4\xee\xe1\xf0\xee \xef\xee\xe6\xe0\xeb\xee\xe2\xe0\xf2\xfc\n";
const GREETING: &str = "\u{414}\u{43e}\u{431}\u{440}\u{43e} \u{43f}\u{43e}\u{436}\u{430}\u{43b}\u{43e}\u{432}\u{430}\u{442}\u{44c}";

fn src(doc: &Document) -> String {
    plain_text(&doc.units[0].source())
}

#[test]
fn russian_source_reads_windows_1251() {
    let opts = ExtractOptions::new().with_source_lang("ru");
    let doc = Yaml.extract(RU_1251, &opts).unwrap();
    assert_eq!(src(&doc), GREETING);
    assert_eq!(Yaml.merge(&doc).unwrap(), RU_1251);
}

#[test]
fn no_source_language_keeps_windows_1252() {
    let doc = Yaml.extract(RU_1251, &ExtractOptions::new()).unwrap();
    assert!(src(&doc).starts_with('\u{c4}'));
    assert_eq!(Yaml.merge(&doc).unwrap(), RU_1251);
}

#[test]
fn translated_text_is_written_as_utf8() {
    let opts = ExtractOptions::new().with_source_lang("ru");
    let mut doc = Yaml.extract(RU_1251, &opts).unwrap();
    let id = doc.units[0].segments[0].id.clone();
    doc.set_target(&id, vec![Inline::text("\u{4f60}\u{597d}")]).unwrap();
    let out = Yaml.merge(&doc).unwrap();
    assert!(String::from_utf8(out).unwrap().contains("\u{4f60}\u{597d}"));
}

/// Shift_JIS has two spellings for one character (NEC row 13 byte pair 87 9B and the JIS X 0208 pair 81 E6 both read as
/// U+2235). A Japanese file that uses the NEC spelling comes back byte for byte when nothing changed.
#[test]
fn double_mapped_shift_jis_untouched_merge_keeps_the_original_bytes() {
    const SJIS: &[u8] = b"title: \x82\xb1\x82\xf1\x82\xc9\x82\xbf\x82\xcd\x87\x9b\n";
    let opts = ExtractOptions::new().with_source_lang("ja");
    let doc = Yaml.extract(SJIS, &opts).unwrap();
    assert_eq!(src(&doc), "\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}\u{2235}");
    assert_eq!(Yaml.merge(&doc).unwrap(), SJIS);
}

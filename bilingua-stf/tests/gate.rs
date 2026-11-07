//! Opus gate (11-05): existing translations as targets (default) and `stf.load_targets` = false, injection through
//! translations, the column-header quadratic path.
mod common;
use bilingua_model::*;
use bilingua_stf::{records, StfFilter, LOAD_TARGETS};
use common::*;

const BILINGUAL: &str = "Language code: de\nType: Bilingual\n\n------------------TRANSLATED-------------------\n\n# KEY\tLABEL\tTRANSLATION\tOUT OF DATE\nCustomLabel.A\tSave\tSpeichern\t-\nCustomLabel.B\tHi {0}\\n{!$User.Name}\t{!$User.Name} Hallo\\n{0} \\t {9}\t*\nCustomLabel.C\tCancel\t\t-\nCustomLabel.D\tKeep\t   \t-\nCustomLabel.E\tOnly code\t{0}\t-\n\n------------------UNTRANSLATED-----------------\n\n# KEY\tLABEL\nCustomLabel.F\tWelcome\n";

fn off() -> ExtractOptions {
    locos_opts()
}

fn targets(doc: &Document) -> Vec<(String, SegState)> {
    doc.units.iter().map(|u| (plain_text(&u.segments[0].target), u.segments[0].state)).collect()
}

#[test]
fn existing_translations_load_as_targets_by_default() {
    let doc = extract(BILINGUAL.as_bytes()).unwrap();
    assert_eq!(doc.units.len(), 6);
    let t = targets(&doc);
    assert_eq!(t[0], ("Speichern".into(), SegState::Translated));
    assert_eq!(t[1].1, SegState::Draft, "OUT OF DATE * is a draft");
    assert_eq!(t[2], (String::new(), SegState::New), "empty TRANSLATION cell");
    assert_eq!(t[3], (String::new(), SegState::New), "blank TRANSLATION cell");
    assert_eq!(t[4].1, SegState::Translated, "a placeholder-only translation is still a translation");
    assert_eq!(t[5], (String::new(), SegState::New), "UNTRANSLATED rows have no target");
    // the placeholders of the source map onto the target by text; {9} and \t are not in the source and stay text
    let tags = |v: &[Inline]| v.iter().filter_map(|i| if let Inline::Tag { id, .. } = i { Some(id.clone()) } else { None }).collect::<Vec<_>>();
    let b = &doc.units[1];
    assert_eq!(tags(&b.segments[0].source), ["1", "2", "3"]);
    assert_eq!(tags(&b.segments[0].target), ["3", "2", "1"]);
    assert!(plain_text(&b.segments[0].target).contains("\\t {9}"));
    // a loaded target obeys the model's tag rules: setting it again is accepted
    let mut d2 = doc.clone();
    d2.set_target("u2", b.segments[0].target.clone()).unwrap();
    assert_eq!(StfFilter.merge(&d2).unwrap(), BILINGUAL.as_bytes());
}

#[test]
fn load_targets_false_gives_the_locos_unit_list() {
    let on = extract(BILINGUAL.as_bytes()).unwrap();
    let doc = StfFilter.extract(BILINGUAL.as_bytes(), &off()).unwrap();
    assert!(doc.units.iter().all(|u| u.segments[0].target.is_empty() && u.segments[0].state == SegState::New));
    let src = |d: &Document| d.units.iter().map(|u| u.source()).collect::<Vec<_>>();
    assert_eq!(src(&doc), src(&on));
    for v in ["0", "no", "off", "FALSE"] {
        let o = opts().with_format_options([(LOAD_TARGETS.to_string(), v.to_string())].into());
        assert!(StfFilter.extract(BILINGUAL.as_bytes(), &o).unwrap().units.iter().all(|u| u.segments[0].target.is_empty()), "{v}");
    }
    let o = opts().with_format_options([(LOAD_TARGETS.to_string(), "true".to_string())].into());
    assert_eq!(targets(&StfFilter.extract(BILINGUAL.as_bytes(), &o).unwrap()), targets(&on));
}

#[test]
fn untouched_merge_is_exact_in_both_modes_even_with_a_target_language() {
    let mut files = corpus();
    files.push(("gate".into(), BILINGUAL.as_bytes().to_vec()));
    for (name, data) in &files {
        for eo in [opts(), off()] {
            for target in [None, Some("ja-JP"), Some("de")] {
                let eo = eo.clone().with_target_lang(target.map(String::from));
                let doc = StfFilter.extract(data, &eo).unwrap();
                assert_eq!(StfFilter.merge(&doc).unwrap(), *data, "{name} {target:?}");
                assert_eq!(StfFilter.merge_with(&doc, &MergeOptions::default()).unwrap(), *data, "{name} {target:?}");
            }
        }
    }
}

#[test]
fn editing_one_loaded_target_changes_only_that_cell_and_stamps() {
    let mut doc = extract_for(BILINGUAL.as_bytes(), Some("de-AT")).unwrap();
    doc.set_target("u1", vec![Inline::text("Sichern")]).unwrap();
    let out = String::from_utf8(StfFilter.merge(&doc).unwrap()).unwrap();
    let want = BILINGUAL.replace("Save\tSpeichern\t-", "Save\tSichern\t-").replace("Language code: de\n", "Language code: de_AT\n");
    assert_eq!(out, want);
    // new translation for an UNTRANSLATED row goes in place of the LABEL
    let mut doc = extract(BILINGUAL.as_bytes()).unwrap();
    doc.set_target("u6", vec![Inline::text("Willkommen")]).unwrap();
    let out = String::from_utf8(StfFilter.merge(&doc).unwrap()).unwrap();
    assert_eq!(out, BILINGUAL.replace("CustomLabel.F\tWelcome", "CustomLabel.F\tWillkommen"));
    // merge_with keeps the same rules and writes UTF-8 when asked
    let out = StfFilter.merge_with(&doc, &bilingua_model::profile::locos().merge_options("stf")).unwrap();
    assert!(String::from_utf8(out).unwrap().contains("CustomLabel.F\tWillkommen\n"));
}

#[test]
fn hostile_translations_cannot_split_rows_or_add_headers() {
    let evil = [
        "a\tb",
        "a\nb",
        "a\r\nb",
        "a\rb",
        "\n# KEY\tLABEL\tTRANSLATION",
        "\nLanguage code: xx",
        "\nType: Source",
        "\n------------------UNTRANSLATED-----------------\nCustomLabel.Z\tInjected",
        "# comment",
        "\\t \\n \\r \\\\",
        "{0} {!$User.Id} {99}",
        "\u{2028}line\u{85}sep\u{c}ff",
    ];
    for e in evil {
        for eo in [opts(), off()] {
            let mut doc = StfFilter.extract(BILINGUAL.as_bytes(), &eo).unwrap();
            let ids: Vec<String> = doc.units.iter().map(|u| u.segments[0].id.clone()).collect();
            for id in &ids {
                doc.set_target(id, vec![Inline::text(format!("x{e}"))]).unwrap();
            }
            for mo in [MergeOptions::default(), bilingua_model::profile::locos().merge_options("stf")] {
                let out = StfFilter.merge_with(&doc, &mo).unwrap();
                let text = String::from_utf8(out.clone()).unwrap();
                assert_eq!(text.lines().count(), BILINGUAL.lines().count(), "{e:?}: a row split");
                let keys: Vec<String> = records(&out).unwrap().iter().map(|r| r.key.clone()).collect();
                let want: Vec<String> = records(BILINGUAL.as_bytes()).unwrap().iter().map(|r| r.key.clone()).collect();
                assert_eq!(keys, want, "{e:?}: keys");
                assert!(text.starts_with("Language code: de\nType: Bilingual\n"), "{e:?}: headers");
                let back = StfFilter.extract(&out, &off()).unwrap();
                assert_eq!(back.units.len(), doc.units.len(), "{e:?}: unit count");
                for (r, w) in records(&out).unwrap().iter().zip(records(BILINGUAL.as_bytes()).unwrap()) {
                    assert_eq!(r.section, w.section, "{e:?}: section");
                }
            }
        }
    }
}

#[test]
fn translations_in_legacy_and_utf16_files_round_trip() {
    for name in ["bilingual_fr_cp1252.stf", "bilingual_zh_utf16le.stf", "bilingual_ko_utf16be.stf", "bilingual_de_utf8bom.stf"] {
        let data = corpus().into_iter().find(|(n, _)| n == name).unwrap().1;
        let doc = extract(&data).unwrap();
        assert!(doc.units.iter().any(|u| !u.segments[0].target.is_empty()), "{name}: targets loaded");
        assert_eq!(StfFilter.merge(&doc).unwrap(), data, "{name}");
        let mut d = doc.clone();
        let id = d.units[0].segments[0].id.clone();
        d.set_target(&id, vec![Inline::text("\u{e9}t\u{e9}")]).unwrap();
        let out = StfFilter.merge(&d).unwrap();
        let back = extract(&out).unwrap();
        assert_eq!(plain_text(&back.units[0].segments[0].target), "\u{e9}t\u{e9}", "{name}");
        assert_eq!(back.skeleton_encoding(), doc.skeleton_encoding(), "{name}: encoding kept");
    }
}

trait Enc {
    fn skeleton_encoding(&self) -> String;
}
impl Enc for Document {
    fn skeleton_encoding(&self) -> String {
        match &self.skeleton {
            Skeleton::Text { encoding, .. } => encoding.clone(),
            _ => String::new(),
        }
    }
}

#[test]
fn a_wide_column_header_over_many_rows_stays_linear() {
    // 200 000 column names, then 400 000 data rows: per-row column lookups would be 8e10 steps
    let mut s = String::from("Language code: de\n# KEY");
    for _ in 0..200_000 {
        s.push_str("\tLABEL");
    }
    s.push_str("\tTRANSLATION\n");
    for _ in 0..400_000 {
        s.push_str("K\t\n");
    }
    let t = std::time::Instant::now();
    let doc = StfFilter.extract(s.as_bytes(), &opts()).unwrap();
    assert_eq!(doc.units.len(), 0);
    assert_eq!(StfFilter.merge(&doc).unwrap(), s.as_bytes());
    assert!(t.elapsed().as_secs() < 30, "took {:?}", t.elapsed());
}

#[test]
fn a_label_full_of_placeholders_loads_its_translation_linearly() {
    // 99 000 `\n` in the label and in the translation: matching tags by a linear search per tag would be ~5e9 steps
    let cell = format!("a{}", "\\n".repeat(99_000));
    let s = format!("Language code: de\n# KEY\tLABEL\tTRANSLATION\nCustomLabel.A\t{cell}\tb{}\n", &cell[1..]);
    let t = std::time::Instant::now();
    let doc = StfFilter.extract(s.as_bytes(), &opts()).unwrap();
    let seg = &doc.units[0].segments[0];
    assert_eq!(seg.target.iter().filter(|i| matches!(i, Inline::Tag { .. })).count(), 99_000);
    assert_eq!(StfFilter.merge(&doc).unwrap(), s.as_bytes());
    assert!(t.elapsed().as_secs() < 30, "took {:?}", t.elapsed());
}

/// Wild file (11-06): a Source file whose rows use `KEY=label` instead of a tab has no rows. It stays byte for byte
/// and says why.
#[test]
fn rows_without_a_tab_give_no_units_and_a_warning() {
    let src = "Language code: en_US\nType: Source\n\nCustomLabel.A=Save\nCustomLabel.B=Cancel\n";
    let doc = StfFilter.extract(src.as_bytes(), &off()).unwrap();
    assert!(doc.units.is_empty());
    assert!(doc.warnings.iter().any(|w| w.kind == "no_tab_rows"), "{:?}", doc.warnings);
    assert_eq!(StfFilter.merge(&doc).unwrap(), src.as_bytes());
    // an empty Bilingual file has nothing stray and nothing to say
    let empty = "Language code: ja\nType: Bilingual\n\n------------------TRANSLATED-------------------\n\n------------------UNTRANSLATED-----------------\n";
    assert!(StfFilter.extract(empty.as_bytes(), &off()).unwrap().warnings.is_empty());
}

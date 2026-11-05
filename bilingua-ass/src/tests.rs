use super::*;

#[test]
fn keywords_follow_python_ignorecase() {
    assert_eq!(head_len("Title: x", "title", true), Some(7));
    assert_eq!(head_len("T\u{130}tle:x", "title", true), Some(7));
    assert_eq!(head_len("T\u{131}TLE :\t x", "title", true), Some(10));
    assert_eq!(head_len("Titel: x", "title", true), None);
    assert_eq!(head_len("dialogue: x", "Dialogue", false), None);
    assert_eq!(colon_end("Format : a,b", "format", true), Some(8));
}

#[test]
fn lines_end_only_at_cr_lf_crlf() {
    let t = "a\u{2028}b\r\nc\rd\ne\x0cf";
    let v: Vec<&str> = lines(t).map(|(a, b, _)| &t[a..b]).collect();
    assert_eq!(v, ["a\u{2028}b", "c", "d", "e\x0cf"]);
    assert_eq!(lines("").count(), 0);
    assert_eq!(lines("\n\n").count(), 2);
}

#[test]
fn text_field_split_counts_commas() {
    assert_eq!(split_text_field("0,1,2,3", 4), Some(6));
    assert_eq!(split_text_field("0,1,2", 4), None);
    assert_eq!(split_text_field("a, b, c", 1), Some(0));
    assert_eq!(split_text_field("0,1,x,y,z", 3), Some(4));
}

#[test]
fn safe_text_neutralises_codes() {
    assert_eq!(safe_text("a{b"), "a\u{ff5b}b");
    assert_eq!(safe_text("a\\Nb"), "a\u{ff3c}Nb");
    assert_eq!(safe_text("a\\"), "a\u{ff3c}");
    assert_eq!(safe_text("C:\\x"), "C:\\x");
    assert_eq!(safe_text("a\r\nb\rc\nd"), "a\\Nb\\Nc\\Nd");
    assert_eq!(safe_text("\\\\N"), "\\\u{ff3c}N");
}

#[test]
fn stamp_rules() {
    assert_eq!(stamp_language("[Script Info]\nTitle: x\n", "de-DE"), "[Script Info]\nLanguage: de\nTitle: x\n");
    assert_eq!(stamp_language("[Script Info]\nLanguage: en  \nx\n", "fr"), "[Script Info]\nLanguage: fr  \nx\n");
    assert_eq!(stamp_language("[Script Info]\nLanguage: en\n", "fil"), "[Script Info]\n");
    assert_eq!(stamp_language("[Events]\n", "de"), "[Events]\n");
    assert_eq!(stamp_language("[Script Info]\nA\n", "!!"), "[Script Info]\nA\n");
    assert_eq!(stamp_language("[Script Info]", "es"), "[Script Info]\nLanguage: es\n");
}

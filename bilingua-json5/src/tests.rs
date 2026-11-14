use super::scan::{quote, scan, Limits};

fn vals(src: &str) -> Vec<(String, String, String)> {
    scan(src, &Limits::default()).unwrap().into_iter().map(|v| (v.key, v.value, v.comment)).collect()
}

fn fails(src: &str) -> String {
    scan(src, &Limits::default()).unwrap_err().to_string()
}

fn bs(s: &str) -> String {
    s.replace('@', "\x5c")
}

#[test]
fn key_paths_are_dotted_with_indexes() {
    let v = vals("{a: {b: ['x', {c: 'y'}], 'd e': 'z'}, '': 'w'}");
    let keys: Vec<&str> = v.iter().map(|r| r.0.as_str()).collect();
    assert_eq!(keys, vec!["a.b[0]", "a.b[1].c", "a.d e", ""]);
    assert_eq!(vals("['a', ['b']]").iter().map(|r| r.0.clone()).collect::<Vec<_>>(), vec!["[0]", "[1][0]"]);
}

#[test]
fn comment_attaches_to_the_key_below_it_not_the_value_above() {
    let v = vals("{\n  a: 'one', // about a\n  // about b\n  /* more about b */\n  b: 'two',\n}");
    assert_eq!(v[0].2, "");
    assert_eq!(v[1].2, "about b\nmore about b");
}

#[test]
fn block_comment_lines_lose_their_stars() {
    let v = vals("{\n  /**\n   * First line\n   * second line\n   */\n  a: 'x',\n}");
    assert_eq!(v[0].2, "First line\nsecond line");
}

#[test]
fn comment_before_an_array_item() {
    let v = vals("[\n  // item note\n  'x',\n  'y',\n]");
    assert_eq!(v[0].2, "item note");
    assert_eq!(v[1].2, "");
}

#[test]
fn first_comment_in_the_file_is_collected_even_on_the_first_line() {
    let v = vals("/* top */ {a: 'x'}");
    assert_eq!(v[0].2, "");
    // a comment on the same line as the opening brace belongs to the brace (LocOS rule)
    let v = vals("{ /* same line */ a: 'x'}");
    assert_eq!(v[0].2, "");
    let v = vals("{
 /* above a */ a: 'x'}");
    assert_eq!(v[0].2, "above a");
}

#[test]
fn escapes_decode_per_json5() {
    assert_eq!(vals(&bs("['a@tb@nc@x41@u0042@/@'@\"']"))[0].1, "a\tb\ncAB/'\"");
    assert_eq!(vals(&bs("['@b@f@r@v@0']"))[0].1, "\u{8}\u{c}\r\u{b}\0");
    assert_eq!(vals(&bs("['@d']"))[0].1, "d"); // an unknown escape is the character itself
    assert_eq!(vals(&bs("['one@\ntwo']"))[0].1, "onetwo");
    assert_eq!(vals(&bs("['one@\r\ntwo']"))[0].1, "onetwo");
    assert_eq!(vals(&bs("['@ud83d@ude00']"))[0].1, "\u{1f600}");
    // a line continuation between the two halves of a pair produces nothing (LocOS joins the pair)
    assert_eq!(vals(&bs("['@ud83d@\n@ude00']"))[0].1, "\u{1f600}");
}

#[test]
fn bad_escapes_are_errors() {
    for s in ["['@1']", "['@9']", "['@x4']", "['@u12']", "['@00']", "['@ud800']", "['@udc00']", "['@ud800@u0041']", "['@ud800x']"] {
        assert!(fails(&bs(s)).contains("JSON5"), "{s}");
    }
}

#[test]
fn unicode_keys_and_escaped_keys() {
    let v = vals(&bs("{caf\u{e9}: 'a', \u{65e5}\u{672c}: 'b', @u0061bc: 'c', _x$1: 'd', \u{4e00}2: 'e'}"));
    assert_eq!(v.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(), vec!["caf\u{e9}", "\u{65e5}\u{672c}", "abc", "_x$1", "\u{4e00}2"]);
    assert!(fails("{1a: 'x'}").contains("key"));
}

#[test]
fn number_and_keyword_spellings() {
    for ok in ["[1]", "[-1]", "[+1.5e-3]", "[.5]", "[5.]", "[0xFFab]", "[-0x1]", "[Infinity,-Infinity,+Infinity,NaN,-NaN]", "[true,false,null]", "[1e3, 1E+3]"] {
        assert!(scan(ok, &Limits::default()).is_ok(), "{ok}");
    }
    for bad in ["[0x]", "[1e]", "[1e+]", "[.]", "[+]", "[-]", "[truex]", "[nullx]", "[1_000]", "[1x]", "[+true]", "[Infinityx]", "[-.]"] {
        assert!(scan(bad, &Limits::default()).is_err(), "{bad}");
    }
}

#[test]
fn whitespace_includes_unicode_spaces_and_bom() {
    let ok = "\u{feff}\u{a0}{\u{2003}a:\u{3000}'x'\u{2028}}\u{205f}";
    assert_eq!(vals(ok)[0].1, "x");
}

#[test]
fn line_comments_end_at_unicode_line_separators() {
    assert_eq!(vals("{ // c\u{2028}a: 'x'}")[0].1, "x");
}

#[test]
fn trailing_commas_and_empty_containers() {
    assert!(scan("{a: [], b: {}, c: [1,], d: {e: 1,},}", &Limits::default()).is_ok());
    assert!(scan("[,]", &Limits::default()).is_err());
    assert!(scan("{a: 1,,}", &Limits::default()).is_err());
}

#[test]
fn top_level_forms() {
    assert_eq!(vals("'only'")[0].1, "only");
    assert_eq!(vals("  // c\n 'only' // d\n")[0].1, "only");
    assert!(scan("1", &Limits::default()).unwrap().is_empty());
    assert!(fails("'a' 'b'").contains("after the top-level value"));
}

#[test]
fn raw_line_breaks_inside_a_string_are_errors_but_ls_ps_are_fine() {
    assert!(scan("['a\nb']", &Limits::default()).is_err());
    assert!(scan("['a\rb']", &Limits::default()).is_err());
    assert_eq!(vals("['a\u{2028}b']")[0].1, "a\u{2028}b");
}

#[test]
fn quote_escapes_only_what_the_style_needs() {
    assert_eq!(quote("it's \"x\"", b'\''), "'it\\'s \"x\"'");
    assert_eq!(quote("it's \"x\"", b'"'), "\"it's \\\"x\\\"\"");
    assert_eq!(quote("a\u{7f}b\u{1}", b'"'), "\"a\\u007fb\\u0001\"");
    assert_eq!(quote("\u{8}\u{c}", b'"'), "\"\\b\\f\"");
}

#[test]
fn the_selection_rule_is_any_letter_or_digit() {
    use super::scan::is_translatable;
    for t in ["a", "1", "---a", "\u{65e5}", "\u{627}", "\u{663}"] {
        assert!(is_translatable(t), "{t}");
    }
    for t in ["", " ", "---", "{}", "...", "\u{2014}", "%"] {
        assert!(!is_translatable(t), "{t}");
    }
}

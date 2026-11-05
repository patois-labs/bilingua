//! Port of the Localization OS suite `tests/verify_properties_filter.py` (9 cases) and
//! `tests/verify_mozilla_properties.py` (3 cases, in `locos_mozilla.rs`). Same inputs, same expected bytes.

mod common;

use bilingua_model::*;
use bilingua_properties::{Limits, Properties};
use common::*;

pub const ASCII: &[u8] = b"# Greeting shown on the home page\r\n\
! second comment style\r\n\
app.title = Welcome to Northwind\r\n\
app.colon:Sign in\r\n\
app.space Sign out\r\n\
long.text = First part, \\\r\n    second part\r\n\
latin = Caf\\u00e9 au lait\r\n\
empty =\r\n\
dup = First\r\n\
dup = Second\r\n\
mf.args = Hello {0}, you have {1} messages\r\n\
mf.apos = Don''t delete {0}\r\n\
mf.lonequote = Use '{0}' literally\r\n\
plain.apos = Don't panic\r\n\
id.only = {0}\r\n\
pad = \\ leading space\r\n\
key\\ with\\ space = Spaced key value\r\n\
url = https\\://example.com\r\n";

const ASCII_UNITS: [&str; 14] = [
    "Welcome to Northwind",
    "Sign in",
    "Sign out",
    "First part, second part",
    "Caf\u{e9} au lait",
    "First",
    "Second",
    "Hello {0}, you have {1} messages",
    "Don't delete {0}",
    "Use '{0}' literally",
    "Don't panic",
    "leading space",
    "Spaced key value",
    "https://example.com",
];

fn replace(hay: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let p = hay.windows(from.len()).position(|w| w == from).unwrap_or_else(|| panic!("missing {:?}", String::from_utf8_lossy(from)));
    let mut v = hay[..p].to_vec();
    v.extend_from_slice(to);
    v.extend_from_slice(&hay[p + from.len()..]);
    v
}

#[test]
fn grammar_and_units() {
    assert_eq!(units(ASCII), ASCII_UNITS);
    assert_identity(ASCII);
}

#[test]
fn ascii_round_trip_escapes_non_ascii() {
    let mapping: Vec<(usize, &str)> = vec![
        (0, "Willkommen bei Northwind"),
        (3, "Erster Teil,\nzweiter Teil"),
        (4, "Caf\u{e9} \u{2014} \u{65e5}\u{672c} \u{1f600}"),
        (8, "N'effacez pas {0}"),
        (10, "Pas de panique, c'est {fini}"),
        (11, "espace"),
        (12, "a\\b"),
    ];
    let mut exp = ASCII.to_vec();
    for (a, b) in [
        (&b"Welcome to Northwind"[..], &b"Willkommen bei Northwind"[..]),
        (b"First part, \\\r\n    second part", b"Erster Teil,\\nzweiter Teil"),
        (b"Caf\\u00e9 au lait", b"Caf\\u00E9 \\u2014 \\u65E5\\u672C \\uD83D\\uDE00"),
        (b"Don''t delete {0}", b"N''effacez pas {0}"),
        (b"Don't panic", b"Pas de panique, c'est {fini}"),
        (b"\\ leading space", b"\\ espace"),
        (b"Spaced key value", b"a\\\\b"),
    ] {
        exp = replace(&exp, a, b);
    }
    assert_round_trip(ASCII, &mapping, &exp);
}

#[test]
fn messageformat_apostrophes() {
    let src = b"a = It''s {0}\nb = Quote '{0}' here\nc = It's plain\n";
    assert_eq!(units(src), ["It's {0}", "Quote '{0}' here", "It's plain"]);
    let out = translate(src, &[(0, "C'est {0}"), (1, "Zitat '{0}' hier"), (2, "C'est simple")]);
    assert_eq!(out, b"a = C''est {0}\nb = Zitat '{0}' hier\nc = C'est simple\n");
    // a translator who already typed '' does not get it quadrupled
    let out2 = translate(src, &[(0, "C''est {0}")]);
    assert_eq!(out2, b"a = C''est {0}\nb = Quote '{0}' here\nc = It's plain\n");
    // a value with {n} arguments and no apostrophe is a MessageFormat pattern too
    let src3 = b"open = Open the application {0} now\nsave = Saved {1,number} of {0}\nhelp = Open help\n";
    let out3 = translate(
        src3,
        &[
            (0, "Ouvrez l'application {0} maintenant"),
            (1, "{1,number} sur {0} enregistr\u{e9}s d'abord"),
            (2, "Ouvrez l'aide"),
        ],
    );
    assert_eq!(
        out3,
        b"open = Ouvrez l''application {0} maintenant\nsave = {1,number} sur {0} enregistr\\u00E9s d''abord\nhelp = Ouvrez l'aide\n"
    );
    // a source that quotes on purpose is shown as written; an elision in the translation never quotes
    let src4 = b"hint = Wrap a variable in '{' and '}' to insert it into {0}.\n";
    assert_eq!(units(src4), ["Wrap a variable in '{' and '}' to insert it into {0}."]);
    let out4 = translate(src4, &[(0, "Envelopper une variable entre '{' et '}' pour l'ins\u{e9}rer dans {0}.")]);
    assert_eq!(out4, b"hint = Envelopper une variable entre '{' et '}' pour l''ins\\u00E9rer dans {0}.\n");
    let out5 = translate(src4, &[(0, "Racchiudi '{' e '}' per l''inserirla in {0}.")]);
    assert_eq!(out5, b"hint = Racchiudi '{' e '}' per l''inserirla in {0}.\n");
    // the elision closes a quote the translation opened: left single, so {0} keeps working
    let src6 = b"json = Type '{'\"id\": 42'}' to select the record in {0}.\n";
    let tr6 = "Tapez '{'\"id\": 42\"}' pour s\u{e9}lectionner l'enregistrement dans {0}.";
    let out6 = translate(src6, &[(0, tr6)]);
    assert!(String::from_utf8_lossy(&out6).contains("l'enregistrement dans {0}."), "{}", String::from_utf8_lossy(&out6));
}

#[test]
fn latin1_and_utf8_encodings() {
    let latin: Vec<u8> = "title=Caf\u{e9}\nother=Na\u{ef}ve\n".chars().map(|c| c as u8).collect();
    assert_eq!(units(&latin), ["Caf\u{e9}", "Na\u{ef}ve"]);
    let d = extract(&latin);
    assert!(matches!(&d.skeleton, Skeleton::Text { encoding, .. } if encoding == "iso-8859-1"));
    let out = translate(&latin, &[(0, "Caf\u{e9} \u{65e5}\u{672c}")]);
    assert_eq!(out, b"title=Caf\xe9 \\u65E5\\u672C\nother=Na\xefve\n");
    assert_identity(&latin);

    let utf8 = "\u{feff}title=Caf\u{e9}\n".as_bytes();
    assert_eq!(units(utf8), ["Caf\u{e9}"]);
    let out = translate(utf8, &[(0, "\u{65e5}\u{672c}\u{8a9e}")]);
    assert_eq!(out, "\u{feff}title=\u{65e5}\u{672c}\u{8a9e}\n".as_bytes());
    assert_identity(utf8);
}

#[test]
fn hostile_inputs() {
    let err = Properties.extract(b"k = bad \\u12G4\n", &opts()).unwrap_err();
    assert!(matches!(&err, Error::Parse { msg, .. } if msg.contains("malformed")), "{err}");
    let bin = b"k = v\x00\x01\x02\x03\x04\x05".repeat(50);
    let err = Properties.extract(&bin, &opts()).unwrap_err();
    assert!(matches!(&err, Error::Unsupported(m) if m.contains("binary")), "{err}");
    let utf16: Vec<u8> = [0xFFu8, 0xFE].into_iter().chain("k=v\n".encode_utf16().flat_map(|u| u.to_le_bytes())).collect();
    let err = Properties.extract(&utf16, &opts()).unwrap_err();
    assert!(matches!(&err, Error::Unsupported(m) if m.contains("UTF-16")), "{err}");
    // over the unit cap
    let big: String = (0..30).map(|i| format!("key{i} = Value number {i}\n")).collect();
    let small = Limits { max_units: 10, ..Limits::default() };
    let err = Properties.extract_with(big.as_bytes(), &opts(), &small).unwrap_err();
    assert!(matches!(&err, Error::Unsupported(m) if m.contains("limit")), "{err}");
    // a key-only file and a file of comments only parse to nothing
    assert!(units(b"# only a comment\nflag\n").is_empty());
    // odd backslash at the end of the file, no next line: no crash
    assert_eq!(units(b"k = tail \\"), ["tail"]);
}

#[test]
fn records_view() {
    let recs = Properties.records(ASCII).unwrap();
    let first = &recs[0];
    assert_eq!(first.key, "app.title");
    assert_eq!(first.value, "Welcome to Northwind");
    assert_eq!(first.comment, "Greeting shown on the home page\nsecond comment style");
    let dups: Vec<bool> = recs.iter().filter(|r| r.key == "dup").map(|r| r.duplicate).collect();
    assert_eq!(dups, [false, true]);
    let by = |k: &str| recs.iter().find(|r| r.key == k).unwrap().clone();
    assert!(by("empty").unit_index.is_none() && by("id.only").unit_index.is_none());
    assert_eq!(by("key with space").value, "Spaced key value");
    assert_eq!(by("url").unit_index, Some(13));
    // the same view in the document
    let d = extract(ASCII);
    assert_eq!(d.units[0].context.key.as_deref(), Some("app.title"));
    assert_eq!(d.units[0].context.notes, ["Greeting shown on the home page", "second comment style"]);
    assert_eq!(d.units[6].context.extra.get("duplicate").map(String::as_str), Some("true"));
}

#[test]
fn placeholders_are_tags() {
    let d = extract(b"a = Hello {0}, you have {1} messages\nb = Saved %s files\nc = Name {user} and %1$s\n");
    let tags = |i: usize| -> Vec<(String, bool)> {
        d.units[i].segments[0].source.iter().filter_map(|x| if let Inline::Tag { display, free, .. } = x { Some((display.clone(), *free)) } else { None }).collect()
    };
    assert_eq!(tags(0), [("{0}".into(), true), ("{1}".into(), true)]);
    assert_eq!(tags(1), [("%s".into(), false)]);
    assert_eq!(tags(2), [("{user}".into(), true), ("%1$s".into(), true)]);
    // a reordered translation keeps its tags
    let out = translate(b"a = Hello {0}, you have {1} messages\n", &[(0, "Hallo {1} {0}")]);
    assert_eq!(out, b"a = Hallo {1} {0}\n");
    // a translation that loses a tag is still written (the quality gate is LocOS's)
    let out = translate(b"a = Saved %s files\n", &[(0, "Dateien")]);
    assert_eq!(out, b"a = Dateien\n");
}

/// Independent emulation of `java.util.Properties.load` for one natural line.
fn java_load_line(line: &str) -> (String, String) {
    let ws = [' ', '\t', '\u{0c}'];
    let line: Vec<char> = line.trim_start_matches(ws).chars().collect();
    let n = line.len();
    let (mut key_len, mut value_start, mut has_sep, mut prec_bs) = (0, n, false, false);
    while key_len < n {
        let c = line[key_len];
        if (c == '=' || c == ':') && !prec_bs {
            value_start = key_len + 1;
            has_sep = true;
            break;
        }
        if ws.contains(&c) && !prec_bs {
            value_start = key_len + 1;
            break;
        }
        prec_bs = if c == '\\' { !prec_bs } else { false };
        key_len += 1;
    }
    while value_start < n {
        let c = line[value_start];
        if !ws.contains(&c) {
            if !has_sep && (c == '=' || c == ':') {
                has_sep = true;
            } else {
                break;
            }
        }
        value_start += 1;
    }
    let conv = |s: &[char]| -> String {
        let mut out = String::new();
        let mut j = 0;
        while j < s.len() {
            if s[j] == '\\' && j + 1 < s.len() {
                let d = s[j + 1];
                if d == 'u' {
                    let h: String = s[j + 2..j + 6].iter().collect();
                    out.push(char::from_u32(u32::from_str_radix(&h, 16).unwrap()).unwrap());
                    j += 6;
                    continue;
                }
                out.push(match d {
                    't' => '\t',
                    'n' => '\n',
                    'r' => '\r',
                    'f' => '\u{0c}',
                    o => o,
                });
                j += 2;
                continue;
            }
            out.push(s[j]);
            j += 1;
        }
        out
    };
    (conv(&line[..key_len]), conv(&line[value_start..]))
}

#[test]
fn leading_separator_chars_732() {
    let src = b"key3 Text 3\nk\\:\\= Text\nkey4=Text 4\nkey5: Text 5\nkey6 = Text 6\nkey7\tTab text\nmf Don''t open {0}\nmid Plain value\n";
    let mapping: Vec<(usize, &str)> = vec![
        (0, "=1+2 Text 3"),
        (1, ": optional"),
        (2, "= Total"),
        (3, ":colon"),
        (4, "==double"),
        (5, "="),
        (6, ": N'ouvrez pas {0}"),
        (7, "a=b: c"),
    ];
    let out = translate(src, &mapping);
    assert_eq!(
        String::from_utf8_lossy(&out),
        "key3 \\=1+2 Text 3\nk\\:\\= \\: optional\nkey4=\\= Total\nkey5: \\:colon\nkey6 = \\==double\nkey7\t\\=\nmf \\: N''ouvrez pas {0}\nmid a=b: c\n"
    );
    let want = [
        ("key3", "=1+2 Text 3"),
        ("k:=", ": optional"),
        ("key4", "= Total"),
        ("key5", ":colon"),
        ("key6", "==double"),
        ("key7", "="),
        ("mf", ": N''ouvrez pas {0}"),
        ("mid", "a=b: c"),
    ];
    let java: Vec<(String, String)> = std::str::from_utf8(&out).unwrap().lines().map(java_load_line).collect();
    let want_owned: Vec<(String, String)> = want.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    assert_eq!(java, want_owned);
    // our own parser reads it back the same way (a bare "=" has no words, so it is not a unit)
    let expect: Vec<String> = (0..8).filter(|i| *i != 5).map(|i| if i == 6 { ": N'ouvrez pas {0}".to_string() } else { mapping[i].1.to_string() }).collect();
    assert_eq!(units(&out), expect);
    let values: Vec<String> = Properties.records(&out).unwrap().into_iter().map(|r| r.value).collect();
    assert_eq!(values, want.iter().map(|w| w.1.to_string()).collect::<Vec<_>>());
    // the source file parses the same in both readers
    let sl: Vec<(String, String)> = std::str::from_utf8(src).unwrap().lines().map(java_load_line).collect();
    assert_eq!(sl[1], ("k:=".to_string(), "Text".to_string()));
    // untouched and echoed entries (including an already escaped leading `\=`) stay byte-identical
    let esc = b"eq \\=already\ncol \\:kept\nplain = x=y\n";
    assert_eq!(units(esc), ["=already", ":kept", "x=y"]);
    assert_identity(esc);
    assert_identity(src);
}

#[test]
fn end_to_end_latin1_upload_and_export() {
    let src: Vec<u8> = "# Title\r\ntitle = Caf\u{e9} menu\r\nid = {0}\r\nbye = Goodbye {0}\r\n".chars().map(|c| c as u8).collect();
    assert_eq!(units(&src), ["Caf\u{e9} menu", "Goodbye {0}"]);
    let out = translate(&src, &[(0, "Caf\u{e9}-Men\u{fc} \u{2615}"), (1, "Auf Wiedersehen {0}")]);
    assert_eq!(out, b"# Title\r\ntitle = Caf\xe9-Men\xfc \\u2615\r\nid = {0}\r\nbye = Auf Wiedersehen {0}\r\n");
}

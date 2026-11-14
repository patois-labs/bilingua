//! Regression tests for the 0.9.1 security audit: `segment` and `wordcount` never write a control character of
//! the document to the terminal.

use std::path::PathBuf;

fn run(args: &[&str]) -> String {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let mut out: Vec<u8> = Vec::new();
    bilingua_cli::run(&args, &mut out).unwrap();
    String::from_utf8(out).unwrap()
}

fn file(name: &str, body: &str) -> String {
    let d: PathBuf = std::env::temp_dir().join(format!("bilingua-cli-audit-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    let p = d.join(name);
    std::fs::write(&p, body).unwrap();
    p.to_string_lossy().to_string()
}

/// True for what the escaper must never let through: C0, DEL, C1, line separators and bidirectional controls.
fn unsafe_char(c: char) -> bool {
    c.is_control() || matches!(c, '\u{61c}' | '\u{200e}' | '\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

/// The rows of an output: nothing unsafe is left once the CLI's own tabs and line ends are taken away.
fn rows(out: &str) -> Vec<Vec<&str>> {
    let rows: Vec<Vec<&str>> = out.strip_suffix('\n').expect("ends with a line end").split('\n').map(|r| r.split('\t').collect()).collect();
    for cell in rows.iter().flatten() {
        assert!(!cell.chars().any(unsafe_char), "{cell:?}");
    }
    rows
}

#[test]
fn segment_escapes_terminal_and_bidi_controls() {
    let txt = "Hello \u{1b}]0;TITLE\u{7} and \u{1b}[2J\u{1b}[1;1H world \u{9b}31m c1 \u{7f} del \u{202e}evil\u{2066}x\u{2069}\u{200f} end\n";
    let out = run(&["segment", &file("a.txt", txt), "--mode", "paragraph"]);
    let r = rows(&out);
    assert_eq!(r.len(), 1, "{out:?}");
    assert_eq!(r[0].len(), 2, "{out:?}");
    assert_eq!(r[0][1], "Hello \\u{1b}]0;TITLE\\u{7} and \\u{1b}[2J\\u{1b}[1;1H world \\u{9b}31m c1 \\u{7f} del \\u{202e}evil\\u{2066}x\\u{2069}\\u{200f} end");

    // tag display text goes through the same escaper
    let md = "See [x\u{1b}[2Jy](http://e/) and `c\u{1b}[1Ad` here.\n";
    let out = run(&["segment", &file("a.md", md)]);
    rows(&out);
    assert!(out.contains("\\u{1b}[1A"), "{out:?}");
}

#[test]
fn wordcount_escapes_the_key_so_no_row_is_forged() {
    let json = "{\"k\\ntotal\\t0\\t0 units, 0 segments\\n\\u001b[1A\": \"one two three\"}";
    let out = run(&["wordcount", &file("a.json", json)]);
    let r = rows(&out);
    assert_eq!(r.len(), 2, "{out:?}");
    assert_eq!(r[0], ["u1", "3", "/k\\ntotal\\t0\\t0 units, 0 segments\\n\\u{1b}[1A"]);
    assert_eq!(r[1], ["total", "3", "1 units, 1 segments"]);
}

/// Ordinary text is printed as before: line breaks and tabs as `\n` and `\t`, everything else as it is.
#[test]
fn ordinary_output_is_unchanged() {
    let out = run(&["segment", &file("b.json", "{\"a\": \"Caf\u{e9} \u{4eca}\u{65e5}\\tx\\ny\"}")]);
    assert_eq!(out, "u1\tCaf\u{e9} \u{4eca}\u{65e5}\\tx\\ny\n");
    let out = run(&["wordcount", &file("c.json", "{\"menu\": {\"open\": \"Open the file\"}}")]);
    assert_eq!(out, "u1\t3\t/menu/open\ntotal\t3\t1 units, 1 segments\n");
}

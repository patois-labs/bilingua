//! Regression tests for the email fixes of security review run 2 (G5-7) and BILI-52: a body of very many
//! delimiters is refused at the part cap, and a re-wrapped plain-text paragraph keeps each line's own indent.

use bilingua_email::Email;
use bilingua_model::*;

/// A multipart body of 200,000 delimiters is refused at the part cap (the ranges past the cap are no longer
/// collected first).
#[test]
fn a_body_of_many_boundaries_is_refused_without_collecting_them() {
    let mut m = String::from("Subject: hello there\nMIME-Version: 1.0\nContent-Type: multipart/mixed; boundary=b\n\n");
    m.push_str(&"--b\n".repeat(200_000));
    match Email.extract(m.as_bytes(), &ExtractOptions::new()) {
        Err(Error::Limit(e)) => assert_eq!(e.kind, LimitKind::Units, "{e:?}"),
        other => panic!("expected a limit error, got {:?}", other.map(|d| d.units.len())),
    }
}

/// BILI-52: a paragraph whose first line is indented with a tab and whose second line is not. Its translation is
/// wrapped again; only the first line gets the tab (before, every line did).
#[test]
fn a_partly_indented_paragraph_keeps_each_lines_indent() {
    let first = "\tThis opening line is indented with one tab and runs past sixty characters";
    let body = format!("{first}\nand this second line carries on without any indent at all.\n");
    let mail = format!("Subject: hello there\nMIME-Version: 1.0\nContent-Type: text/plain; charset=utf-8\n\n{body}");
    let mut doc = Email.extract(mail.as_bytes(), &ExtractOptions::new().with_granularity(Granularity::Paragraph)).unwrap();
    let u = doc.units.iter_mut().find(|u| plain_text(&u.segments[0].source).starts_with("This opening")).expect("the paragraph");
    let words = "Diese erste Zeile ist mit einem Tabulator eingerueckt und laeuft weit ueber sechzig Zeichen hinaus, die zweite Zeile nicht.";
    u.segments[0].target = vec![Inline::text(words)];
    u.segments[0].state = SegState::Translated;
    let out = String::from_utf8(Email.merge(&doc).unwrap()).unwrap();
    let text = &out[out.find("\n\n").unwrap() + 2..];
    let lines: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
    assert!(lines.len() >= 2, "{text:?}");
    assert!(lines[0].starts_with('\t'), "{text:?}");
    assert!(lines[1..].iter().all(|l| !l.starts_with('\t')), "{text:?}");
}

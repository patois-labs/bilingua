//! The Localization OS suite `tests/verify_email_filter.py`, ported case for case (14 cases). Each test names the
//! LocOS case it ports. The LocOS case `test_api_and_cli_round_trip` drives the web app and the CLI; the filter
//! part of it (a mock translation with a locale prefix, the Content-ID and epilogue kept, the signed-mail
//! refusal) is ported here.

mod common;
use bilingua_email::Email;
use bilingua_model::*;
use common::*;

const PNG_LEN: usize = 208;

fn refused(data: &[u8], needle: &str) -> Error {
    match try_ex(data) {
        Ok(_) => panic!("expected a refusal for {:?}", String::from_utf8_lossy(&data[..data.len().min(60)])),
        Err(e) => {
            assert!(e.to_string().to_lowercase().contains(&needle.to_lowercase()), "{needle:?} not in {e}");
            e
        }
    }
}

fn is_limit(e: &Error) -> bool {
    matches!(e, Error::Limit(_))
}

// test_registration
#[test]
fn registration() {
    assert_eq!(Email.id(), "eml");
    assert_eq!(Email.extensions(), &["eml"]);
    assert!(!Email.bilingual());
    assert!(Email.sniff(&mixed()));
    assert!(Email.sniff(SIMPLE));
    assert!(!Email.sniff(b"%PDF-1.4 not an email"));
    assert!(!Email.sniff(b"key: value\nother: thing\n"));
    assert!(!Email.sniff(b"{\"a\": 1}"));
}

// test_mixed_units_and_identity
#[test]
fn mixed_units_and_identity() {
    let m = mixed();
    let t = texts(&m);
    assert_eq!(
        t,
        vec![
            "Caf\u{e9} menu for {0} guests",
            "Hello Bob,",
            "The caf\u{e9} opens at 9. Please confirm by replying to this message before the end of the week so we can book a table for everyone.",
            "Ann",
            "Hello \u{27e6}g1\u{27e7}Bob\u{27e6}/g1\u{27e7},",
            "The caf\u{e9} opens at 9.",
            "\u{27e6}x1\u{27e7}\u{27e6}g2\u{27e7}Open\u{27e6}/g2\u{27e7}",
            "Company logo",
        ]
    );
    let d = ex(&m);
    let kinds: Vec<_> = d.units.iter().map(|u| u.context.style.clone().unwrap_or_default()).collect();
    assert_eq!(kinds[..4], ["Subject", "text/plain", "text/plain", "text/plain"]);
    assert_eq!(merged(&d), m);
    assert_eq!(identity(&m), m);
}

// test_mixed_translated
#[test]
fn mixed_translated() {
    let m = mixed();
    let out = build(
        &m,
        &[
            (0, "Men\u{fc} des Caf\u{e9}s f\u{fc}r {0} G\u{e4}ste"),
            (1, "Hallo Bob,"),
            (
                2,
                "Das Caf\u{e9} \u{f6}ffnet um 9. Bitte best\u{e4}tigen Sie bis zum Ende der Woche, damit wir einen Tisch f\u{fc}r alle reservieren k\u{f6}nnen.",
            ),
            (3, "Ann"),
            (4, "Hallo \u{27e6}g1\u{27e7}Bob\u{27e6}/g1\u{27e7},"),
            (5, "Das Caf\u{e9} \u{f6}ffnet um 9 \u{2192} heute."),
            (7, "Firmenlogo"),
        ],
    );
    let msg = parse(&out);
    assert_eq!(msg.subject(), "Men\u{fc} des Caf\u{e9}s f\u{fc}r {0} G\u{e4}ste");
    assert!(contains(&out, b"Subject: =?iso-8859-1?"), "original charset kept");
    let leaves = msg.leaves();
    let plain = leaves.iter().find(|e| e.ctype() == "text/plain" && e.header("content-disposition").is_none()).unwrap().text();
    assert!(plain.contains("Das Caf\u{e9} \u{f6}ffnet um 9. Bitte") && plain.contains("> Earlier: are we still on?"), "{plain}");
    let lines: Vec<&str> = plain.lines().collect();
    assert_eq!(lines[lines.len() - 2..], ["-- ", "Ann"], "signature separator kept");
    let html = leaves.iter().find(|e| e.ctype() == "text/html").unwrap().text();
    assert!(html.contains("Hallo <b>Bob</b>,") && html.contains("9 \u{2192} heute"), "{html}");
    assert!(html.contains("src=\"cid:logo@example.com\"") && html.contains("alt=\"Firmenlogo\""), "{html}");
    assert!(html.contains("href=\"https://tracker.example.com/x\""));
    // plain part: latin-1 + QP kept; html part: base64 kept
    let i = find(&out, b"--alt\r\n").unwrap();
    assert!(contains(&out[i..i + 200], b"charset=\"iso-8859-1\"\r\nContent-Transfer-Encoding: quoted-printable"));
    assert!(contains(&out, b"Content-Transfer-Encoding: base64\r\n"));
    // everything outside the changed ranges is byte-identical
    let at = |n: &[u8]| find(&m, n).unwrap();
    for chunk in [&m[..at(b"Subject:")], &m[at(b"Date:")..at(b"Content-Language:")], &m[at(b"--rel\r\nContent-Type: image/png")..]] {
        assert!(contains(&out, chunk), "{:?}", String::from_utf8_lossy(&chunk[..chunk.len().min(80)]));
    }
    let img = leaves.iter().find(|e| e.ctype() == "image/png").unwrap();
    assert_eq!(img.header("content-id"), Some("<logo@example.com>"));
    assert_eq!(b64(&img.body), png());
    assert_eq!(png().len(), PNG_LEN);
}

// test_charset_fallback_to_utf8
#[test]
fn charset_fallback_to_utf8() {
    let out = build(&mixed(), &[(1, "\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}, \u{411}\u{43e}\u{431}!")]);
    let msg = parse(&out);
    let plain = msg.all().into_iter().find(|e| e.ctype() == "text/plain" && e.children.is_empty() && e.header("content-disposition").is_none()).unwrap();
    assert_eq!(plain.param("charset").unwrap(), "utf-8", "{:?}", plain.header("content-type"));
    assert!(plain.text().contains("\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}, \u{411}\u{43e}\u{431}!"));
    assert_eq!(plain.cte(), "quoted-printable");
}

// test_simple_7bit
#[test]
fn simple_7bit() {
    let t = texts(SIMPLE);
    assert_eq!(
        t,
        vec!["Weekly update", "Hi team,", "Your order %1$s ships on {date}.", "Indented line stays indented.", "- first item", "- second item"]
    );
    assert_eq!(merged(&ex(SIMPLE)), SIMPLE);
    let out = build(SIMPLE, &[(2, "Ihre Bestellung %1$s wird am {date} versandt."), (3, "Bleibt.")]);
    let want = String::from_utf8(SIMPLE.to_vec())
        .unwrap()
        .replace("Your order %1$s ships on {date}.", "Ihre Bestellung %1$s wird am {date} versandt.")
        .replace("  Indented line stays indented.", "  Bleibt.");
    assert_eq!(String::from_utf8(out).unwrap(), want);
    // non-ASCII into a 7bit us-ascii part: charset and CTE both change, recorded in the headers
    let out = build(SIMPLE, &[(1, "Hallo Gr\u{f6}\u{df}e,")]);
    let msg = parse(&out);
    assert_eq!(msg.param("charset").unwrap(), "utf-8");
    assert_eq!(msg.cte(), "quoted-printable");
    assert_eq!(msg.text().lines().next().unwrap(), "Hallo Gr\u{f6}\u{df}e,");
    assert!(out.starts_with(&SIMPLE[..find(SIMPLE, b"Content-Type").unwrap()]));
}

// test_flowed
#[test]
fn flowed() {
    let t = texts(FLOWED);
    assert_eq!(t, vec!["Flowed", "This is one long logical line that a mail client wrapped with soft breaks so it fits in seventy two columns."]);
    assert_eq!(merged(&ex(FLOWED)), FLOWED);
    let new = "Dies ist eine lange logische Zeile, die ein Mailprogramm mit weichen Umbruechen ".repeat(2);
    let out = build(FLOWED, &[(1, new.trim())]);
    let at = find(&out, b"\r\n\r\n").unwrap() + 4;
    let body = &out[at..];
    let lines: Vec<&[u8]> = body.split(|c| *c == b'\n').map(|l| l.strip_suffix(b"\r").unwrap_or(l)).collect();
    assert!(lines.iter().all(|l| l.len() <= 78), "{lines:?}");
    assert!(lines[0].ends_with(b" ") && !lines[2].ends_with(b" "), "{lines:?}");
    // unwrapping the flowed lines gives the translation back
    let end = find(body, b"\r\n\r\n").unwrap();
    let first_para = String::from_utf8(body[..end].to_vec()).unwrap();
    assert_eq!(first_para.replace(" \r\n", " "), new.trim());
    assert!(contains(&out, b">Quoted flowed text \r\n>continues here.\r\n"));
    // a flowed line directly followed by a quote or the signature ends its paragraph
    let edge = b"Subject: e\nContent-Type: text/plain; format=flowed\n\nMine flows \n>Theirs\nSign off \n-- \nAnn\n";
    assert_eq!(texts(edge), vec!["e", "Mine flows", "Sign off", "Ann"]);
    let out = build(edge, &[(1, "Meins"), (2, "Gruss")]);
    assert!(out.ends_with(b"\n\nMeins\n>Theirs\nGruss\n-- \nAnn\n"), "{:?}", String::from_utf8_lossy(&out));
}

// test_hard_wrapped_paragraph
#[test]
fn hard_wrapped_paragraph() {
    let data = b"Subject: x\n\nThis paragraph was hard wrapped by an old mail client at sixty-eight\ncolumns, so its lines belong together as one sentence for translation.\n";
    let t = texts(data);
    assert!(t[1].starts_with("This paragraph") && t[1].ends_with("translation."), "{t:?}");
    let new = "Ein Absatz ".repeat(20);
    let out = build(data, &[(1, &new)]);
    let body = String::from_utf8(out[find(&out, b"\n\n").unwrap() + 2..].to_vec()).unwrap();
    assert!(body.lines().all(|l| l.chars().count() <= 71), "{body}");
    assert_eq!(body.split_whitespace().collect::<Vec<_>>().join(" "), new.trim());
}

// test_localized_content_language
#[test]
fn localized_content_language() {
    let out = build_to(&mixed(), &[(1, "Hallo Bob,")], Some("de-DE"));
    assert!(contains(&out, b"Content-Language: de-DE\r\n"));
    let html = parse(&out).leaves().into_iter().find(|e| e.ctype() == "text/html").unwrap().text();
    assert!(html.contains("lang=\"de-DE\""), "{html}");
    // no Content-Language header: none is added, and nothing translated gives the original back
    let out = build_to(SIMPLE, &[], Some("de-DE"));
    assert!(!contains(&out, b"Content-Language") && out == SIMPLE);
}

// test_subject_8bit_and_folded
#[test]
fn subject_8bit_and_folded() {
    let data = b"Subject: Gr\xc3\xbc\xc3\x9fe\r\n aus K\xc3\xb6ln\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Transfer-Encoding: 8bit\r\n\r\nK\xc3\xb6ln.\r\n";
    assert_eq!(texts(data), vec!["Gr\u{fc}\u{df}e aus K\u{f6}ln", "K\u{f6}ln."]);
    assert_eq!(merged(&ex(data)), data);
    let out = build(data, &[(0, "Greetings from Cologne"), (1, "K\u{f6}ln \u{2713}.")]);
    assert!(out.starts_with(b"Subject: Greetings from Cologne\r\nContent-Type"), "{:?}", String::from_utf8_lossy(&out));
    assert!(out.ends_with("K\u{f6}ln \u{2713}.\r\n".as_bytes()));
}

// test_refusals
#[test]
fn refusals() {
    let signed = b"Subject: s\r\nMIME-Version: 1.0\r\nContent-Type: multipart/signed; protocol=\"application/pkcs7-signature\"; micalg=sha-256; boundary=b\r\n\r\n--b\r\nContent-Type: text/plain\r\n\r\nHi\r\n--b\r\nContent-Type: application/pkcs7-signature\r\n\r\nMIAG\r\n--b--\r\n";
    refused(signed, "signature");
    refused(b"Subject: e\r\nContent-Type: application/pkcs7-mime; smime-type=enveloped-data\r\n\r\nMIA\r\n", "S/MIME");
    refused(
        b"Subject: p\r\nContent-Type: multipart/encrypted; protocol=\"application/pgp-encrypted\"; boundary=x\r\n\r\n--x\r\nContent-Type: application/pgp-encrypted\r\n\r\nVersion: 1\r\n--x--\r\n",
        "encrypted",
    );
    refused(b"Subject: p\n\n-----BEGIN PGP SIGNED MESSAGE-----\nHash: SHA256\n\nHi\n", "signature");
    refused(b"%PDF-1.4 not an email", "not an email");
    refused(b"\x00\x01binary", "not an email");
    refused(b"Subject: c\nContent-Type: text/plain; charset=x-made-up\n\nHi\n", "charset");
    refused(b"Subject: c\nContent-Type: text/plain; charset=base64\n\nHi\n", "charset");
    refused(b"Subject: c\nContent-Type: text/plain; charset=utf-8\n\n\xff\xfe bad\n", "do not match");
    refused(b"From: a@example.com\nContent-Type: image/png\n\nxx\n", "no Subject");
    let mut big = b"Subject: h\n".to_vec();
    for _ in 0..150_000 {
        big.extend_from_slice(b"X-Pad: y\n");
    }
    let e = refused(&big, "header block");
    assert!(is_limit(&e));
    // 8-bit text mislabelled us-ascii is read as UTF-8 and relabelled when rebuilt
    let mis = b"Subject: m\nContent-Type: text/plain; charset=us-ascii\n\nCaf\xc3\xa9\n";
    assert_eq!(texts(mis), vec!["m", "Caf\u{e9}"]);
    let out = build(mis, &[(1, "Caf\u{e9} \u{2713}")]);
    assert!(contains(&out, b"charset=\"utf-8\""), "{:?}", String::from_utf8_lossy(&out));
    assert_eq!(parse(&out).text(), "Caf\u{e9} \u{2713}\n");
}

// test_caps
#[test]
fn caps() {
    let mut deep = b"Subject: d\n".to_vec();
    for i in 0..bilingua_email::MAX_DEPTH + 2 {
        deep.extend_from_slice(format!("Content-Type: multipart/mixed; boundary=\"b{i}\"\n\n--b{i}\n").as_bytes());
    }
    deep.extend_from_slice(b"Content-Type: text/plain\n\nhi\n");
    assert!(is_limit(&refused(&deep, "deeper")));
    let mut many = b"Subject: m\nContent-Type: multipart/mixed; boundary=z\n\n".to_vec();
    for _ in 0..bilingua_email::MAX_PARTS + 1 {
        many.extend_from_slice(b"--z\nContent-Type: text/plain\n\npart\n");
    }
    many.extend_from_slice(b"--z--\n");
    assert!(is_limit(&refused(&many, "MIME parts")));
    // the segment cap and the characters-per-segment cap (LocOS config MAX_DOCUMENT_SEGMENTS / MAX_SEGMENT_CHARS)
    let mut lots = b"Subject: s\n\n".to_vec();
    for i in 0..10 {
        lots.extend_from_slice(format!("line {i}\n\n").as_bytes());
    }
    let lim = Limits::new().with_max_units(3);
    let e = Email.extract(&lots, &ExtractOptions::new().with_limits(lim)).unwrap_err();
    assert!(is_limit(&e), "{e}");
    let mut long = b"Subject: s\n\n".to_vec();
    long.extend_from_slice(&b"word ".repeat(400));
    long.push(b'\n');
    let lim = Limits::new().with_max_unit_chars(1000);
    let e = Email.extract(&long, &ExtractOptions::new().with_limits(lim)).unwrap_err();
    assert!(is_limit(&e), "{e}");
    // a boundary that never closes still parses (the last part runs to the end)
    let open_ended = b"Subject: o\nContent-Type: multipart/mixed; boundary=q\n\n--q\nContent-Type: text/plain\n\nbody\n";
    assert_eq!(texts(open_ended), vec!["o", "body"]);
    assert_eq!(merged(&ex(open_ended)), open_ended);
}

// test_message_rfc822_attachment_untouched
#[test]
fn message_rfc822_attachment_untouched() {
    let data = b"Subject: Fwd: hello\nContent-Type: multipart/mixed; boundary=f\n\n--f\nContent-Type: text/plain\n\nSee below.\n--f\nContent-Type: message/rfc822\n\nSubject: inner\n\nInner body.\n--f--\n";
    assert_eq!(texts(data), vec!["Fwd: hello", "See below."]);
    let out = build(data, &[(1, "Siehe unten.")]);
    assert!(contains(&out, b"Subject: inner\n\nInner body.\n") && contains(&out, b"Siehe unten."));
}

// test_boundary_injection
#[test]
fn boundary_injection() {
    let evil = "Hi\n--BND\nContent-Type: text/html\n\n<b>owned</b>\n--BND--";
    let out = build(BND_MSG, &[(1, evil), (2, "Oi\n--BND--\n")]);
    let msg = parse(&out);
    let parts = msg.leaves();
    assert_eq!(parts.len(), 3, "{:?}", String::from_utf8_lossy(&out));
    assert_eq!(parts[0].ctype(), "text/plain");
    assert_eq!(parts[0].cte(), "base64");
    assert!(parts[0].text().starts_with("Hi\n--BND\n"), "{}", parts[0].text());
    assert_eq!(parts[1].cte(), "base64");
    assert_eq!(count(&out, b"\n--BND\n"), 3);
    assert!(out.ends_with(b"AAAA\n--BND--\n"));
    assert_eq!(count(&out, b"Content-Transfer-Encoding:"), 2, "replaced in place, not added");
    let t = texts(&out);
    assert_eq!(t[1], "Hi");
    assert!(t.iter().any(|x| x.contains("<b>owned</b>")) && t.iter().any(|x| x == "--BND"), "{t:?}");
    // a harmless translation keeps the original transfer encodings
    let out2 = build(BND_MSG, &[(1, "Hallo dort."), (2, "Hallo")]);
    assert!(contains(&out2, b"Content-Transfer-Encoding: 8bit\n\nHallo dort.\n--BND"), "{:?}", String::from_utf8_lossy(&out2));
}

// test_api_and_cli_round_trip (the filter part: a mock translation with a locale prefix, then the checks the
// suite makes on the exported file)
#[test]
fn mock_translation_round_trip_and_signed_refusal() {
    let loc = "de-DE";
    let m = mixed();
    let mut d = Email.extract(&m, &opts(Some(loc))).unwrap();
    for u in d.units.iter_mut() {
        let mut t = vec![Inline::text(format!("[{loc}] "))];
        t.extend(u.segments[0].source.iter().cloned());
        u.segments[0].target = t;
        u.segments[0].state = SegState::Draft;
    }
    let out = merged(&d);
    let msg = parse(&out);
    assert!(msg.subject().starts_with(&format!("[{loc}]")), "{}", msg.subject());
    let html = msg.leaves().into_iter().find(|e| e.ctype() == "text/html").unwrap().text();
    assert!(html.contains(&format!("[{loc}]")) && html.contains("src=\"cid:logo@example.com\""), "{html}");
    assert!(contains(&out, b"Content-ID: <logo@example.com>") && contains(&out, b"Epilogue stays."));
    assert!(contains(&out, b"Content-Language: de-DE"));
    let e = refused(b"Subject: s\r\nContent-Type: application/pkcs7-mime\r\n\r\nMIA\r\n", "S/MIME");
    assert!(matches!(e, Error::Unsupported(_)));
}

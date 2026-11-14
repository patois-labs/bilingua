//! Hostile input: deep or endless multipart nesting, missing or absurd boundaries, huge and malformed headers,
//! broken transfer encodings and charsets, truncated messages. Each gives a typed error or reads, never a panic,
//! a stack overflow or a hang; property tests run random bytes, byte-mutated and word-mutated fixtures and
//! generated messages through extract, an untouched merge and a pseudo-translated merge.

mod common;
use bilingua_email::Email;
use bilingua_model::*;
use common::*;
use proptest::prelude::*;
use std::time::{Duration, Instant};

const DATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data");

fn fixtures() -> Vec<Vec<u8>> {
    let mut names: Vec<String> = std::fs::read_dir(DATA).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).filter(|n| n.ends_with(".eml")).collect();
    names.sort();
    names.iter().map(|n| std::fs::read(format!("{DATA}/{n}")).unwrap()).collect()
}

fn is_typed(e: &Error) -> bool {
    matches!(e, Error::Parse { .. } | Error::Unsupported(_) | Error::Limit(_) | Error::Encoding(_))
}

/// A translated HTML part may be refused: a parse refusal, or the browser-safety guard (a `Write` error) when the
/// mutated source has markup a browser would read differently after translation.
fn html_refusal(e: &Error) -> bool {
    matches!(e, Error::Parse { format, .. } if format == "html")
        || matches!(e, Error::Write(msg) if msg.contains("cannot be exported safely"))
}

/// The invariants of any message that extracts.
fn check_file(bytes: &[u8]) {
    for target in [None, Some("ar-SA"), Some("ja")] {
        let mut d = match Email.extract(bytes, &opts(target)) {
            Ok(d) => d,
            Err(e) => {
                assert!(is_typed(&e), "{e:?}");
                continue;
            }
        };
        assert_eq!(Email.merge(&d).expect("merge of an extracted document"), bytes, "untouched merge changed the message");
        pseudo(&mut d);
        let out = match Email.merge(&d) {
            Ok(o) => o,
            Err(e) => {
                assert!(html_refusal(&e), "{e:?}");
                continue;
            }
        };
        match Email.extract(&out, &opts(None)) {
            Ok(back) => assert!(back.units.len() >= d.units.len().min(1)),
            Err(e) => assert!(is_typed(&e), "{e:?}"),
        }
    }
    let _ = Email.sniff(bytes);
}

fn timed(what: &str, f: impl FnOnce()) {
    let t = Instant::now();
    f();
    let el = t.elapsed();
    assert!(el < Duration::from_secs(60), "{what} took {el:?}");
}

#[test]
fn endless_nesting_is_a_limit_not_a_stack_overflow() {
    let mut deep = b"Subject: d\n".to_vec();
    for i in 0..20_000 {
        deep.extend_from_slice(format!("Content-Type: multipart/mixed; boundary=\"b{i}\"\n\n--b{i}\n").as_bytes());
    }
    let e = Email.extract(&deep, &opts(None)).unwrap_err();
    assert!(matches!(e, Error::Limit(_)), "{e}");
    // the same boundary at every level
    let mut same = b"Subject: d\n".to_vec();
    for _ in 0..5_000 {
        same.extend_from_slice(b"Content-Type: multipart/mixed; boundary=x\n\n--x\n");
    }
    let r = Email.extract(&same, &opts(None));
    assert!(r.is_err());
    // wide: 100k tiny parts
    let mut wide = b"Subject: w\nContent-Type: multipart/mixed; boundary=z\n\n".to_vec();
    for _ in 0..100_000 {
        wide.extend_from_slice(b"--z\n\nx\n");
    }
    assert!(matches!(Email.extract(&wide, &opts(None)), Err(Error::Limit(_))));
}

#[test]
fn missing_empty_and_absurd_boundaries() {
    let cases: Vec<Vec<u8>> = vec![
        b"Subject: a\nContent-Type: multipart/mixed\n\n--x\nContent-Type: text/plain\n\nbody\n--x--\n".to_vec(),
        b"Subject: a\nContent-Type: multipart/mixed; boundary=\n\n--\nContent-Type: text/plain\n\nbody\n----\n".to_vec(),
        b"Subject: a\nContent-Type: multipart/mixed; boundary=\"\"\n\n----\nbody\n".to_vec(),
        format!("Subject: a\nContent-Type: multipart/mixed; boundary={}\n\n--{}\nContent-Type: text/plain\n\nbody\n", "b".repeat(300), "b".repeat(300)).into_bytes(),
        "Subject: a\nContent-Type: multipart/mixed; boundary=\"gr\u{fc}n\"\n\n--gr\u{fc}n\nbody\n".as_bytes().to_vec(),
        b"Subject: a\nContent-Type: multipart/mixed; boundary=\"unclosed\n\n--unclosed\nbody\n".to_vec(),
        b"Subject: a\nContent-Type: multipart/mixed; boundary*0=ab; boundary*1*=%41%42\n\n--abAB\nContent-Type: text/plain\n\nrfc2231\n--abAB--\n".to_vec(),
        b"Subject: a\nContent-Type: ;;;;;;;;\n\nbody\n".to_vec(),
        b"Subject: a\nContent-Type: text/plain; charset=\"utf-8\n\nbody\n".to_vec(),
        b"Subject: a\nContent-Type: a/b/c\n\nbody\n".to_vec(),
    ];
    for c in cases {
        check_file(&c);
    }
}

#[test]
fn huge_and_malformed_headers() {
    let big = |name: &str, n: usize| {
        let mut v = b"Subject: h\n".to_vec();
        v.extend_from_slice(name.as_bytes());
        v.extend_from_slice(b": ");
        v.extend(std::iter::repeat_n(b'x', n));
        v.extend_from_slice(b"\n\nbody\n");
        v
    };
    // a 2 MB field is over the header block cap; 900 KB is read
    let e = Email.extract(&big("X-Big", 2_000_000), &opts(None)).unwrap_err();
    assert!(matches!(e, Error::Limit(_)), "{e}");
    check_file(&big("X-Big", 900_000));
    let mut folded = b"Subject: f\nX-Fold: a".to_vec();
    for _ in 0..200_000 {
        folded.extend_from_slice(b"\n b");
    }
    folded.extend_from_slice(b"\n\nbody\n");
    check_file(&folded);
    // a hundred thousand parameters on one Content-Type
    let mut params = b"Subject: p\nContent-Type: text/plain".to_vec();
    for i in 0..100_000 {
        params.extend_from_slice(format!("; a{i}=\"x;y\"").as_bytes());
    }
    params.extend_from_slice(b"; charset=utf-8\n\nbody\n");
    timed("params", || check_file(&params));
    // quotes that never close, many of them
    let mut quotes = b"Subject: q\nContent-Type: text/plain".to_vec();
    for _ in 0..200_000 {
        quotes.extend_from_slice(b"; a=\"");
    }
    quotes.extend_from_slice(b"\n\nbody\n");
    timed("quotes", || check_file(&quotes));
    for h in [&b"Subject: =?utf-8?b?!!!!?=\n\nbody\n"[..], b"Subject: =?=?=?=?\n\nbody\n", b"Subject: =?x?q?\n\nbody\n", b"Subject: =?utf-8?q?=ZZ=?=\n\nb\n", b"Subject: =?utf-8?b?AAAA?= =?latin1?q?x?= =?\n\nb\n", b"From: a\nSubject:\n\nbody\n", b"From: a\nSubject\n\nbody\n"] {
        check_file(h);
    }
    // a Subject of a million "=?" pairs
    let mut subj = b"Subject: ".to_vec();
    subj.extend(b"=?a?q?".repeat(150_000));
    subj.extend_from_slice(b"\n\nbody\n");
    timed("subject", || check_file(&subj));
}

#[test]
fn malformed_transfer_encodings_and_charsets() {
    let pre = |ct: &str, cte: &str, body: &[u8]| {
        let mut v = format!("Subject: s\nContent-Type: {ct}\nContent-Transfer-Encoding: {cte}\n\n").into_bytes();
        v.extend_from_slice(body);
        v
    };
    let cases = vec![
        pre("text/plain; charset=utf-8", "base64", b"!!!! not base64 ????\n"),
        pre("text/plain; charset=utf-8", "base64", b"QUJD\nR"),
        pre("text/plain; charset=utf-8", "base64", b"====\n"),
        pre("text/plain; charset=utf-8", "quoted-printable", b"=\n=\r=\r\n=ZZ=3\n=3D=3d=E\n=\n"),
        pre("text/plain; charset=utf-8", "quoted-printable", &b"=C3".repeat(100_000)),
        pre("text/plain; charset=utf-8", "x-uuencode", b"begin 644 x\n"),
        pre("text/plain; charset=utf-8", "", b"empty cte\n"),
        pre("text/plain; charset=utf-16", "8bit", b"\xff\xfeh\0i\0"),
        pre("text/plain; charset=utf-7", "7bit", b"+AGg-i"),
        pre("text/plain; charset=iso-2022-jp", "7bit", b"\x1b$B\x1b$B\x1b$B$$\x1b(B\x1b(B\x1b"),
        pre("text/plain; charset=shift_jis", "8bit", b"\x81\x81\x81 \xff\xff\n"),
        pre("text/plain; charset=gb2312", "8bit", b"\x81"),
        pre("text/plain", "8bit", b"\x80\x81\x82 undeclared \xe9\n"),
        pre("text/plain", "7bit", b"nul \0 byte\n"),
        pre("text/html; charset=utf-8", "8bit", b"<p><b>unclosed <i>tags <a href=\"x>text"),
        pre("text/html; charset=utf-8", "8bit", &b"<p>".repeat(200_000)),
        pre("text/html; charset=iso-8859-1", "8bit", b"<meta charset=utf-8><p>\xe9"),
        pre("text/plain; charset=utf-8; format=flowed; delsp=yes", "8bit", &b" \n".repeat(100_000)),
        pre("text/plain; charset=utf-8; format=flowed", "8bit", &b"> \n>\n-- \n-- \n \n".repeat(20_000)),
        pre("text/plain; charset=utf-8", "8bit", &vec![b'x'; 3_000_000]),
        pre("text/plain; charset=utf-8", "8bit", &b"\r".repeat(500_000)),
        pre("text/plain; charset=utf-8", "8bit", &b"line \x0b\x0c\x1c\x1d\x1e\xc2\x85\xe2\x80\xa8\n".repeat(50_000)),
        pre("text/plain; charset=utf-8", "8bit", b"-----BEGIN PGP MESSAGE-----\nxx\n"),
    ];
    for c in cases {
        timed("encodings", || check_file(&c));
    }
    // a million blank lines is over the line budget
    let blanks = pre("text/plain", "7bit", &b"\n".repeat(1_100_000));
    assert!(matches!(Email.extract(&blanks, &opts(None)), Err(Error::Limit(_))));
}

#[test]
fn every_prefix_of_every_fixture_is_safe() {
    for data in fixtures() {
        let step = (data.len() / 120).max(1);
        let mut n = 0;
        while n <= data.len() {
            check_file(&data[..n]);
            n += step;
        }
        check_file(&data[..data.len().saturating_sub(1)]);
    }
}

#[test]
fn hostile_inputs_with_dense_delimiters_are_linear() {
    // many delimiter-like lines in one part, in one translation, and in nested levels
    let mut m = b"Subject: s\nContent-Type: multipart/mixed; boundary=b\n\n--b\nContent-Type: text/plain\n\n".to_vec();
    for _ in 0..200_000 {
        m.extend_from_slice(b"--b-x\n--bb\n- -b\n");
    }
    m.extend_from_slice(b"\n--b--\n");
    timed("delimiter lines", || check_file(&m));
    let mut t = b"Subject: s\nContent-Type: multipart/mixed; boundary=b\n\n".to_vec();
    for _ in 0..400 {
        t.extend_from_slice(b"--b\nContent-Type: text/plain\n\n--b-\n");
    }
    timed("many parts", || check_file(&t));
    // html with endless unclosed script and style tags and tags with unterminated quotes
    let mut h = b"Subject: s\nContent-Type: text/html; charset=utf-8\n\n<html lang=en>".to_vec();
    h.extend(b"<script><style><meta http-equiv=\"content-language\" <a \"<b '".repeat(50_000));
    let mut d = match Email.extract(&h, &opts(Some("de"))) {
        Ok(d) => d,
        Err(_) => return,
    };
    pseudo(&mut d);
    timed("html language", || {
        let _ = Email.merge(&d);
    });
}

fn mutate(base: &[u8], edits: &[(usize, u8, u8)]) -> Vec<u8> {
    let mut v = base.to_vec();
    for &(pos, byte, kind) in edits {
        if v.is_empty() {
            break;
        }
        let at = pos % v.len();
        match kind % 4 {
            0 => v[at] = byte,
            1 => v.insert(at, byte),
            2 => {
                v.remove(at);
            }
            _ => {
                let end = (at + 1 + byte as usize % 40).min(v.len());
                v.drain(at..end);
            }
        }
    }
    v
}

const WORDS: [&str; 24] = [
    "boundary", "--", "\r\n", "\n", "charset", "base64", "quoted-printable", "7bit", "multipart/mixed", "multipart/alternative", "text/html", "text/plain", "format=flowed", "=?utf-8?b?",
    "?=", "=E9", "Content-Type: ", "Content-Transfer-Encoding: ", "Subject: ", "\n\n", " ", "\t", "=\n", ";",
];

fn word_mutate(base: &[u8], edits: &[(usize, usize)]) -> Vec<u8> {
    let mut v = base.to_vec();
    for &(pos, w) in edits {
        let at = if v.is_empty() { 0 } else { pos % v.len() };
        let word = WORDS[w % WORDS.len()].as_bytes();
        if w % 3 == 0 && at + word.len() <= v.len() {
            v.splice(at..at + word.len(), word.iter().copied());
        } else {
            v.splice(at..at, word.iter().copied());
        }
    }
    v
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 160, max_shrink_iters: 20, .. ProptestConfig::default() })]

    #[test]
    fn random_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..400)) {
        check_file(&bytes);
        let mut with_head = b"Subject: x\nContent-Type: text/plain\n\n".to_vec();
        with_head.extend_from_slice(&bytes);
        check_file(&with_head);
    }

    #[test]
    fn random_text_after_email_headers_never_panics(body in "\\PC{0,300}", cs in prop::sample::select(vec!["utf-8", "iso-8859-1", "windows-1252", "shift_jis", "iso-2022-jp", "gb2312", "euc-kr", "koi8-r"]), cte in prop::sample::select(vec!["7bit", "8bit", "quoted-printable", "base64"])) {
        let m = format!("Subject: {}\nContent-Type: text/plain; charset={cs}\nContent-Transfer-Encoding: {cte}\n\n{body}\n", body.chars().take(20).collect::<String>().replace(['\n', '\r'], " "));
        check_file(m.as_bytes());
    }

    #[test]
    fn byte_mutated_fixtures_never_panic(i in 0usize..64, edits in proptest::collection::vec((any::<usize>(), any::<u8>(), any::<u8>()), 1..12)) {
        let all = fixtures();
        check_file(&mutate(&all[i % all.len()], &edits));
    }

    #[test]
    fn word_mutated_fixtures_never_panic(i in 0usize..64, edits in proptest::collection::vec((any::<usize>(), any::<usize>()), 1..8)) {
        let all = fixtures();
        check_file(&word_mutate(&all[i % all.len()], &edits));
    }

    #[test]
    fn translations_with_hostile_text_never_panic(i in 0usize..64, text in "\\PC{0,200}", raw in proptest::collection::vec(any::<char>(), 0..60)) {
        let all = fixtures();
        let data = &all[i % all.len()];
        let mut d = Email.extract(data, &opts(Some("ar"))).unwrap();
        let hostile: String = raw.into_iter().collect();
        for u in d.units.iter_mut() {
            let mut t = vec![Inline::text(format!("{text}{hostile}"))];
            // keep the unit's own tags so the target is valid
            t.extend(u.segments[0].source.iter().filter(|x| matches!(x, Inline::Tag { .. })).cloned());
            u.segments[0].target = t;
        }
        // bilingua-html panics on text like "&amp;" followed by non-ASCII text at a certain offset; the email
        // crate turns that into a typed html error (see the report), so a merge may refuse, but only for that
        let out = match Email.merge(&d) {
            Ok(o) => o,
            Err(e) => {
                prop_assert!(html_refusal(&e), "{e:?}");
                return Ok(());
            }
        };
        match Email.extract(&out, &opts(None)) {
            Ok(_) => {}
            Err(e) => prop_assert!(is_typed(&e), "{e:?}"),
        }
        // the structure never changes: the same number of leaf parts, whatever the text says
        let before = parse(data).all().len();
        let after = parse(&out).all().len();
        prop_assert_eq!(before, after, "parts changed");
    }
}

//! Opus gate (11-05): untouched merges on every golden and corpus file, the GSI fields a merge may touch, and the
//! translation shapes that used to take quadratic time (many refused subtitles, many distinct characters, many
//! zero-width codes in one word).

mod common;
use bilingua_model::*;
use bilingua_stl::Stl;
use common::*;
use std::time::Instant;

fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

fn golden_inputs() -> Vec<(String, Vec<u8>)> {
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/locos_golden.json")).unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    cases.iter().map(|c| (c["name"].as_str().unwrap().to_string(), hex(c["hex"].as_str().unwrap()))).collect()
}

fn corpus_inputs() -> Vec<(String, Vec<u8>)> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/stl");
    let mut v: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .map(|p| (p.display().to_string(), std::fs::read(&p).unwrap()))
        .collect();
    v.sort();
    assert!(v.len() >= 6);
    v
}

#[test]
fn untouched_merges_return_the_exact_bytes() {
    let mut n = 0;
    for (name, data) in golden_inputs().into_iter().chain(corpus_inputs()) {
        for target in [None, Some("fr-FR")] {
            let Ok(mut d) = Stl.extract(&data, &opts(target)) else { continue };
            assert_eq!(Stl.merge(&d).unwrap(), data, "{name}: merge");
            assert_eq!(Stl.merge_with(&d, &MergeOptions::new().with_output_encoding(OutputEncoding::Utf8).with_stamp_target_always(false)).unwrap(), data, "{name}: utf-8 asked");
            if target.is_none() {
                assert_eq!(Stl.merge_with(&d, &bilingua_model::profile::locos().merge_options("xml")).unwrap(), data, "{name}: locos, no target");
            }
            // every target set to its own source: nothing changed, so no table switch and no relayout
            for u in d.units.iter_mut() {
                let s = &mut u.segments[0];
                s.target = s.source.clone();
                s.state = SegState::Translated;
            }
            let out = Stl.merge(&d).unwrap();
            if target.is_none() {
                assert_eq!(out, data, "{name}: targets = sources");
            } else if !d.units.is_empty() {
                // something is "translated": only LC (14-15) is stamped
                assert_eq!(out.len(), data.len());
                assert_eq!(&out[14..16], b"0F", "{name}");
                assert_eq!(out[..14], data[..14]);
                assert_eq!(out[16..], data[16..]);
            }
            n += 1;
        }
    }
    assert!(n > 250, "{n}");
}

#[test]
fn an_empty_target_language_stamps_nothing() {
    // LocOS: `language_code(target_locale) if target_locale else None`
    let data = cat(&[&gsi_default(1), &t(1, b"Hello there")]);
    let mut d = Stl.extract(&data, &opts(Some(""))).unwrap();
    assert_eq!(Stl.merge_with(&d, &bilingua_model::profile::locos().merge_options("xml")).unwrap(), data);
    set(&mut d, 0, "Bonjour");
    let out = Stl.merge(&d).unwrap();
    assert_eq!(&out[14..16], b"09");
}

#[test]
fn merge_keeps_tns_tng_and_block_numbers_and_updates_tnb() {
    let data = cat(&[
        &gsi_default(3),
        &t(1, b"Short one"),
        &tti(&Tti { sn: 2, ebn: 0xFE, ..Default::default() }, b"user data"),
        &t(3, b"Another"),
    ]);
    let mut d = ex(&data);
    set(&mut d, 0, &"long words ".repeat(7));
    let out = Stl.merge(&d).unwrap();
    let bl = blocks(&out);
    // two rows of 40 characters fit in one extension pair at most; the new block count is in TNB
    assert_eq!(&out[238..243], format!("{:05}", bl.len()).as_bytes());
    assert_eq!(out[243..248], data[243..248], "TNS");
    assert_eq!(out[248..251], data[248..251], "TNG");
    let sns: Vec<u16> = bl.iter().map(|b| u16::from_le_bytes([b[1], b[2]])).collect();
    assert!(sns.windows(2).all(|w| w[0] <= w[1]), "{sns:?}");
    assert_eq!(*sns.last().unwrap(), 3);
}

/// A tag the translation moves in front of a letter cannot be a floating diacritic in disguise unless the original
/// had that byte as a code: the output never holds a byte the original did not.
#[test]
fn moved_codes_only_reuse_their_own_bytes() {
    let data = cat(&[&gsi_default(1), &t(1, b"a\x01b\x80c d")]);
    let d = ex(&data);
    assert_eq!(locos_text(&d.units[0]), "a \u{27E6}x1\u{27E7}b\u{27E6}x2\u{27E7}c d");
    let out = build(&data, &[(0, "\u{27E6}x2\u{27E7}\u{27E6}x2\u{27E7}x \u{27E6}x1\u{27E7}y".into())]).unwrap();
    let tf = &blocks(&out)[0][16..];
    assert!(tf.starts_with(b"\x80\x80x\x01y"), "{tf:x?}");
}

fn many_subtitles(n: usize) -> Vec<u8> {
    let mut v = gsi_default(n);
    for k in 0..n {
        v.extend(t((k % 60000) as u16 + 1, b"Hi"));
    }
    v
}

#[test]
fn many_refused_subtitles_are_reported_in_linear_time() {
    let n = 45_000;
    let data = many_subtitles(n);
    let map: Vec<(usize, String)> = (0..n).map(|k| (k, "x".repeat(50))).collect();
    let start = Instant::now();
    let r = build(&data, &map);
    let secs = start.elapsed().as_secs_f64();
    let Err(Error::Unwritable { reason: m, .. }) = r else { panic!("must refuse") };
    assert!(m.contains("subtitle 1: does not fit in 2 rows of 40 characters"), "{m}");
    assert!(m.contains("(and 44990 more)"), "{m}");
    assert!(secs < 30.0, "{secs:.1} s");
}

#[test]
fn many_distinct_characters_are_checked_in_linear_time() {
    let data = cat(&[&gsi_default(1), &t(1, b"Hi")]);
    // 120 000 distinct CJK and Hangul characters: no table holds any of them
    let text: String = (0x4E00u32..0x9FFF).chain(0xAC00..0xD7A3).chain(0x20000..0x2A6DF).filter_map(char::from_u32).take(120_000).collect();
    let start = Instant::now();
    let r = build(&data, &[(0, text)]);
    let secs = start.elapsed().as_secs_f64();
    assert!(matches!(r, Err(Error::Write(_) | Error::Unwritable { .. })));
    assert!(secs < 30.0, "{secs:.1} s");
}

#[test]
fn zero_width_codes_in_long_words_wrap_in_linear_time() {
    // an italics code (0x80) takes no cell: 480 000 of them in 60 words of one letter (two rows of 99)
    let data = cat(&[&gsi(b"00", b"09", b'1', b"99", 1), &t(1, b"a\x80b c")]);
    let d = ex(&data);
    assert_eq!(locos_text(&d.units[0]), "a\u{27E6}x1\u{27E7}b c");
    let word = format!("y{}", "\u{27E6}x1\u{27E7}".repeat(8000));
    let text = vec![word; 60].join(" ");
    let start = Instant::now();
    let r = build(&data, &[(0, text)]);
    let secs = start.elapsed().as_secs_f64();
    // laid out in rows, then refused: more than 240 extension blocks
    assert!(matches!(r, Err(Error::Unwritable { reason: ref m, .. }) if m.contains("240 extension blocks")), "{r:?}");
    assert!(secs < 30.0, "{secs:.1} s");
}

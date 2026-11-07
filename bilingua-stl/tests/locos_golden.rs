//! LocOS golden: 138 files (the suite's fixtures, hostile files and random generated ones) with what the LocOS
//! STL filter parses and builds, made by `tests/tools/gen_golden.py`. Units, build bytes (by SHA-256) and
//! refusal messages must be equal. Set `STL_GOLDEN_DEBUG=dir` to save the files that differ.

mod common;
use bilingua_model::*;
use bilingua_stl::Stl;
use common::*;
use std::collections::BTreeMap;

fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

/// A plain SHA-256 so the test needs no crate.
pub fn sha256(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be,
        0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa,
        0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85,
        0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
        0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f,
        0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[4 * i], chunk[4 * i + 1], chunk[4 * i + 2], chunk[4 * i + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut a = h;
        for i in 0..64 {
            let s1 = a[4].rotate_right(6) ^ a[4].rotate_right(11) ^ a[4].rotate_right(25);
            let ch = (a[4] & a[5]) ^ (!a[4] & a[6]);
            let t1 = a[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a[0].rotate_right(2) ^ a[0].rotate_right(13) ^ a[0].rotate_right(22);
            let maj = (a[0] & a[1]) ^ (a[0] & a[2]) ^ (a[1] & a[2]);
            let t2 = s0.wrapping_add(maj);
            a = [t1.wrapping_add(t2), a[0], a[1], a[2], a[3].wrapping_add(t1), a[4], a[5], a[6]];
        }
        for i in 0..8 {
            h[i] = h[i].wrapping_add(a[i]);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

fn mapping(m: &serde_json::Value) -> Vec<(usize, String)> {
    let mut v: Vec<(usize, String)> = m.as_object().unwrap().iter().map(|(k, t)| (k.parse().unwrap(), t.as_str().unwrap().to_string())).collect();
    v.sort();
    v
}

/// The code tables, read from the crate's generated table file (the test keeps its own decoder).
#[allow(dead_code)]
#[path = "../src/tables.rs"]
mod tables;

/// byte -> character, for table `n` (0-4); floating diacritics of table 00 have none here.
fn chars(n: u8) -> [Option<char>; 256] {
    let mut d = [None; 256];
    for b in 0x20u8..0x7F {
        d[b as usize] = Some(b as char);
    }
    if n == 0 {
        for &(b, c) in tables::T00_HIGH {
            d[b as usize] = Some(c);
        }
    } else {
        let high = [&tables::HIGH_01, &tables::HIGH_02, &tables::HIGH_03, &tables::HIGH_04][n as usize - 1];
        for (i, c) in high.iter().enumerate() {
            d[0xA0 + i] = (*c != '\0').then_some(*c);
        }
    }
    d
}

/// The only accepted differences from LocOS (issue 798): a code table switch LocOS makes (and writes, or refuses for
/// another reason such as row length), refused by Bilingua with its table-switch message, where the file really is
/// the case the message names:
/// - "hold bytes with no equivalent": the subtitle text holds a byte the old table has no character for (or a table
///   00 floating diacritic) that the new table reads as a character. LocOS copies it raw and it turns into a letter.
/// - "comment subtitles hold characters": the file has a comment subtitle with upper-half bytes (LocOS leaves the
///   comment in the old table, so it reads as other letters under the new table).
///
/// Any other refusal, and any refusal of a file that is not such a case, still counts as a difference.
fn switch_refused_on_purpose(got: &Result<Vec<u8>, Error>, want: &serde_json::Value, input: &[u8]) -> bool {
    let Err(Error::Unwritable { reason, .. }) = got else { return false };
    let head = "The EBU STL file was not exported: the translation needs code table 0";
    if want.get("err").is_some_and(|e| e != "CaptionTargetError") || !reason.starts_with(head) {
        return false;
    }
    let to = reason.as_bytes()[head.len()].wrapping_sub(b'0');
    let from = match &input[12..14] {
        [a, b] if a.is_ascii_whitespace() && b.is_ascii_whitespace() => 0,
        [b'0', d @ b'0'..=b'4'] => d - b'0',
        _ => return false,
    };
    if to > 4 || to == from {
        return false;
    }
    let (old, new) = (chars(from), chars(to));
    let diacritic = |b: u8| tables::DIACRITICS.iter().any(|(x, _)| *x == b);
    // a byte the old table reads as no character (a control code here) that the new table reads as a character
    let turns = |b: u8| (old[b as usize].is_none() || (from == 0 && diacritic(b))) && (new[b as usize].is_some() || (to == 0 && diacritic(b)));
    // the subtitles as the crate groups them: (first block, comment, text field)
    let mut groups: Vec<(usize, bool, Vec<u8>)> = Vec::new();
    let mut open = false;
    for (i, blk) in input[1024..].chunks(128).enumerate() {
        if blk.len() < 128 || blk[3] == 0xFE || (0xF0..=0xFD).contains(&blk[3]) {
            continue;
        }
        let first_sn = groups.last().map(|g| &input[1024 + g.0 * 128 + 1..1024 + g.0 * 128 + 3]);
        if open && first_sn == Some(&blk[1..3]) {
            groups.last_mut().unwrap().2.extend_from_slice(&blk[16..]);
        } else {
            groups.push((i, blk[15] == 1, blk[16..].to_vec()));
        }
        open = blk[3] != 0xFF;
    }
    // units: subtitles that are not comments and have a character other than a space (a floating mark counts)
    let has_text = |tf: &[u8]| {
        tf.iter().enumerate().any(|(i, &b)| match old[b as usize] {
            Some(c) => !c.is_whitespace(),
            None => from == 0 && diacritic(b) && tf.get(i + 1).is_some_and(|n| *n < 0x80 && old[*n as usize].is_some()),
        })
    };
    let units: Vec<&[u8]> = groups.iter().filter(|g| !g.1 && has_text(&g.2)).map(|g| g.2.as_slice()).collect();
    if Stl.extract(input, &opts(None)).map(|d| d.units.len()).ok() != Some(units.len()) {
        return false;
    }
    // every subtitle the message names must really be the case it names
    let named = |part: &str| -> Vec<String> {
        let Some(at) = reason.find(part) else { return Vec::new() };
        let rest = &reason[at + part.len()..];
        let end = [". Remove", "; and ", " (and "].iter().filter_map(|m| rest.find(m)).min().unwrap_or(rest.len());
        rest[..end].split(", ").map(String::from).collect()
    };
    let bytes = named("(they would read as other characters): ");
    let lacking = named("comment subtitles hold characters that table does not have: ");
    if bytes.is_empty() && lacking.is_empty() {
        return false;
    }
    let comment = |item: &str| {
        let n = item.strip_prefix("comment subtitle at block ")?.parse::<usize>().ok()?;
        groups.iter().find(|g| g.0 + 1 == n)
    };
    let bytes_ok = bytes.iter().all(|item| {
        let tf = match item.strip_prefix("subtitle ").and_then(|n| n.parse::<usize>().ok()) {
            Some(k) => units.get(k.wrapping_sub(1)).copied(),
            None => comment(item).map(|g| g.2.as_slice()),
        };
        tf.is_some_and(|tf| tf.iter().any(|b| turns(*b)))
    });
    // a comment that lacks characters: an upper-half character of the old table that the new table does not have, or
    // a table 00 accented letter (a floating mark before its letter) when the new table is not Latin
    let lacking_ok = lacking.iter().all(|item| {
        comment(item).is_some_and(|g| {
            g.1 && g.2.iter().any(|b| (*b >= 0xA0 && old[*b as usize].is_some_and(|c| !new.contains(&Some(c)))) || (from == 0 && to != 0 && diacritic(*b)))
        })
    });
    bytes_ok && lacking_ok
}

fn check(name: &str, what: &str, got: Result<Vec<u8>, Error>, want: &serde_json::Value, bad: &mut Vec<String>, dir: &Option<String>, input: &[u8]) {
    if switch_refused_on_purpose(&got, want, input) {
        println!("accepted 798 refusal: {name}: {what}");
        return;
    }
    if let Some(err) = want.get("err") {
        let msg = want["msg"].as_str().unwrap();
        match got {
            Err(Error::Unwritable { reason: m, .. }) if err == "CaptionTargetError" && m == msg => {}
            Err(e) => bad.push(format!("{name}: {what}: LocOS refuses with {err} {msg:?}, we give error {e:?}")),
            Ok(_) => bad.push(format!("{name}: {what}: LocOS refuses ({msg}) but we wrote a file")),
        }
        return;
    }
    match got {
        Err(e) => bad.push(format!("{name}: {what}: LocOS builds, we refuse: {e}")),
        Ok(b) => {
            if sha256(&b) != want["sha"].as_str().unwrap() {
                if let Some(d) = dir {
                    std::fs::write(format!("{d}/{name}.{}.got.stl", what.replace([':', ' '], "_")), &b).unwrap();
                    std::fs::write(format!("{d}/{name}.in.stl"), input).unwrap();
                }
                bad.push(format!("{name}: {what}: bytes differ (got {} bytes, LocOS {})", b.len(), want["len"]));
            }
        }
    }
}

#[test]
fn matches_locos_on_suite_fixtures_hostile_and_random_files() {
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/locos_golden.json")).unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 130);
    let dir = std::env::var("STL_GOLDEN_DEBUG").ok();
    let (mut parsed, mut refused, mut builds) = (0, 0, 0);
    let mut bad: Vec<String> = Vec::new();
    for c in &cases {
        let name = c["name"].as_str().unwrap();
        let data = hex(c["hex"].as_str().unwrap());
        if let Some(cls) = c.get("error") {
            let msg = c["msg"].as_str().unwrap();
            match Stl.extract(&data, &opts(None)) {
                Err(Error::Parse { msg: m, .. }) if cls == "StlParseError" && m == msg => {}
                Err(Error::Limit(_)) if cls == "InputLimitError" => {}
                other => bad.push(format!("{name}: LocOS refuses with {cls} {msg:?}; we give {:?}", other.map(|_| "a document"))),
            }
            refused += 1;
            continue;
        }
        let want: Vec<&str> = c["units"].as_array().unwrap().iter().map(|u| u.as_str().unwrap()).collect();
        let doc = match Stl.extract(&data, &opts(None)) {
            Ok(d) => d,
            Err(e) => {
                bad.push(format!("{name}: LocOS parses, we refuse: {e}"));
                continue;
            }
        };
        let got = texts(&doc);
        if got.iter().map(String::as_str).collect::<Vec<_>>() != want {
            bad.push(format!("{name}: units {got:?} vs {want:?}"));
            continue;
        }
        parsed += 1;
        let b = c["builds"].as_object().unwrap();
        for (label, r) in b {
            let m = mapping(&r["map"]);
            check(name, &format!("build {label}"), build(&data, &m), r, &mut bad, &dir, &data);
            builds += 1;
        }
        let maps: BTreeMap<&str, Vec<(usize, String)>> = b.iter().map(|(k, r)| (k.as_str(), mapping(&r["map"]))).collect();
        for (key, r) in c["localized"].as_object().unwrap() {
            let (_, locale) = key.split_once(':').unwrap();
            let lang = r["lang"].as_str().unwrap();
            let empty = Vec::new();
            let m = if lang == "none" { &empty } else { &maps[lang] };
            check(name, &format!("localized {key}"), build_localized(&data, m, locale), r, &mut bad, &dir, &data);
            builds += 1;
        }
    }
    println!("golden: {parsed} files parsed like LocOS, {refused} refused like LocOS, {builds} builds compared");
    assert!(bad.is_empty(), "{} differences, first ones:\n{}", bad.len(), bad.iter().take(60).cloned().collect::<Vec<_>>().join("\n"));
}

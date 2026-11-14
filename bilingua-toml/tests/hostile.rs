//! Hostile input: random bytes, byte-mutated and truncated fixtures never panic; the scanner agrees with the
//! reference `toml` crate (TOML 1.0) on what is valid.

mod common;
use bilingua_model::*;
use bilingua_toml::TomlFilter;
use common::*;
use proptest::prelude::*;
use std::path::PathBuf;

fn fixtures() -> Vec<Vec<u8>> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/toml");
    let mut v: Vec<Vec<u8>> = std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok()).filter(|e| e.path().extension().is_some_and(|x| x == "toml")).map(|e| std::fs::read(e.path()).unwrap()).collect();
    v.sort();
    v
}

/// Extract; when it works, merge untouched (must be exact) and pseudo-translated (must stay valid TOML).
fn exercise(data: &[u8]) {
    let Ok(d) = try_extract(data) else { return };
    assert_eq!(TomlFilter.merge(&d).unwrap(), data, "untouched merge changed bytes");
    let mut d = d;
    let ids: Vec<(String, Vec<Inline>)> = d
        .units
        .iter()
        .filter(|u| u.translate)
        .map(|u| {
            let mut t = vec![Inline::text("'\"\\\n\u{1}\u{4e2d}''' \"\"\" ")];
            t.extend(u.source());
            (u.id.clone(), merge_text(t))
        })
        .collect();
    for (id, t) in ids {
        d.set_target(&id, t).unwrap();
    }
    let out = TomlFilter.merge(&d).expect("a translated merge of a valid file must succeed");
    let s = std::str::from_utf8(&out).unwrap();
    let s = s.strip_prefix('\u{feff}').unwrap_or(s);
    assert!(toml::from_str::<toml::Table>(s).is_ok(), "merged output is not valid TOML");
    assert_eq!(extract(&out).units.len(), d.units.len());
}

/// Valid per the scanner implies valid per the reference parser, and the other way round (apart from limits).
fn agree(data: &[u8]) {
    let Ok(s) = std::str::from_utf8(data) else { return };
    let s = s.strip_prefix('\u{feff}').unwrap_or(s);
    let reference = toml::from_str::<toml::Table>(s).is_ok();
    match try_extract(data) {
        Ok(_) => assert!(reference, "accepted but the reference parser rejects: {s:?}"),
        Err(Error::Parse { .. }) => assert!(!reference, "rejected but the reference parser accepts: {s:?}"),
        Err(_) => {} // encoding or size limits
    }
}

fn mutate(base: &[u8], edits: &[(usize, u8, u8)]) -> Vec<u8> {
    let mut v = base.to_vec();
    for &(pos, kind, byte) in edits {
        if v.is_empty() {
            break;
        }
        let p = pos % v.len();
        match kind % 4 {
            0 => v[p] = byte,
            1 => v.insert(p, byte),
            2 => {
                v.remove(p);
            }
            _ => v.truncate(p),
        }
    }
    v
}

proptest! {
    #![proptest_config(ProptestConfig::default())]

    #[test]
    fn random_bytes_never_panic(data in proptest::collection::vec(any::<u8>(), 0..400)) {
        exercise(&data);
        agree(&data);
    }

    #[test]
    fn random_tomlish_text_never_panics(s in "[a-z0-9 =\\[\\]{}\"'#.,\\n\\\\_-]{0,200}") {
        exercise(s.as_bytes());
        agree(s.as_bytes());
    }

    #[test]
    fn mutated_fixtures_never_panic(which in 0usize..8, edits in proptest::collection::vec((any::<usize>(), any::<u8>(), any::<u8>()), 1..6)) {
        let fx = fixtures();
        let data = mutate(&fx[which % fx.len()], &edits);
        exercise(&data);
        agree(&data);
    }
}

#[test]
fn every_truncation_of_every_fixture_is_handled() {
    for fx in fixtures() {
        // every prefix on a char boundary or not
        for n in 0..fx.len() {
            let cut = &fx[..n];
            exercise(cut);
            agree(cut);
        }
    }
}

#[test]
fn deep_and_huge_inputs_return_errors() {
    let mut deep = String::from("a = ");
    deep.push_str(&"{ b = ".repeat(100_000));
    assert!(try_extract(deep.as_bytes()).is_err());
    let arr = format!("a = {}", "[".repeat(1_000_000));
    assert!(try_extract(arr.as_bytes()).is_err());
    let tables: String = (0..100_000).map(|i| format!("[t{i}]\n")).collect();
    assert!(try_extract(tables.as_bytes()).is_ok());
    // a very long single line without a terminator
    let long = format!("a = \"{}", "x".repeat(5_000_000));
    assert!(try_extract(long.as_bytes()).is_err());
    // headers that collide
    assert!(try_extract(b"[a]\nb = 1\n[a]\n").is_err());
    assert!(try_extract(b"a = 1\n[a]\n").is_err());
    assert!(try_extract(b"a = [1]\n[[a]]\n").is_err());
    assert!(try_extract(b"[a]\nb.c = 1\n[a.b]\n").is_err());
    assert!(try_extract(b"a = { b = 1 }\n[a.c]\n").is_err());
    assert!(try_extract(b"a = { b = 1,\n}\n").is_err());
}

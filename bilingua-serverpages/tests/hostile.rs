//! Hostile input: random bytes, markup-and-code shaped noise, mutated and truncated fixtures, deep and huge files.
//! Nothing panics; a file that opens merges back to its exact bytes and survives a pseudo-translation with its code
//! intact.
mod common;
use bilingua_model::*;
use bilingua_serverpages::Dialect;
use common::*;
use proptest::prelude::*;

fn dialect(fmt: &str) -> Dialect {
    Dialect::from_extension(filter(fmt).extensions()[0]).unwrap()
}

fn check(fmt: &str, data: &[u8]) {
    let f = filter(fmt);
    let _ = f.sniff(data);
    let Ok(mut doc) = f.extract(data, &opts()) else { return };
    assert_eq!(f.merge(&doc).unwrap(), data, "{fmt}: untouched merge must be exact");
    let ids: Vec<(String, Vec<Inline>)> = doc.units.iter().map(|u| (u.segments[0].id.clone(), u.source())).collect();
    for (id, src) in &ids {
        let mut t = vec![Inline::text("\u{ab}<?php x ?>\"'>")];
        t.extend(src.iter().filter(|i| matches!(i, Inline::Tag { .. })).cloned());
        let _ = doc.set_target(id, t);
    }
    match f.merge(&doc) {
        Ok(out) => {
            // the code of the page survives, byte for byte, whatever the translation said
            if let (Ok(a), Ok(b)) = (std::str::from_utf8(data), std::str::from_utf8(&out)) {
                let d = dialect(fmt);
                if let (Ok(mut a), Ok(mut b)) = (d.code_islands(a), d.code_islands(b)) {
                    a.sort();
                    b.sort();
                    assert_eq!(a, b, "{fmt}: code islands changed");
                }
            }
            let back = f.extract(&out, &opts());
            if let Ok(back) = back {
                assert_eq!(back.units.len(), doc.units.len(), "{fmt}: unit count after the merge");
            }
        }
        Err(Error::Write(_)) | Err(Error::Parse { .. }) | Err(Error::Limit(_)) => {}
        Err(e) => panic!("{fmt}: unexpected merge error {e:?}"),
    }
}

fn fixtures() -> Vec<(&'static str, Vec<u8>)> {
    FORMATS.iter().map(|f| (*f, std::fs::read(data_dir().join(format!("suite.{f}"))).unwrap())).collect()
}

const PIECES: &[&str] = &[
    "<", ">", "<?php ", "<?=", "?>", "<%", "%>", "<%=", "<%--", "--%>", "${", "#{", "}", "{", "\"", "'", "`", "<p>", "</p>", "<b>", "</b>", "<!--", "-->", "<script>",
    "</script>", "<style>", "alt=", "=", " ", "\n", "\r\n", "text", "\u{e9}", "\u{65e5}", "<img ", "<a href=", "#", "//", "/*", "*/", "<<<EOT\n", "EOT\n", "<jsp:scriptlet>",
    "</jsp:scriptlet>", "<!--#echo ", "<!--#include ", "translate=\"no\"", "<br>", "\u{FDD0}", "\u{FDD1}", "<![CDATA[", "]]>", "<meta name=description content=", "<html lang=",
    "<title>", "</title>", "<textarea>", "<svg>", "<xmp>", "&amp;", "&", "\\", "%%>", "<%%", "<?xml ", "<div ", "title=\"", "<option>", "<li>", "<td>",
];

fn noise() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(prop::sample::select(PIECES), 0..60).prop_map(|v| v.concat().into_bytes())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 400, ..ProptestConfig::default() })]

    #[test]
    fn random_bytes_never_panic(data in prop::collection::vec(any::<u8>(), 0..400)) {
        for fmt in FORMATS {
            check(fmt, &data);
        }
    }

    #[test]
    fn markup_and_code_shaped_noise_never_panics(data in noise()) {
        for fmt in FORMATS {
            check(fmt, &data);
        }
    }

    #[test]
    fn mutated_fixtures_never_panic(idx in 0usize..5, flips in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 1..6), cut in prop::option::of(any::<prop::sample::Index>()), ins in prop::option::of((any::<prop::sample::Index>(), prop::sample::select(PIECES)))) {
        let (fmt, base) = fixtures().swap_remove(idx);
        let mut d = base.clone();
        for (i, b) in &flips {
            let k = i.index(d.len());
            d[k] = *b;
        }
        if let Some((i, p)) = ins {
            let k = i.index(d.len() + 1);
            d.splice(k..k, p.bytes());
        }
        if let Some(c) = cut {
            let k = c.index(d.len() + 1);
            d.truncate(k);
        }
        check(fmt, &d);
    }
}

#[test]
fn truncated_fixtures_at_every_byte_never_panic() {
    for (fmt, base) in fixtures() {
        for n in 0..=base.len() {
            check(fmt, &base[..n]);
        }
    }
}

#[test]
fn deep_and_huge_files_are_typed_errors_or_fine() {
    for fmt in FORMATS {
        let f = filter(fmt);
        // very deep nesting of inline and block tags
        let deep = format!("{}text{}", "<b><div><span>".repeat(20000), "</span></div></b>".repeat(20000));
        let _ = f.extract(deep.as_bytes(), &opts());
        // thousands of unclosed openers
        let open = "<a href=\"x\" title=\"t\">word ".repeat(30000);
        let _ = f.extract(open.as_bytes(), &opts());
        // a million angle brackets
        let lt = "<".repeat(1_000_000);
        assert!(matches!(f.extract(lt.as_bytes(), &opts()), Err(Error::Limit(_))) || f.extract(lt.as_bytes(), &opts()).is_ok(), "{fmt}");
        // one enormous text run and one enormous attribute
        let run = format!("<p>{}</p>", "word ".repeat(200_000));
        let r = f.extract(run.as_bytes(), &opts());
        assert!(matches!(r, Err(Error::Limit(_))), "{fmt}: a 1 MB paragraph is over the unit length limit");
        let attr = format!("<img alt=\"{}\">", "a".repeat(5_000_000));
        let _ = f.extract(attr.as_bytes(), &opts());
        // more units than the limit
        let many = "<p>a b</p>".repeat(60_000);
        assert!(matches!(f.extract(many.as_bytes(), &opts()), Err(Error::Limit(_))), "{fmt}");
        // an input over the byte limit
        let small = ExtractOptions { limits: Some(Limits { max_input_bytes: 100, ..Limits::default() }), ..Default::default() };
        assert!(matches!(f.extract(&[b'a'; 101], &small), Err(Error::Limit(_))), "{fmt}");
    }
}

#[test]
fn many_islands_and_long_code_stay_linear() {
    let t = std::time::Instant::now();
    let php = format!("<p>a</p>{}", "<?php $x = '\\''; ?>".repeat(40000));
    let _ = extract("phtml", php.as_bytes());
    let jsp = format!("<p>a ${{x}} b</p>{}", "${a[\"}\"]}".repeat(40000));
    let _ = extract("jsp", jsp.as_bytes());
    // quote-heavy PHP that never closes: refused fast, not quadratic
    let q = format!("<?php {}", "'a\\'".repeat(100_000));
    assert!(extract("phtml", q.as_bytes()).is_err());
    let h = format!("<?php $a = <<<X\n{}", "x\n".repeat(200_000));
    assert!(extract("phtml", h.as_bytes()).is_err());
    assert!(t.elapsed().as_secs() < 60, "{:?}", t.elapsed());
}

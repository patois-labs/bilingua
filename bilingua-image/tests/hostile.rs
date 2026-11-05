//! Hostile input: bombs, injection through translations, damaged files; property tests over random bytes and
//! byte-mutated fixtures. Nothing may panic; refusals are typed errors.
mod common;
use bilingua_image::{ImageFilter, SvgFilter};
use bilingua_model::guard::LimitKind;
use bilingua_model::*;
use common::*;
use proptest::prelude::*;

fn kind(e: &Error) -> Option<LimitKind> {
    match e {
        Error::Limit(l) => Some(l.kind),
        _ => None,
    }
}

// ---- XML bombs -----------------------------------------------------------------------------------------------------------

#[test]
fn entity_bombs_and_external_entities_are_refused_before_any_parsing() {
    let lol = "<?xml version=\"1.0\"?><!DOCTYPE svg [<!ENTITY a \"aaaaaaaaaa\"><!ENTITY b \"&a;&a;&a;&a;&a;&a;&a;&a;&a;&a;\"><!ENTITY c \"&b;&b;&b;&b;&b;&b;&b;&b;&b;&b;\">]><svg xmlns=\"http://www.w3.org/2000/svg\"><text>&c;</text></svg>";
    assert_eq!(kind(&SvgFilter.extract(lol.as_bytes(), &opts()).unwrap_err()), Some(LimitKind::XmlEntities));
    let xxe = "<!DOCTYPE svg [<!ENTITY x SYSTEM \"file:///etc/passwd\">]><svg><text>&x;</text></svg>";
    assert_eq!(kind(&ImageFilter.extract(xxe.as_bytes(), &opts()).unwrap_err()), Some(LimitKind::XmlEntities));
    let param = "<!DOCTYPE svg [<!ENTITY % p SYSTEM \"http://example.invalid/x.dtd\"> %p;]><svg><text>x</text></svg>";
    assert!(SvgFilter.extract(param.as_bytes(), &opts()).is_err());
    // UTF-16 does not hide it
    let u16: Vec<u8> = [0xFFu8, 0xFE].into_iter().chain(lol.encode_utf16().flat_map(u16::to_le_bytes)).collect();
    assert!(SvgFilter.extract(&u16, &opts()).is_err());
    // an entity that is only mentioned (not declared) is text, never expanded
    let d = extract(b"<svg><text>&xxe; &lol1; ok</text></svg>");
    assert_eq!(texts(&d), ["&xxe; &lol1; ok"]);
    let out = s(build(b"<svg><text>&xxe; ok</text></svg>", &[(0, "&xxe; ok!")]).unwrap());
    assert!(out.contains("&amp;xxe; ok!"), "{out}");
}

#[test]
fn deep_and_wide_documents_are_refused_with_a_typed_error() {
    let deep = format!("<svg>{}x{}</svg>", "<g>".repeat(300), "</g>".repeat(300));
    assert_eq!(kind(&SvgFilter.extract(deep.as_bytes(), &opts()).unwrap_err()), Some(LimitKind::XmlDepth));
    let wide = format!("<svg>{}</svg>", "<g/>".repeat(1_100_000));
    assert!(matches!(SvgFilter.extract(wide.as_bytes(), &opts()), Err(Error::Limit(_))));
    let o = opts().with_limits(Limits::new().with_max_input_bytes(1000));
    assert_eq!(kind(&SvgFilter.extract(&vec![b'a'; 2000], &o).unwrap_err()), Some(LimitKind::InputBytes));
    let long = format!("<svg><text>{}</text></svg>", "a".repeat(100_001));
    assert_eq!(kind(&SvgFilter.extract(long.as_bytes(), &opts()).unwrap_err()), Some(LimitKind::UnitChars));
}

// ---- injection through translations ---------------------------------------------------------------------------------

const TARGETS: [&str; 10] = [
    "</text><script>alert(1)</script><text>",
    "<foreignObject><iframe src=\"javascript:alert(1)\"/></foreignObject>",
    "]]> <![CDATA[ x ]]>",
    "--> <!-- ",
    "<?php echo 1 ?> <!DOCTYPE svg [<!ENTITY x SYSTEM \"file:///etc/passwd\">]> &x;",
    "\" onload=\"alert(1)\" x=\"",
    "' onmouseover='alert(1)",
    "&lt;script&gt; &#x3c;script&#x3e; &amp;amp;",
    "nul\u{0}vt\u{b}ff\u{c}esc\u{1b}bom\u{feff}nc\u{ffff}",
    "</svg><svg onload=\"alert(1)\">",
];

const INJ: &str = "<?xml version=\"1.0\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\"><title>T</title><text x=\"1\" y=\"2\"><tspan>Alpha</tspan> Beta</text><desc>D</desc><foreignObject><p>html</p></foreignObject></svg>";

#[test]
fn a_translation_never_adds_markup_or_makes_the_file_ill_formed() {
    let before = start_names(INJ);
    for t in TARGETS {
        for lang in ["", "ar", "de-DE"] {
            let mut d = SvgFilter.extract(INJ.as_bytes(), &if lang.is_empty() { opts() } else { with_target(lang) }).unwrap();
            for i in 0..d.units.len() {
                set(&mut d, i, t);
            }
            let out = String::from_utf8(SvgFilter.merge_with(&d, &locos_opts()).unwrap()).unwrap();
            wellformed(&out).unwrap_or_else(|e| panic!("{t:?} {lang}: {e}\n{out}"));
            assert_eq!(start_names(&out), before, "{t:?} {lang}: elements appeared\n{out}");
            // the text reads back as what was written, minus the control characters XML forbids
            let back = SvgFilter.extract(out.as_bytes(), &opts()).unwrap();
            let want: String = t.chars().filter(|c| !matches!(*c as u32, 0..=8 | 0xE..=0x1F | 0xFFFE | 0xFFFF)).map(|c| if matches!(c as u32, 0xB | 0xC) { '\n' } else { c }).collect();
            let want = want.trim().to_string();
            assert!(texts(&back).iter().all(|x| *x == want), "{t:?} -> {:?}", texts(&back));
        }
    }
}

#[test]
fn text_in_script_style_foreignobject_and_attributes_is_never_a_unit() {
    let svg = "<svg><script>var a = 'Hello';</script><style>.a{content:'World'}</style><foreignObject><div><text>Not</text></div></foreignObject><text title=\"Attr\" onclick=\"Click()\">Real</text><metadata><title>Meta</title></metadata></svg>";
    let d = extract(svg.as_bytes());
    // LocOS reads a <text> element anywhere (even inside foreignObject): same here
    assert_eq!(texts(&d), ["Not", "Real"]);
}

#[test]
fn text_nodes_inside_comments_cdata_and_attribute_values_are_left_alone() {
    // a `>` in these regions ends a tag early in the LocOS reader and the rest looks like text
    let svg = "<svg><text title=\"a>b\">Hi</text><text><!-- x > y -->Yo</text><text><![CDATA[ p > q ]]>Ya</text></svg>";
    let d = extract(svg.as_bytes());
    assert!(d.warnings.iter().any(|w| w.kind == "svg-markup-text"), "{:?}", d.warnings);
    let mut d = d;
    for i in 0..d.units.len() {
        set(&mut d, i, "T");
    }
    let out = String::from_utf8(SvgFilter.merge(&d).unwrap()).unwrap();
    wellformed(&out).unwrap();
    assert!(out.contains("title=\"a>b\"") && out.contains("<!-- x > y -->") && out.contains("<![CDATA[ p > q ]]>"), "{out}");
}

// ---- damaged files ------------------------------------------------------------------------------------------------

#[test]
fn damaged_pngs_are_typed_errors() {
    let good = png(Some("[{\"text\": \"a\", \"bbox\": [1, 2, 3, 4]}]"), 40, 40, &[]);
    assert_eq!(image_extract(&good).unwrap().units.len(), 1);
    // bad checksum anywhere before the end
    for at in [20usize, 30, 45, 60] {
        let mut b = good.clone();
        b[at] ^= 0x55;
        assert!(image_extract(&b).is_err(), "flip at {at}");
    }
    // cut off before the image data has started
    for cut in [0usize, 7, 8, 10, 20, 33, 40, 60] {
        assert!(image_extract(&good[..cut]).is_err(), "cut at {cut}");
    }
    // a chunk length that runs past the end, or is huge
    let mut b = good.clone();
    b[8..12].copy_from_slice(&0xFFFF_FFFFu32.to_be_bytes());
    assert!(image_extract(&b).is_err());
    // header lies about the size: more rows promised than the data holds
    let mut b = good.clone();
    b[20..24].copy_from_slice(&4000u32.to_be_bytes());
    let crc = crc32fast::hash(&b[12..29]).to_be_bytes();
    b[29..33].copy_from_slice(&crc);
    assert!(image_extract(&b).is_err());
}

#[test]
fn text_chunk_bombs_are_refused() {
    // 8 MB of zeros deflate to a few KB: over the 1 MB a text chunk may expand to
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
    std::io::Write::write_all(&mut e, &vec![b' '; 8 << 20]).unwrap();
    let packed = e.finish().unwrap();
    let mut z = b"Comment\0\0".to_vec();
    z.extend_from_slice(&packed);
    assert!(packed.len() < 20_000);
    let img = png(None, 10, 10, &[(b"zTXt", z)]);
    assert!(image_extract(&img).unwrap_err().to_string().contains("too large"));
    // a zTXt with an unknown compression method
    let img = png(None, 10, 10, &[(b"zTXt", b"Comment\0\x07abc".to_vec())]);
    assert!(image_extract(&img).is_err());
    // broken zlib data in a zTXt is not an error (nothing is kept), as in Pillow
    let img = png(None, 10, 10, &[(b"zTXt", b"Comment\0\0notzlib".to_vec())]);
    assert!(image_extract(&img).is_ok());
    // total text memory over 64 MB
    let chunk64: Vec<u8> = [b"k\0".as_slice(), &vec![b'x'; 1 << 20]].concat();
    let extra: Vec<(&[u8; 4], Vec<u8>)> = (0..66).map(|_| (b"tEXt", chunk64.clone())).collect();
    let img = png(None, 10, 10, &extra);
    assert!(image_extract(&img).unwrap_err().to_string().contains("too much memory"));
}

#[test]
fn hostile_sidecars_are_contained() {
    // far more regions than the cap: the first hundred in reading order
    let many: Vec<String> = (0..5000).map(|i| format!("{{\"text\": \"t{i}\", \"bbox\": [{}, {}, 1, 1]}}", i % 50, i / 50)).collect();
    let img = png(Some(&format!("[{}]", many.join(","))), 20, 20, &[]);
    assert_eq!(image_extract(&img).unwrap().units.len(), 100);
    // deep nesting and absurd numbers: no panic, no regions or a refusal
    for j in [format!("{}1{}", "[".repeat(100_000), "]".repeat(100_000)), "[{\"text\":\"a\",\"bbox\":[1e999,0,1,1]}]".into(), "[{\"text\":\"a\",\"bbox\":[1e308,1e308,1e308,1e308]}]".into(), "[{\"text\":\"a\",\"bbox\":[NaN,0,1,1]}]".into(), "\"\\ud800\"".into(), "[{\"text\":\"\\ud800x\",\"bbox\":[1,2,3,4]}]".into()] {
        let img = png(Some(&j), 20, 20, &[]);
        let _ = image_extract(&img);
    }
    // a unit's text is whatever the sidecar says, control characters included; the merge still works
    let img = png(Some("[{\"text\": \"a\\u0000b\\n\", \"bbox\": [1, 2, 3, 4]}]"), 20, 20, &[]);
    let mut d = image_extract(&img).unwrap();
    set(&mut d, 0, "x\u{0}y");
    let out = ImageFilter.merge(&d).unwrap();
    assert_eq!(texts(&image_extract(&out).unwrap()), ["x\u{0}y"]);
}

// ---- property tests -----------------------------------------------------------------------------------------------

fn fixtures() -> Vec<Vec<u8>> {
    vec![
        INJ.as_bytes().to_vec(),
        RTL_SAMPLE.as_bytes().to_vec(),
        [b"\xef\xbb\xbf".as_slice(), INJ.as_bytes()].concat(),
        [0xFFu8, 0xFE].into_iter().chain(INJ.encode_utf16().flat_map(u16::to_le_bytes)).collect(),
        INJ.replace("<?xml version=\"1.0\"?>", "<?xml version=\"1.0\" encoding=\"windows-1252\"?>").into_bytes(),
        png(Some("[{\"text\": \"Sale\", \"bbox\": [1, 2, 3, 4]}, {\"text\": \"Shop\", \"bbox\": [1, 9, 3, 4]}]"), 24, 24, &[]),
    ]
}

const RTL_SAMPLE: &str = "<svg><text id=\"a\" x=\"1\" style=\"text-anchor: end\">One</text><g text-anchor=\"end\"><text direction=\"ltr\">Two<tspan text-anchor=\"start\">Three</tspan></text></g><!-- c > d --></svg>";

fn exercise(bytes: &[u8]) {
    for f in [&SvgFilter as &dyn Filter, &ImageFilter] {
        let _ = f.sniff(bytes);
        if let Ok(mut d) = f.extract(bytes, &with_target("ar")) {
            for i in 0..d.units.len() {
                d.units[i].segments[0].target = vec![Inline::text(format!("t<&>{i}"))];
            }
            let _ = f.merge(&d);
            let _ = f.merge_with(&d, &locos_opts());
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 600, ..ProptestConfig::default() })]

    #[test]
    fn random_bytes_never_panic(b in proptest::collection::vec(any::<u8>(), 0..400)) {
        exercise(&b);
    }

    #[test]
    fn random_text_with_markup_characters_never_panics(s in "[<>/&\"'=!?\\[\\]a-z \\n-]{0,300}") {
        exercise(s.as_bytes());
        exercise(format!("<svg><text>{s}</text></svg>").as_bytes());
        exercise(format!("<svg {s}><text {s}>x</text></svg>").as_bytes());
    }

    #[test]
    fn mutated_fixtures_never_panic(idx in 0usize..6, edits in proptest::collection::vec((any::<prop::sample::Index>(), any::<u8>(), 0u8..4), 1..8)) {
        let mut b = fixtures()[idx].clone();
        for (at, v, kind) in edits {
            if b.is_empty() { break; }
            let i = at.index(b.len());
            match kind {
                0 => b[i] = v,
                1 => { b.remove(i); }
                2 => b.insert(i, v),
                _ => b.truncate(i),
            }
        }
        exercise(&b);
    }

    #[test]
    fn translations_with_any_characters_keep_valid_svg_valid(t in "\\PC{0,40}") {
        let mut d = SvgFilter.extract(INJ.as_bytes(), &with_target("he")).unwrap();
        for i in 0..d.units.len() { set(&mut d, i, &t); }
        let out = String::from_utf8(SvgFilter.merge_with(&d, &locos_opts()).unwrap()).unwrap();
        prop_assert!(wellformed(&out).is_ok(), "{:?}", out);
        prop_assert_eq!(start_names(&out), start_names(INJ));
    }
}

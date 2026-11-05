//! Hostile input: malformed, truncated, deep and huge files give typed errors and never panic; hostile zips (bombs,
//! duplicate members, path escapes, forged sizes, encrypted members); XML bombs and XXE; property tests over random
//! bytes, mutated XML in a valid package, and arbitrary target text.
mod common;
use bilingua_model::*;
use bilingua_odf::*;
use common::*;
use proptest::prelude::*;

fn typed(e: &Error) -> bool {
    matches!(e, Error::Parse { .. } | Error::Unsupported(_) | Error::Limit(_) | Error::Encoding(_))
}

/// Extract; when it works, translate every unit and merge, then read the result back. Nothing may panic, errors must
/// be typed, and a merge of something that was read must itself be readable with the same number of units.
fn exercise(f: &dyn Filter, bytes: &[u8], target: Option<&str>) -> bool {
    let o = ExtractOptions { target_lang: target.map(str::to_string), ..Default::default() };
    match f.extract(bytes, &o) {
        Err(e) => {
            assert!(typed(&e), "untyped error: {e:?}");
            false
        }
        Ok(mut d) => {
            let same = f.merge(&d).expect("untouched merge");
            assert_eq!(same, bytes, "an untouched merge must return the input");
            for u in d.units.iter_mut().filter(|u| u.translate) {
                u.segments[0].target = vec![Inline::text(format!("\u{ab}{}\u{bb}", plain_text(&u.segments[0].source)))];
            }
            let n = d.units.len();
            match f.merge(&d) {
                Ok(out) => {
                    let back = f.extract(&out, &ExtractOptions::default()).unwrap_or_else(|e| panic!("merged output unreadable: {e}"));
                    assert_eq!(back.units.len(), n);
                }
                Err(e) => panic!("merge failed on a readable file: {e}"),
            }
            true
        }
    }
}

// ---- a tiny zip writer that can be wrong on purpose ----

fn crc32(data: &[u8]) -> u32 {
    let mut c = !0u32;
    for b in data {
        c ^= *b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
        }
    }
    !c
}

struct Raw<'a> {
    name: &'a str,
    data: &'a [u8],
    /// Size written in the central directory (a forged one when different from the data).
    claimed: Option<u32>,
    flags: u16,
}

fn raw_zip(es: &[Raw]) -> Vec<u8> {
    let mut z = Vec::new();
    let mut dir = Vec::new();
    for e in es {
        let off = z.len() as u32;
        let crc = crc32(e.data);
        let size = e.claimed.unwrap_or(e.data.len() as u32);
        let hdr = |z: &mut Vec<u8>, central: bool| {
            z.extend_from_slice(if central { &[0x50, 0x4b, 1, 2] } else { &[0x50, 0x4b, 3, 4] });
            if central {
                z.extend_from_slice(&20u16.to_le_bytes());
            }
            z.extend_from_slice(&20u16.to_le_bytes());
            z.extend_from_slice(&e.flags.to_le_bytes());
            z.extend_from_slice(&0u16.to_le_bytes());
            z.extend_from_slice(&0u16.to_le_bytes());
            z.extend_from_slice(&0x21u16.to_le_bytes());
            z.extend_from_slice(&crc.to_le_bytes());
            z.extend_from_slice(&size.to_le_bytes());
            z.extend_from_slice(&size.to_le_bytes());
            z.extend_from_slice(&(e.name.len() as u16).to_le_bytes());
            z.extend_from_slice(&0u16.to_le_bytes());
            if central {
                z.extend_from_slice(&[0; 10]);
                z.extend_from_slice(&off.to_le_bytes());
            }
            z.extend_from_slice(e.name.as_bytes());
        };
        hdr(&mut z, false);
        z.extend_from_slice(e.data);
        hdr(&mut dir, true);
    }
    let start = z.len() as u32;
    z.extend_from_slice(&dir);
    z.extend_from_slice(&[0x50, 0x4b, 5, 6, 0, 0, 0, 0]);
    z.extend_from_slice(&(es.len() as u16).to_le_bytes());
    z.extend_from_slice(&(es.len() as u16).to_le_bytes());
    z.extend_from_slice(&(dir.len() as u32).to_le_bytes());
    z.extend_from_slice(&start.to_le_bytes());
    z.extend_from_slice(&0u16.to_le_bytes());
    z
}

fn r<'a>(name: &'a str, data: &'a [u8]) -> Raw<'a> {
    Raw { name, data, claimed: None, flags: 0 }
}

#[track_caller]
fn refuse(bytes: &[u8]) -> Error {
    let e = Odt::default().extract(bytes, &opts(None)).expect_err("must be refused");
    assert!(typed(&e), "{e:?}");
    e
}

#[test]
fn the_raw_zip_helper_makes_a_readable_package() {
    let c = odt_content();
    let z = raw_zip(&[r("mimetype", MIME_TEXT.as_bytes()), r("content.xml", c.as_bytes())]);
    assert!(Odt::default().extract(&z, &opts(None)).is_ok());
}

#[test]
fn hostile_zips() {
    let c = odt_content();
    let m = MIME_TEXT.as_bytes();
    // duplicate members, also by case
    refuse(&raw_zip(&[r("mimetype", m), r("content.xml", c.as_bytes()), r("content.xml", c.as_bytes())]));
    refuse(&raw_zip(&[r("mimetype", m), r("content.xml", c.as_bytes()), r("Content.XML", c.as_bytes())]));
    // path escapes
    for bad in ["../evil.xml", "a/../../evil.xml", "/etc/passwd", "C:\\evil.xml", "x/..\\..\\evil", "a\u{1}b", "a:stream"] {
        refuse(&raw_zip(&[r("mimetype", m), r("content.xml", c.as_bytes()), r(bad, b"x")]));
    }
    // an encrypted member
    refuse(&raw_zip(&[r("mimetype", m), Raw { name: "content.xml", data: c.as_bytes(), claimed: None, flags: 1 }]));
    // a directory that understates the size of a member
    let big = vec![b'a'; 4000];
    let forged = raw_zip(&[r("mimetype", m), Raw { name: "content.xml", data: c.as_bytes(), claimed: Some(10), flags: 0 }, r("Pictures/x", &big)]);
    refuse(&forged);
    // a directory that claims an enormous size
    let huge = raw_zip(&[r("mimetype", m), Raw { name: "content.xml", data: c.as_bytes(), claimed: Some(u32::MAX), flags: 0 }]);
    refuse(&huge);
    // many members
    let names: Vec<String> = (0..60).map(|i| format!("m{i}.bin")).collect();
    let mut es = vec![r("mimetype", m), r("content.xml", c.as_bytes())];
    for n in &names {
        es.push(r(n, b"x"));
    }
    let z = raw_zip(&es);
    let few = ExtractOptions { limits: Some(Limits { max_zip_entries: 20, ..Limits::default() }), ..Default::default() };
    assert!(matches!(Odt::default().extract(&z, &few), Err(Error::Limit(l)) if l.kind == LimitKind::ZipEntries));
    // a big zero member: ratio
    assert!(matches!(
        Odt::default().extract(&Pkg::new(c.clone()).extra("Pictures/z.bin", vec![0; 12 << 20]).build(), &opts(None)),
        Err(Error::Limit(l)) if l.kind == LimitKind::ZipRatio
    ));
    // the total size
    let small = ExtractOptions { limits: Some(Limits { max_zip_bytes: 1000, ..Limits::default() }), ..Default::default() };
    assert!(matches!(Odt::default().extract(&odt(), &small), Err(Error::Limit(_))));
    // input size
    let tiny = ExtractOptions { limits: Some(Limits { max_input_bytes: 100, ..Limits::default() }), ..Default::default() };
    assert!(matches!(Odt::default().extract(&odt(), &tiny), Err(Error::Limit(l)) if l.kind == LimitKind::InputBytes));
}

#[test]
fn xml_bombs_and_xxe() {
    let bombs = [
        r#"<!DOCTYPE x [<!ENTITY a "aaaa"><!ENTITY b "&a;&a;&a;&a;"><!ENTITY c "&b;&b;&b;&b;">]>"#,
        r#"<!DOCTYPE x [<!ENTITY xxe SYSTEM "file:///etc/passwd">]>"#,
        r#"<!DOCTYPE x [<!ENTITY % p SYSTEM "http://127.0.0.1:1/x.dtd"> %p;]>"#,
        r#"<!DOCTYPE x SYSTEM "http://127.0.0.1:1/x.dtd">"#,
        "<!DOCTYPE x>",
    ];
    for b in bombs {
        // in the parsed parts, in a part nobody parses, and in the manifest
        let c = odt_content().replacen("<office:document-content", &format!("{b}<office:document-content"), 1);
        refuse(&Pkg::new(c.clone()).build());
        refuse(&Pkg::new(odt_content()).styles(odt_styles().replacen("<office:document-styles", &format!("{b}<office:document-styles"), 1)).build());
        let other = format!("<?xml version=\"1.0\"?>{b}<settings/>");
        let r = Odt::default().extract(&Pkg::new(odt_content()).extra("settings.xml", other.into_bytes()).build(), &opts(None));
        assert_eq!(r.is_err(), b.contains("<!ENTITY"), "a part nobody parses is refused only for entity declarations: {b}");
    }
    // entity references in text are refused unless predefined
    for e in ["&xxe;", "&#0;", "&#xD800;", "&#x110000;", "&a", "&;", "&#;", "&#x;"] {
        refuse(&Pkg::new(odt_content().replace("Getting started", e)).build());
    }
    // predefined and numeric ones are read
    let ok = Pkg::new(odt_content().replace("Getting started", "A &amp; B &lt; C &#x41; &#66;")).build();
    assert_eq!(units("odt", &ok)[0].1, "A & B < C A B");
}

#[test]
fn deep_and_huge_xml() {
    // far past the depth limit: an error, not a stack overflow
    let deep = odt_content().replace("Getting started", &format!("{}x{}", "<text:span>".repeat(100_000), "</text:span>".repeat(100_000)));
    assert!(matches!(Odt::default().extract(&Pkg::new(deep).build(), &opts(None)), Err(Error::Limit(l)) if l.kind == LimitKind::XmlDepth));
    // nesting inside the limit works end to end
    let nested = odt_content().replace("Getting started", &format!("{}word{}", "<text:span>".repeat(120), "</text:span>".repeat(120)));
    let data = Pkg::new(nested).build();
    assert!(exercise(&Odt::default(), &data, Some("de-DE")));
    // more elements than allowed
    let many = odt_content().replace("Getting started", &"<text:bookmark text:name=\"b\"/>".repeat(2000));
    let l = ExtractOptions { limits: Some(Limits { max_xml_elements: 500, ..Limits::default() }), ..Default::default() };
    assert!(matches!(Odt::default().extract(&Pkg::new(many).build(), &l), Err(Error::Limit(l)) if l.kind == LimitKind::XmlElements));
    // a long attribute and a long name
    let long = odt_content().replace("<text:p>3</text:p>", &format!("<text:p text:style-name=\"{}\">3</text:p>", "n".repeat(500_000)));
    assert!(exercise(&Odt::default(), &Pkg::new(long).build(), None));
    let name = odt_content().replace("<text:p>3</text:p>", &format!("<text:{} />", "n".repeat(100_000)));
    refuse(&Pkg::new(name).build());
}

#[test]
fn malformed_xml_is_a_parse_error() {
    let cases = [
        "",
        "   ",
        "<",
        "<a",
        "<a>",
        "</a>",
        "<a></b>",
        "<a/><b/>",
        "<office:document-content/>",
        "<a x=1/>",
        "<a x=\"1\"",
        "<a x=\"1\" x=\"2\"/>",
        "<a><!-- unterminated</a>",
        "<a><![CDATA[ unterminated</a>",
        "<a><?pi</a>",
        "<a>\u{0}</a>",
        "\u{1}<a/>",
        "<a>&amp</a>",
        "<1a/>",
        "<a:b:c xmlns:a=\"u\"/>",
        "<a xmlns:p=\"\"><p:b/></a>",
        "text",
        "\u{feff}",
    ];
    for c in cases {
        let e = refuse(&Pkg::new(c.to_string()).build());
        assert!(matches!(e, Error::Parse { .. } | Error::Limit(_)), "{c:?}: {e:?}");
    }
}

#[test]
fn every_prefix_of_a_package_and_of_its_xml_is_refused_or_read() {
    let data = odt();
    let mut ok = 0;
    for n in (0..data.len()).step_by(7) {
        if exercise(&Odt::default(), &data[..n], Some("de")) {
            ok += 1;
        }
    }
    assert_eq!(ok, 0, "a truncated zip must not be read");
    let c = odt_content();
    let s = odt_styles();
    for n in (0..c.len()).step_by(11) {
        // cut on a char boundary
        let mut k = n;
        while !c.is_char_boundary(k) {
            k -= 1;
        }
        exercise(&Odt::default(), &Pkg::new(c[..k].to_string()).styles(s.clone()).build(), Some("ar"));
    }
    for n in (0..s.len()).step_by(13) {
        exercise(&Odt::default(), &Pkg::new(c.clone()).styles(s[..n].to_string()).build(), Some("ar"));
    }
}

#[test]
fn every_family_survives_translation_into_every_test_locale() {
    let c = std::fs::read_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/odf")).unwrap();
    for f in c.flatten() {
        let name = f.file_name().to_string_lossy().to_string();
        let bytes = std::fs::read(f.path()).unwrap();
        for lang in ["de-DE", "ja", "zh-Hans", "ko", "ar", "he", "th", "hi", "-", "x", ""] {
            assert!(exercise(filter_for(name.rsplit('.').next().unwrap()).as_ref(), &bytes, Some(lang)), "{name} {lang}");
        }
    }
}

fn corpus_xml() -> Vec<String> {
    vec![odt_content(), odt_styles()]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(std::env::var("ODF_PROPTEST_CASES").ok().and_then(|v| v.parse().ok()).unwrap_or(300)))]

    #[test]
    fn random_bytes_never_panic(b in proptest::collection::vec(any::<u8>(), 0..400)) {
        exercise(&Odt::default(), &b, None);
        let mut pk = b"PK\x03\x04".to_vec();
        pk.extend_from_slice(&b);
        exercise(&Odt::default(), &pk, Some("ja"));
    }

    #[test]
    fn random_text_in_a_valid_package_never_panics(s in "\\PC{0,200}") {
        exercise(&Odt::default(), &Pkg::new(s.clone()).build(), None);
        let wrapped = format!("<?xml version=\"1.0\"?><office:document-content {NSDECL}><office:body><office:text><text:p>{s}</text:p></office:text></office:body></office:document-content>");
        exercise(&Odt::default(), &Pkg::new(wrapped).build(), Some("de"));
    }

    #[test]
    fn mutated_xml_never_panics(
        which in 0usize..2,
        at in 0usize..10_000,
        len in 0usize..40,
        op in 0usize..4,
        junk in proptest::collection::vec(prop::sample::select(vec!['<', '>', '&', ';', '"', '\'', '/', ' ', '=', '!', '?', '-', '[', ']', ':', 'x', '\u{e9}', '\u{65e5}', '\r', '\n']), 0..8),
    ) {
        let mut parts = corpus_xml();
        let s = &mut parts[which];
        let mut a = at % (s.len() + 1);
        while !s.is_char_boundary(a) { a -= 1; }
        let mut b = (a + len).min(s.len());
        while !s.is_char_boundary(b) { b += 1; }
        let junk: String = junk.into_iter().collect();
        match op {
            0 => { s.replace_range(a..b, ""); }
            1 => { s.insert_str(a, &junk); }
            2 => { s.replace_range(a..b, &junk); }
            _ => { let dup = s[a..b].to_string(); s.insert_str(b, &dup); }
        }
        let pkg = Pkg::new(parts[0].clone()).styles(parts[1].clone()).build();
        exercise(&Odt::default(), &pkg, Some("ar"));
        exercise(&Odt::with_options(OdfOptions { translate_comments: true }), &pkg, None);
    }

    #[test]
    fn any_target_text_merges_and_reads_back(t in "\\PC{0,60}", spaces in "[a-z ]{1,30}") {
        let data = odt();
        let f = Odt::default();
        let mut d = f.extract(&data, &opts(Some("de"))).unwrap();
        d.units[1].segments[0].target = vec![Inline::text(t.clone())];
        d.units[0].segments[0].target = vec![Inline::text(spaces.clone())];
        let out = f.merge(&d).unwrap();
        let back = f.extract(&out, &opts(None)).unwrap();
        prop_assert!(back.units.len() <= d.units.len());
        // letters and spaces come back exactly
        if spaces.chars().any(|c| c.is_ascii_lowercase()) {
            prop_assert_eq!(plain_text(&back.units[0].source()), spaces);
        }
    }
}

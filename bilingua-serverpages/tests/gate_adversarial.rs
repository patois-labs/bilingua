//! Hostile TRANSLATIONS: whatever a translator, a machine or an attacker writes into a unit, a server block can be
//! neither opened nor closed, nothing is added to the page's markup, and every block is written back byte for byte.
mod common;
use bilingua_model::*;
use bilingua_serverpages::Dialect;
use common::*;
use proptest::prelude::*;

fn dialect(fmt: &str) -> Dialect {
    Dialect::from_extension(filter(fmt).extensions()[0]).unwrap()
}

const EVIL: &[&str] = &[
    "<?php system('id'); ?>",
    "?>",
    "<?",
    "<?=",
    "%>",
    "<%",
    "<%= x %>",
    "<%-- ",
    "--%>",
    "${evil}",
    "#{evil}",
    "\\${evil}",
    "-->",
    "<!--#exec cmd=\"x\" -->",
    "\"><script>alert(1)</script>",
    "' onfocus='x",
    "\" onfocus=\"x",
    "</p><p>",
    "</script>",
    "<jsp:scriptlet>x</jsp:scriptlet>",
    "\u{FDD0}0\u{FDD1}",
    "\u{FDD0}",
    "\u{27E6}x1\u{27E7}\u{27E6}g1\u{27E7}\u{27E6}/g1\u{27E7}\u{27E6}x99\u{27E7}",
    "&#60;?php echo 1; ?&#62;",
    "&lt;%= x %&gt;",
    "<img src=x onerror=alert(1)>",
    "<!--",
    "<![CDATA[",
    "x\" y='",
    "=\">",
    "`id`",
    "\r\n\r\n",
];

/// Every unit of the page translated to `evil` (plus its own codes where it has any).
fn hostile(fmt: &str, data: &[u8], evil: &str) -> Option<String> {
    let f = filter(fmt);
    let mut doc = f.extract(data, &opts()).ok()?;
    let ids: Vec<(String, Vec<Inline>)> = doc.units.iter().map(|u| (u.segments[0].id.clone(), u.source())).collect();
    for (id, src) in &ids {
        let mut t = vec![Inline::text(format!("a {evil}"))];
        for i in src {
            if matches!(i, Inline::Tag { .. }) {
                t.push(Inline::text(format!(" b {evil} ")));
                t.push(i.clone());
            }
        }
        doc.set_target(id, t).unwrap();
    }
    let out = f.merge(&doc).ok()?;
    Some(String::from_utf8(out).unwrap())
}

fn assert_code_intact(fmt: &str, src: &str, out: &str, what: &str) {
    let d = dialect(fmt);
    let mut a = d.code_islands(src).unwrap();
    let mut b = d.code_islands(out).unwrap_or_else(|e| panic!("{fmt} {what}: the output no longer lexes: {e}\n{out}"));
    a.sort();
    b.sort();
    assert_eq!(a, b, "{fmt} {what}: the page's code changed\n{out}");
}

#[test]
fn hostile_texts_cannot_open_or_close_code() {
    for fmt in FORMATS {
        let src = std::fs::read_to_string(data_dir().join(format!("suite.{fmt}"))).unwrap();
        for evil in EVIL {
            let Some(out) = hostile(fmt, src.as_bytes(), evil) else { continue };
            assert_code_intact(fmt, &src, &out, evil);
            // and the structure: the same number of units, the same elements
            let back = extract(fmt, out.as_bytes()).unwrap_or_else(|e| panic!("{fmt} {evil:?}: {e}\n{out}"));
            let want = extract(fmt, src.as_bytes()).unwrap().units.len();
            assert_eq!(back.units.len(), want, "{fmt} {evil:?}: unit count\n{out}");
        }
    }
}

#[test]
fn hostile_attribute_values_stay_inside_their_attribute() {
    let page = "<p>Hi</p><a href=\"/x\" title=\"Open the inbox\">Go</a><img src=\"i.png\" alt=\"A cat\"><meta name=\"description\" content=\"About us\">";
    for fmt in FORMATS {
        let f = filter(fmt);
        let doc0 = f.extract(page.as_bytes(), &opts()).unwrap();
        let attrs: Vec<usize> = doc0.units.iter().enumerate().filter(|(_, u)| u.context.style.is_some()).map(|(i, _)| i).collect();
        assert!(attrs.len() >= 3, "{fmt}: {attrs:?}");
        for evil in EVIL {
            let mut doc = doc0.clone();
            for &k in &attrs {
                let id = doc.units[k].segments[0].id.clone();
                doc.set_target(&id, vec![Inline::text(format!("a {evil}"))]).unwrap();
            }
            let out = String::from_utf8(f.merge(&doc).unwrap()).unwrap();
            // the same elements with the same attribute names: no new attribute, no new element
            let norm = |s: &str| -> Vec<String> {
                let mut names = Vec::new();
                let mut rest = s;
                while let Some(i) = rest.find('<') {
                    let tag = &rest[i..];
                    let end = tag.find('>').map_or(tag.len(), |e| e + 1);
                    let t = &tag[..end];
                    names.push(t.split(|c: char| c.is_whitespace() || c == '>').next().unwrap().to_string());
                    rest = &tag[end..];
                }
                names
            };
            assert_eq!(norm(&out), norm(page), "{fmt} {evil:?}: elements changed\n{out}");
            assert!(!out.contains("onerror=alert") || out.contains("&lt;img src=x onerror") || out.contains("&lt;img&#32;src"), "{fmt} {evil:?}\n{out}");
            let back = f.extract(out.as_bytes(), &opts()).unwrap();
            assert_eq!(back.units.len(), doc0.units.len(), "{fmt} {evil:?}");
        }
    }
}

#[test]
fn code_in_attributes_and_splitting_text_runs_is_kept() {
    let cases: &[(&str, &str)] = &[
        ("phtml", "<p class=\"<?= $c ?> x\" title=\"<?php echo $t ?>\">Hello <?= $a ?> and <?php echo $b; ?> more</p>"),
        ("jsp", "<p class=\"${c} x\" title=\"<%= t %>\">Hello ${a} and <%= b %> more</p>"),
        ("aspx", "<p class=\"<%: c %> x\" title=\"<%# t %>\">Hello <%: a %> and <%= b %> more</p>"),
        ("erb", "<p class=\"<%= c %> x\" title=\"<%= t %>\">Hello <%= a %> and <%== b %> more</p>"),
        ("shtml", "<p class=\"<!--#echo var=\"C\" --> x\">Hello <!--#echo var=\"A\" --> and <!--#fsize file=\"b\" --> more</p>"),
    ];
    for (fmt, src) in cases {
        let u = texts_str(fmt, src);
        assert_eq!(u, vec!["Hello \u{27E6}x1\u{27E7} and \u{27E6}x2\u{27E7} more".to_string()], "{fmt}");
        // the attributes that hold code are not units
        let out = build_str(fmt, src, &[(0, "Bonjour \u{27E6}x2\u{27E7} et \u{27E6}x1\u{27E7} encore")]);
        assert!(out.contains("Bonjour"), "{fmt}: {out}");
        for island in dialect(fmt).code_islands(src).unwrap() {
            assert!(out.contains(&island), "{fmt}: {island} lost\n{out}");
        }
        assert_code_intact(fmt, src, &out, "reordered");
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 300, ..ProptestConfig::default() })]

    #[test]
    fn any_translation_text_keeps_the_code(evil in "[<>?%$#{}\"'`&;=/\\\\!\\-\\[\\] a-z\u{e9}\u{65e5}\n\u{FDD0}\u{FDD1}\u{27E6}\u{27E7}0-9]{0,40}", idx in 0usize..5) {
        let fmt = FORMATS[idx];
        let src = std::fs::read_to_string(data_dir().join(format!("suite.{fmt}"))).unwrap();
        if let Some(out) = hostile(fmt, src.as_bytes(), &evil) {
            assert_code_intact(fmt, &src, &out, &evil);
            let back = extract(fmt, out.as_bytes()).unwrap();
            prop_assert_eq!(back.units.len(), extract(fmt, src.as_bytes()).unwrap().units.len());
        }
    }
}

#[test]
fn dropped_duplicated_and_reordered_tags_never_lose_or_repeat_code() {
    // tags set on the segment directly, as a buggy editor or a model might
    let src = "<p>Hello <?= $a ?>, <b>bold <?php echo $b; ?></b> end <?= $c ?>.</p>";
    let f = filter("phtml");
    let doc0 = f.extract(src.as_bytes(), &opts()).unwrap();
    let tags: Vec<Inline> = doc0.units[0].source().into_iter().filter(|i| matches!(i, Inline::Tag { .. })).collect();
    let cases: Vec<Vec<Inline>> = vec![
        vec![],
        tags.clone(),
        tags.iter().rev().cloned().collect(),
        tags.iter().chain(tags.iter()).cloned().collect(),
        vec![tags[0].clone(), tags[0].clone(), tags[0].clone()],
        vec![Inline::tag("77", TagKind::Standalone, "x"), tags[0].clone()],
        vec![Inline::tag("1", TagKind::Open, "x"), Inline::tag("1", TagKind::Close, "x")],
    ];
    for t in cases {
        let mut doc = doc0.clone();
        let mut target = vec![Inline::text("T ")];
        for i in t {
            target.push(i);
            target.push(Inline::text(" "));
        }
        doc.units[0].segments[0].target = target;
        let out = String::from_utf8(f.merge(&doc).unwrap()).unwrap();
        for island in ["<?= $a ?>", "<?php echo $b; ?>", "<?= $c ?>"] {
            assert_eq!(out.matches(island).count(), 1, "{island} in {out}");
        }
    }
}

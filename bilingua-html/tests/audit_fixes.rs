//! Regression tests for the 0.9.1 security audit.
use bilingua_html::{sanitize_html, Html};
use bilingua_htmlcore::browser::browser_views;
use bilingua_model::*;
use std::time::{Duration, Instant};

fn extract(page: &str) -> Document {
    Html.extract(page.as_bytes(), &ExtractOptions::default()).unwrap_or_else(|e| panic!("{e:?}"))
}

// --- optional-end-stack-scan-quadratic ------------------------------------------------------------------------

/// Extract, untouched merge, preview; the time of all three.
fn open_merge_preview(page: &str) -> Duration {
    let t = Instant::now();
    let doc = extract(page);
    assert_eq!(Html.merge(&doc).unwrap(), page.as_bytes(), "untouched merge must be exact");
    assert!(Html.preview(&doc).is_some());
    t.elapsed()
}

/// Thousands of open `<rt>` in one unit, then as many end tags that match none of them: each end tag used to walk
/// the whole inline stack (2.8 s at this size in a release build, far longer in a debug build).
#[test]
fn unmatched_inline_end_tags_over_a_deep_inline_stack_are_linear() {
    let n = 40_000;
    let page = format!("<p>{}{}Hello</p>", "<rt>".repeat(n), "</rp>".repeat(n));
    let t = open_merge_preview(&page);
    assert!(t < Duration::from_secs(20), "took {t:?}");
}

/// The variant where every end tag has a match far down the stack (2.1 s at this size in a release build).
#[test]
fn deep_matches_on_the_inline_stack_are_linear() {
    let n = 30_000;
    let page = format!("<p>{}{}{}Hello</p>", "<rt>".repeat(n), "<rp>".repeat(n), "</rt>".repeat(n));
    let t = open_merge_preview(&page);
    assert!(t < Duration::from_secs(20), "took {t:?}");
}

/// `<li>` and `<dd>` do not close each other, so the block stack grows; end tags that match nothing used to walk
/// all of it (3.9 s at this size in a release build).
#[test]
fn unmatched_block_end_tags_over_a_deep_block_stack_are_linear() {
    let n = 80_000;
    let page = format!("{}{}<p>Hello</p>", "<li><dd>".repeat(n / 2), "</div>".repeat(n));
    let t = open_merge_preview(&page);
    assert!(t < Duration::from_secs(20), "took {t:?}");
}

/// The inline codes pair up exactly as before: an end tag closes the innermost open element of its name, and one
/// with nothing to close is a point code.
#[test]
fn inline_end_tags_pair_as_before() {
    let doc = extract("<p>a<b>b<i>c<b>d</b>e</b>f</i>g</u>h</b>i</p>");
    let shown: String = doc.units[0]
        .source()
        .iter()
        .map(|i| match i {
            Inline::Text(t) => t.clone(),
            Inline::Tag { id, kind, .. } => match kind {
                TagKind::Open => format!("[{id}]"),
                TagKind::Close => format!("[/{id}]"),
                _ => format!("[{id}/]"),
            },
        })
        .collect();
    assert_eq!(shown, "a[1]b[2]c[3]d[/3]e[/1]f[/2]g[4/]h[5/]i");
}

/// The place of a unit (its element and path) is found as before when end-tag-optional elements pile up and end
/// tags match nothing or match far down.
#[test]
fn places_are_unchanged() {
    let doc = extract("<ul><li>one<dd>two</span></div><li>three</ul><div><p>four</section><table><tr><td>five</table>six</div>");
    let got: Vec<(String, String)> = doc.units.iter().map(|u| (plain_text(&u.source()), u.context.location.clone().unwrap_or_default())).collect();
    let texts: Vec<&str> = got.iter().map(|g| g.0.as_str()).collect();
    assert_eq!(texts, ["one", "two", "three", "four", "five", "six"]);
    let paths: Vec<&str> = got.iter().map(|g| g.1.as_str()).collect();
    assert_eq!(
        paths,
        [
            "ul:nth-of-type(1) > li:nth-of-type(1)",
            "ul:nth-of-type(1) > li:nth-of-type(1) > dd:nth-of-type(1)",
            "ul:nth-of-type(1) > li:nth-of-type(1) > dd:nth-of-type(1) > li:nth-of-type(1)",
            "div:nth-of-type(1) > p:nth-of-type(1)",
            "div:nth-of-type(1) > table:nth-of-type(1) > tr:nth-of-type(1) > td:nth-of-type(1)",
            "div:nth-of-type(1)",
        ]
    );
}

// --- tokenizer-differential-script-survives -------------------------------------------------------------------

/// What a browser would run in `html`: script elements, event handlers, `srcdoc` and `<base>`, in every parsing
/// mode (scripting on and off, both `<select>` parsers).
fn active(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    for v in browser_views(html) {
        for st in &v.starts {
            if st.name == "script" || st.name == "base" {
                out.push(format!("<{}> at {}", st.name, st.start));
            }
            for (a, val) in &st.attrs {
                if (a.starts_with("on") && a.len() > 2) || a == "srcdoc" {
                    out.push(format!("<{}> {a}={val:?} at {}", st.name, st.start));
                }
            }
        }
    }
    out
}

fn assert_inert(page: &str) {
    let direct = sanitize_html(page, false);
    assert_eq!(active(&direct), Vec::<String>::new(), "sanitize_html\n in: {page}\nout: {direct}");
    assert_eq!(sanitize_html(&direct, false), direct, "a second pass changes nothing\n in: {page}");
    if let Ok(doc) = Html.extract(page.as_bytes(), &ExtractOptions::default()) {
        if let Some(pv) = Html.preview(&doc) {
            assert_eq!(active(&pv), Vec::<String>::new(), "preview\n in: {page}\nout: {pv}");
        }
    }
}

const AUDIT_PAYLOADS: [&str; 10] = [
    "<!--><script>alert(1)</script>-->",
    "<!---><script>alert(8)</script>-->",
    "<!-- x --!><script>alert(5)</script>-->",
    "<![CDATA[><script>alert(4)</script>]]>",
    "<svg><style><img src=x onerror=alert(3)></style></svg>",
    "<svg><textarea><img src=x onerror=alert(9)></textarea></svg>",
    "<svg><title><img src=x onerror=alert(10)></title></svg>",
    "<math><style><img src=x onerror=alert(11)></style></math>",
    "<<script></script>img src=x onerror=alert(2)>",
    "<<base>img src=x onerror=alert(6)>",
];

/// The ten pages of the audit record: no script element and no event handler is left, in the preview and in
/// `sanitize_html`.
#[test]
fn audit_payloads_are_inert() {
    for p in AUDIT_PAYLOADS {
        assert_inert(&format!("<!doctype html><html><body><p>Hello world</p>{p}</body></html>"));
        assert_inert(p);
    }
    // the controls of the record
    assert_inert("<script>alert(0)</script><img src=x onerror=alert(0)>");
    assert_inert("<!-- <script>alert(0)</script> -->");
}

/// More of the same family: places where this sanitizer and a browser could read the page differently.
#[test]
fn other_differentials_are_inert() {
    for p in [
        // a raw-text element the browser ends inside what looks like an attribute value or a comment
        "<xmp><a title=\"</xmp><img src=x onerror=alert(1)>\">",
        "<noscript><a title=\"</noscript><img src=x onerror=alert(1)>\">",
        "<iframe><!--</iframe><img src=x onerror=alert(1)>-->",
        "<noembed><p id='</noembed><script>alert(1)</script>'>",
        "<style><a title=\"</style><img src=x onerror=alert(1)>\">",
        "<textarea><!--</textarea><img src=x onerror=alert(1)>-->",
        "<title><a title=\"</title><img src=x onerror=alert(1)>\">",
        "<plaintext></plaintext><img src=x onerror=alert(1)>",
        // foreign content
        "<svg><![CDATA[<img src=x onerror=alert(1)>]]></svg>",
        "<svg><style><!--</style><img src=x onerror=alert(1)>--></svg>",
        "<svg><style><!--><img src=x onerror=alert(1)></style></svg>",
        "<math><mtext><title><img src=x onerror=alert(1)></title></mtext></math>",
        "<svg><desc><textarea></desc><img src=x onerror=alert(1)></textarea></svg>",
        "<svg><script>alert(1)</script></svg>",
        "<svg><script href=x></svg><svg><script>alert(1)<b></script>",
        // bogus comments and end tags with attributes
        "<? ><script>alert(1)</script>",
        "</ ><script>alert(1)</script>",
        "</><script>alert(1)</script>",
        "<!x <script>alert(1)</script>>",
        "</p title=\"><img src=x onerror=alert(1)>\">",
        "</p \"><img src=x onerror=alert(1)>",
        "<![CDATA[ <script>alert(1)</script>",
        // joins and cuts
        "<scr<script>ipt>alert(1)</script>",
        "<<!-- -->script>alert(1)</script>",
        "<img src=x o<script></script>nerror=alert(1)>",
        "<img src=x on<!-- -->error=alert(1)>",
        "<a b=\"1\"onclick=\"2\"onfocus=\"3\"c=\"4\">x</a>",
        "<a <img src=x onerror=alert(1)>>",
        "<a<img src=x onerror=alert(1)>>",
        "<a href=x<img/src/onerror=alert(1)>",
        "<img src=x onerror",
        "<img src=x onerror=\"alert(1)",
        "<select><style></select><img src=x onerror=alert(1)></style>",
        "<select><textarea><img src=x onerror=alert(1)></textarea></select>",
        "<script>x</script",
        "<script><!--<script></script><img src=x onerror=alert(1)>--></script>",
        "<p>a</p><script>",
    ] {
        assert_inert(p);
        assert_inert(&format!("<!doctype html><html><head><title>T</title></head><body><p>Hello world</p>{p}<p>after</p></body></html>"));
    }
    // a script URL an SVG animation would write into a link
    let out = sanitize_html("<svg><a><set attributeName=href to=javascript:alert(1)><animate attributeName=href values=\"x;javascript:alert(1)\"></a></svg>", false);
    assert!(!out.to_ascii_lowercase().contains("javascript:"), "{out}");
}

/// Generated pages from markup-shaped pieces: none leaves anything active, and a second pass changes nothing.
#[test]
fn generated_pages_are_inert() {
    const PIECES: [&str; 64] = [
        "<", ">", "</", "<!--", "-->", "--!>", "<!-->", "<!--->", "<![CDATA[", "]]>", "<?", "<!", "\"", "'", "=", " ", "/", "-", "!", "x", "<p>", "</p>",
        "<script>", "</script>", "<script ", "<style>", "</style>", "<title>", "</title>", "<textarea>", "</textarea>", "<xmp>", "</xmp>", "<iframe>",
        "</iframe>", "<noscript>", "</noscript>", "<noembed>", "</noembed>", "<plaintext>", "<svg>", "</svg>", "<math>", "</math>", "<mtext>", "<desc>",
        "</desc>", "<foreignObject>", "<annotation-xml encoding=text/html>", "<select>", "</select>", "<option>", "<img src=x onerror=alert(1)>",
        "<img src=x onerror=alert(1) ", "onclick=alert(1)", " onload=\"alert(1)\"", "<a title=\"", "<a href='", "<base href=x>", "<base ", "<br>", "</br>",
        "<div ", "srcdoc=x",
    ];
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next = move || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as usize
    };
    for _ in 0..4000 {
        let len = 1 + next() % 14;
        let page: String = (0..len).map(|_| PIECES[next() % PIECES.len()]).collect();
        let out = sanitize_html(&page, false);
        assert_eq!(active(&out), Vec::<String>::new(), "\n in: {page}\nout: {out}");
        assert_eq!(sanitize_html(&out, false), out, "a second pass changes nothing\n in: {page}");
        let injected = sanitize_html(&page, true);
        assert_eq!(active(&injected), Vec::<String>::new(), "\n in: {page}\nout: {injected}");
    }
}

/// What the sanitizer does to an ordinary page: tags, attributes, the doctype and style sheets are copied as typed.
#[test]
fn ordinary_markup_is_copied_as_typed() {
    let page = concat!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>Kettles &amp; pots</title>\n",
        "<style><!--\n p > b { color: red } a::after { content: \"<\" }\n--></style>\n</head>\n<body class=main>\n",
        "<h1 id='top'>Kettles</h1>\n<p>Fast &amp; quiet, 1 &lt; 2 > 0. <a href=\"/x?a=1&amp;b=2\" title=\"a > b\">More</a><br/>\n",
        "<img src=k.png alt=\"A kettle\" /></p >\n<textarea name=t>a &lt;b&gt; c</textarea>\n<svg viewBox=\"0 0 1 1\"><title>Logo</title><path d=\"M0 0\"/></svg>\n",
        "</body>\n</html>\n"
    );
    assert_eq!(sanitize_html(page, false), page);
}

/// And the few things it now writes differently for input that is not hostile.
#[test]
fn ordinary_input_that_is_rewritten() {
    // comments, processing instructions and bogus comments are left out
    assert_eq!(sanitize_html("<?xml version=\"1.0\"?><p>a<!-- note -->b</p><!--[if IE]><p>old</p><![endif]-->", false), "<p>ab</p>");
    // a `<` that starts no tag is written as a reference, in text and in attribute values
    assert_eq!(sanitize_html("<p title=\"a<b\">1 < 2 <3</p>", false), "<p title=\"a&lt;b\">1 &lt; 2 &lt;3</p>");
    assert_eq!(sanitize_html("<title>a <b> c</title><textarea>if a<b</textarea>", false), "<title>a &lt;b> c</title><textarea>if a&lt;b</textarea>");
    // junk in an end tag is dropped
    assert_eq!(sanitize_html("<p>a</p class=x>", false), "<p>a</p>");
    // the text of a CDATA section stays, its markers go
    assert_eq!(sanitize_html("<svg><text><![CDATA[a < b]]></text></svg>", false), "<svg><text>a &lt; b</text></svg>");
    // a tag in a style sheet is written as a CSS escape
    assert_eq!(sanitize_html("<style>a::after{content:\"</b>\"}</style>", false), "<style>a::after{content:\"\\3c /b>\"}</style>");
    // a tag that runs into the end of the input is dropped, as a browser drops it
    assert_eq!(sanitize_html("<p>a</p><a href=\"x", false), "<p>a</p>");
}

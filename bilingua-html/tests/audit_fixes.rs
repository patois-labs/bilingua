//! Regression tests for the 0.9.1 security audit.
use bilingua_html::{sanitize_html, Html};
use bilingua_htmlcore::browser::{body_budget, browser_views};
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

/// What the allowlist keeps of an ordinary page: text, formatting, links, images, the doctype, a meta charset and
/// the style sheet; tags are rebuilt with double quotes, and SVG goes.
#[test]
fn ordinary_markup_keeps_what_a_translator_sees() {
    let page = concat!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>Kettles &amp; pots</title>\n",
        "<style><!--\n p > b { color: red } a::after { content: \"<\" }\n--></style>\n</head>\n<body class=main>\n",
        "<h1 id='top'>Kettles</h1>\n<p>Fast &amp; quiet, 1 &lt; 2 > 0. <a href=\"/x?a=1&amp;b=2\" title=\"a > b\">More</a><br/>\n",
        "<img src=k.png alt=\"A kettle\" /></p >\n<textarea name=t>a &lt;b&gt; c</textarea>\n<svg viewBox=\"0 0 1 1\"><title>Logo</title><path d=\"M0 0\"/></svg>\n",
        "</body>\n</html>\n"
    );
    let want = concat!(
        "<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<title>Kettles &amp; pots</title>\n",
        "<style><!--\n p > b { color: red } a::after { content: \"<\" }\n--></style>\n</head>\n<body class=\"main\">\n",
        "<h1 id=\"top\">Kettles</h1>\n<p>Fast &amp; quiet, 1 &lt; 2 > 0. <a href=\"/x?a=1&amp;b=2\" title=\"a &gt; b\">More</a><br>\n",
        "<img src=\"k.png\" alt=\"A kettle\"></p>\n<textarea name=\"t\">a &lt;b&gt; c</textarea>\n\n",
        "</body>\n</html>\n"
    );
    assert_eq!(sanitize_html(page, false), want);
    assert_eq!(sanitize_html(want, false), want);
    // lists, tables, emphasis, http, https, mailto and tel links, raster data images
    let keep = concat!(
        "<ul><li><b>a</b> <i>b</i> <u>c</u> <s>d</s> <sup>1</sup><sub>2</sub></li></ul><ol start=\"3\"><li>x</li></ol>",
        "<table border=\"1\"><tr><th colspan=\"2\">h</th></tr><tr><td>1</td><td style=\"color:red\">2</td></tr></table>",
        "<a href=\"https://example.com/a\">s</a><a href=\"http://example.com/\">h</a><a href=\"mailto:a@example.com\">m</a>",
        "<a href=\"tel:+15551234\">t</a><a href=\"#top\">r</a><img src=\"data:image/png;base64,iVBORw0KGgo=\" alt=\"p\">",
        "<span data-bl-seg=\"u1\" aria-label=\"x\">seg</span>"
    );
    assert_eq!(sanitize_html(keep, false), keep);
}

/// And the few things it writes differently for input that is not hostile.
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
    assert_eq!(sanitize_html("<p><![CDATA[a < b]]></p>", false), "<p>a &lt; b</p>");
    // a tag in a style sheet is written as a CSS escape
    assert_eq!(sanitize_html("<style>a::after{content:\"</b>\"}</style>", false), "<style>a::after{content:\"\\3c /b>\"}</style>");
    // a tag that runs into the end of the input is dropped, as a browser drops it
    assert_eq!(sanitize_html("<p>a</p><a href=\"x", false), "<p>a</p>");
    // an element that is not allowed goes and its text stays; one that holds code goes with its content
    assert_eq!(sanitize_html("<form action=\"/x\"><p>a</p><input type=\"text\" value=\"v\"></form>", false), "<p>a</p><input type=\"text\" value=\"v\">");
    assert_eq!(sanitize_html("<p>a<svg><text>b</text></svg>c<math><mi>x</mi></math>d<svg/>e</p>", false), "<p>acde</p>");
    // a second attribute of one name is ignored (a browser takes the first)
    assert_eq!(sanitize_html("<a href=\"/ok\" href=\"javascript:alert(1)\">x</a>", false), "<a href=\"/ok\">x</a>");
}

// --- run 2 G3-8: the preview sanitizer is an allowlist -------------------------------------------------------

/// Elements and attributes that must never reach a browser from the sanitizer, in any parsing mode: anything in
/// SVG or MathML, script, frames, objects, forms, base, link, a meta with http-equiv, event handlers, srcdoc,
/// form actions, and a link or image URL with a script or HTML-document scheme.
fn not_allowlisted(html: &str) -> Vec<String> {
    const BAD: [&str; 16] = [
        "script", "svg", "math", "iframe", "frame", "frameset", "object", "embed", "applet", "form", "base", "link", "template", "animate", "set", "use",
    ];
    let mut out = active(html);
    for v in browser_views(html) {
        for st in &v.starts {
            if st.ns != "html" || BAD.contains(&st.name.as_str()) {
                out.push(format!("<{}> ({}) at {}", st.name, st.ns, st.start));
            }
            for (a, val) in &st.attrs {
                let v = val.as_deref().map(bilingua_htmlcore::pyx::unescape).unwrap_or_default();
                let squeezed: String = v.chars().filter(|c| !c.is_whitespace() && !c.is_control()).collect::<String>().to_ascii_lowercase();
                if matches!(a.as_str(), "action" | "formaction" | "http-equiv" | "xlink:href" | "srcset")
                    || (matches!(a.as_str(), "href" | "src" | "cite" | "data") && ["javascript:", "vbscript:", "data:text", "data:image/svg"].iter().any(|s| squeezed.starts_with(s)))
                    || (a == "style" && (squeezed.contains("javascript") || squeezed.contains("expression")))
                {
                    out.push(format!("<{}> {a}={val:?} at {}", st.name, st.start));
                }
            }
        }
    }
    out
}

fn assert_allowlisted(page: &str) {
    for p in [page.to_string(), format!("<!doctype html><html><head><title>T</title></head><body><p>Hello</p>{page}<p>after</p></body></html>")] {
        let out = sanitize_html(&p, false);
        assert_eq!(not_allowlisted(&out), Vec::<String>::new(), "\n in: {p}\nout: {out}");
        assert_eq!(sanitize_html(&out, false), out, "a second pass changes nothing\n in: {p}");
        let low = out.to_ascii_lowercase();
        for w in ["javascript", "vbscript", "srcdoc", "refresh", "expression", "behavior"] {
            assert!(!low.contains(w), "{w} in the output\n in: {p}\nout: {out}");
        }
        if let Ok(doc) = Html.extract(p.as_bytes(), &ExtractOptions::default()) {
            if let Some(pv) = Html.preview(&doc) {
                assert_eq!(not_allowlisted(&pv), Vec::<String>::new(), "preview\n in: {p}\nout: {pv}");
            }
        }
    }
}

/// The two bypasses of the run 2 record (an SVG animation value list split by an encoded semicolon, and a meta
/// refresh written with character references) and the families around them.
#[test]
fn run2_blocklist_bypasses_are_gone() {
    for p in [
        // SMIL values: the separator only appears after decoding
        "<svg><a><animate attributeName=href values=\"x&#59;javascript:alert(1)\"/><text y=10>t</text></a></svg>",
        "<svg><a><animate attributeName=href values=\"x&semi;javascript:alert(1)\"/><text>t</text></a></svg>",
        "<svg><a><set attributeName=xlink:href to=\"&#x6A;avascript:alert(1)\"/><text>t</text></a></svg>",
        "<svg><a xlink:href=\"javascript:alert(1)\"><text>t</text></a></svg>",
        // meta refresh with character references in http-equiv and in content
        "<meta http-equiv=\"&#114;efresh\" content=\"0;url=https://example.com/\">",
        "<meta http-equiv=\"&#x52;&#x45;FRESH\" content=\"0&#59;url=javascript:alert(1)\">",
        "<meta http-equiv=refresh content=\"0;url=javascript:alert(1)\">",
        // script URLs hidden by blanks, controls and references
        "<a href=\"java&#x09;script:alert(1)\">x</a>",
        "<a href=\"jav&#x0A;ascript:alert(1)\">x</a>",
        "<a href=\"jav&#13;ascript:alert(1)\">x</a>",
        "<a href=\" &#14; javascript:alert(1)\">x</a>",
        "<a href=\"javascript&colon;alert(1)\">x</a>",
        "<a href=\"&#106&#97&#118&#97&#115&#99&#114&#105&#112&#116&#58alert(1)\">x</a>",
        "<a href=\"&#x6a;&#x61;&#x76;&#x61;&#x73;&#x63;&#x72;&#x69;&#x70;&#x74;&#x3a;alert(1)\">x</a>",
        "<a href=\"JaVaScRiPt:alert(1)\">x</a>",
        "<a href=javascript&NewLine;:alert(1)>x</a>",
        "<a href=\"java&Tab;script:alert(1)\">x</a>",
        "<img src=\"vbscript:alert(1)\">",
        "<a href=\"data:text/html,&lt;script&gt;alert(1)&lt;/script&gt;\">x</a>",
        "<img src=\"data:image/svg+xml,&lt;svg onload=alert(1)&gt;\">",
        "<blockquote cite=\"javascript:alert(1)\">q</blockquote>",
        // SVG and MathML namespace tricks
        "<math><mtext><table><mglyph><style><img src=x onerror=alert(1)></style></mglyph></table></mtext></math>",
        "<svg><foreignObject><p onclick=alert(1)>x</p></foreignObject></svg>",
        "<math href=\"javascript:alert(1)\"><mi>x</mi></math>",
        "<svg><use href=\"data:image/svg+xml;base64,PHN2Zz48L3N2Zz4=#x\"/></svg>",
        "<math><annotation-xml encoding=\"text/html\"><img src=x onerror=alert(1)></annotation-xml></math>",
        "<svg></p><style><a id=\"</style><img src=1 onerror=alert(1)>\">",
        "<svg><svg/><script>alert(1)</script></svg>",
        // srcdoc, frames, objects, forms, base and link
        "<iframe srcdoc=\"<script>alert(1)</script>\"></iframe>",
        "<iframe srcdoc=\"&lt;img src=x onerror=alert(1)&gt;\"></iframe>",
        "<object data=\"javascript:alert(1)\"></object><embed src=\"javascript:alert(1)\">",
        "<form action=\"javascript:alert(1)\"><button formaction=\"javascript:alert(1)\">x</button></form>",
        "<base href=\"javascript:/\"><a href=\"x\">y</a>",
        "<link rel=stylesheet href=\"javascript:alert(1)\">",
        "<template><img src=x onerror=alert(1)></template>",
        // style attributes and sheets that run code in some browser
        "<p style=\"background:url(javascript:alert(1))\">x</p>",
        "<p style=\"width:expr\\65 ssion(alert(1))\">x</p>",
        "<p style=\"x:ex/**/pression(alert(1))\">x</p>",
        "<p style=\"behavior:url(x.htc)\">x</p>",
        "<style>p{background:url(\"javascript:alert(1)\")}</style>",
        "<style>p{-moz-binding:url(x.xml#y)}</style>",
    ] {
        assert_allowlisted(p);
    }
    // what is not code stays
    assert_eq!(sanitize_html("<p style=\"scroll-behavior:smooth;color:red\">x</p>", false), "<p style=\"scroll-behavior:smooth;color:red\">x</p>");
}

/// The audit payloads of run 1 and the generated pages hold under the stricter oracle too.
#[test]
fn earlier_payloads_meet_the_allowlist() {
    for p in AUDIT_PAYLOADS {
        assert_allowlisted(p);
    }
    const PIECES: [&str; 24] = [
        "<", ">", "</", "<!--", "-->", "&#59;", "&#x3a;", "&colon;", "&Tab;", "javascript:alert(1)", "\"", "=", " ", "<svg>", "<math>", "<a href=",
        "<img src=", "<animate attributeName=href values=", "<meta http-equiv=", "refresh", "<iframe srcdoc=", "<p style=", "</svg>", "x",
    ];
    let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = move || {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (seed >> 33) as usize
    };
    for _ in 0..3000 {
        let len = 1 + next() % 12;
        let page: String = (0..len).map(|_| PIECES[next() % PIECES.len()]).collect();
        let out = sanitize_html(&page, false);
        assert_eq!(not_allowlisted(&out), Vec::<String>::new(), "\n in: {page}\nout: {out}");
        assert_eq!(sanitize_html(&out, false), out, "a second pass changes nothing\n in: {page}");
    }
}

// --- run 2 G3-2: comments that end with `--!>` in a file with no `-->` ----------------------------------------

/// Seconds `f` takes.
fn secs(f: impl FnOnce()) -> f64 {
    let t = Instant::now();
    f();
    t.elapsed().as_secs_f64()
}

/// Many comments that end with `--!>` and no `-->` anywhere: each comment used to search to the end of the file for
/// `-->`. The sanitizer now takes about what a page of as many ordinary tags takes.
#[test]
fn sanitize_comment_bang_endings_linear() {
    let n = 150_000;
    let attack = format!("<p>Hello world</p>{}", "<!--a--!>".repeat(n));
    let control = format!("<p>Hello world</p>{}", "<b>a</b>x".repeat(n));
    let c = secs(|| {
        sanitize_html(&control, true);
    });
    let a = secs(|| {
        assert_eq!(sanitize_html(&attack, false), "<p>Hello world</p>");
    });
    assert!(a < c * 5.0 + 2.0, "comments {a:.2} s, control {c:.2} s");
    // and the preview of such a page
    let doc = extract(&attack);
    let p = secs(|| {
        Html.preview(&doc).unwrap();
    });
    assert!(p < c * 20.0 + 5.0, "preview {p:.2} s, control {c:.2} s");
}

/// The browser view a merge builds: thousands of open `<li>`, then as many end tags that match none of them. Each
/// end tag used to walk the whole open-element stack; now it is one index lookup.
#[test]
fn browser_view_end_tags_do_not_walk_the_open_elements() {
    let n = 60_000;
    let page = format!("<ul>{}{}</ul>", "<li>x".repeat(n), "</span>".repeat(n));
    let t = Instant::now();
    let views = browser_views(&page);
    assert!(!views.is_empty());
    assert_eq!(views[0].starts.iter().filter(|s| s.name == "li").count(), n);
    let t = t.elapsed();
    assert!(t < Duration::from_secs(20), "took {t:?}");
}

/// An end tag still closes the innermost open element of its name and the ones above it, and stops at an SVG
/// integration point: a script opened after the close is read as a script either way.
#[test]
fn browser_view_end_tags_close_as_before() {
    for page in ["<div><b><i>a</b><script>x()</script></div>", "<svg><foreignObject><p>a</div><script>y()</script></foreignObject></svg>"] {
        let views = browser_views(page);
        for v in views {
            assert_eq!(v.bodies.iter().filter(|(n, _, _)| n == "script").count(), 1, "{page}");
        }
    }
}

// --- resource limits: script / style bodies in the export check -------------------------------------------------

/// Extracts `page`, translates its first unit and merges.
fn translated_merge(page: &str) -> Result<Vec<u8>, Error> {
    let mut doc = extract(page);
    let id = doc.units[0].segments[0].id.clone();
    doc.set_target(&id, vec![Inline::text("Hallo")]).expect("set_target");
    Html.merge(&doc)
}

/// Resource limit: SVG lets `<style>` nest, and each level's body holds the text of every level inside it, so the
/// export check's browser view held the page about n/2 times over (n levels). The bodies of one view now stop at
/// twice the page plus 1 MiB, and a page over that is refused at export instead.
#[test]
fn nested_svg_style_bodies_stay_within_the_budget() {
    let n = 20_000;
    let page = format!("<p>Hello</p><svg>{}x{}</svg>", "<style>".repeat(n), "</style>".repeat(n));
    let t = Instant::now();
    let views = browser_views(&page);
    for v in &views {
        assert!(v.bodies_cut, "the bodies were not cut");
        assert!(v.body_bytes <= body_budget(&page), "{} body bytes", v.body_bytes);
    }
    let r = translated_merge(&page);
    assert!(matches!(&r, Err(Error::Write(m)) if m.contains("nested inside each other")), "{:?}", r.map(|b| b.len()));
    let t = t.elapsed();
    assert!(t < Duration::from_secs(20), "took {t:?}");
    // a few levels are an ordinary file and still merge
    let page = "<p>Hello</p><svg><style><style>a{}</style></style></svg>";
    let out = String::from_utf8(translated_merge(page).expect("merge")).unwrap();
    assert!(out.contains("Hallo") && out.contains("<svg><style><style>a{}</style></style></svg>"), "{out}");
}

/// Resource limit: the XML reading of the export check gave each text to every script / style element seen so far,
/// closed or not, and searched from the page's start for each element's line on a page without line breaks, so a
/// well-formed page with many style elements took time in their number squared (137 s for this page in a debug
/// build). Only the open elements receive a text now, and line breaks are counted once.
#[test]
fn many_style_elements_in_a_well_formed_page_are_linear() {
    let n = 200_000;
    let page = format!("<html xmlns=\"http://www.w3.org/1999/xhtml\"><body><p>Hello</p>{}</body></html>", "<style>a</style>".repeat(n));
    let t = Instant::now();
    let out = String::from_utf8(translated_merge(&page).expect("merge")).unwrap();
    assert!(out.contains("<p>Hallo</p>"));
    let t = t.elapsed();
    assert!(t < Duration::from_secs(20), "took {t:?}");
}

/// Resource limit: the XML reading of the export check looked every element and attribute name up through all the
/// namespace declarations in scope, so one element declaring many prefixes above many elements took time in their
/// product. The lookup is now one index probe.
#[test]
fn many_namespace_declarations_in_a_well_formed_page_are_linear() {
    let n = 50_000;
    let decls: String = (0..n).map(|i| format!(" xmlns:p{i}=\"urn:x\"")).collect();
    let page = format!("<html xmlns=\"http://www.w3.org/1999/xhtml\"{decls}><body><p>Hello</p>{}</body></html>", "<i></i>".repeat(n));
    let t = Instant::now();
    let out = String::from_utf8(translated_merge(&page).expect("merge")).unwrap();
    assert!(out.contains("<p>Hallo</p>"));
    let t = t.elapsed();
    assert!(t < Duration::from_secs(20), "took {t:?}");
}

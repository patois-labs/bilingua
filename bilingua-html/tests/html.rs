use bilingua_html::{sanitize_html, Html};
use bilingua_model::*;

fn extract(src: &str) -> Document {
    Html.extract(src.as_bytes(), &ExtractOptions::default()).unwrap()
}

fn extract_to(src: &str, lang: &str) -> Document {
    Html.extract(src.as_bytes(), &ExtractOptions { target_lang: Some(lang.into()), ..Default::default() }).unwrap()
}

fn merged(d: &Document) -> String {
    String::from_utf8(Html.merge(d).unwrap()).unwrap()
}

/// Source with a mark appended to every text piece, as a stand-in for a translation.
fn mark(src: &[Inline]) -> Vec<Inline> {
    src.iter()
        .map(|i| match i {
            Inline::Text(t) => Inline::Text(format!("{t}~")),
            x => x.clone(),
        })
        .collect()
}

fn translate_all(d: &mut Document) {
    let ids: Vec<(String, Vec<Inline>)> = d.segments().filter(|(u, _)| u.translate).map(|(_, s)| (s.id.clone(), mark(&s.source))).collect();
    for (id, t) in ids {
        d.set_target(&id, t).unwrap();
    }
}

fn texts(d: &Document) -> Vec<String> {
    d.units.iter().map(|u| plain_text(&u.source())).collect()
}

fn round_trip(src: &str) -> Document {
    let d = extract(src);
    assert_eq!(merged(&d), src, "untranslated merge must equal the input");
    d
}

// One page per primary language: (lang, dir, title, heading, paragraph with entity, button, alt, link text).
type Page = (&'static str, &'static str, &'static str, &'static str, &'static str, &'static str, &'static str, &'static str);

const PAGES: &[Page] = &[
    ("en", "ltr", "Larchmoor Tools", "Welcome to Larchmoor", "Our <b>kettles</b> boil fast &amp; quietly.", "Add to cart", "A blue kettle", "Read more"),
    ("es", "ltr", "Herramientas Larchmoor", "Bienvenido a Larchmoor", "Nuestras <b>teteras</b> hierven r&aacute;pido y en silencio.", "Añadir al carrito", "Una tetera azul", "Leer más"),
    ("fr", "ltr", "Outils Larchmoor", "Bienvenue chez Larchmoor", "Nos <b>bouilloires</b> chauffent vite et sans bruit.", "Ajouter au panier", "Une bouilloire bleue", "En savoir plus"),
    ("it", "ltr", "Attrezzi Larchmoor", "Benvenuti da Larchmoor", "I nostri <b>bollitori</b> scaldano in fretta e in silenzio.", "Aggiungi al carrello", "Un bollitore blu", "Leggi di più"),
    ("de", "ltr", "Larchmoor Werkzeuge", "Willkommen bei Larchmoor", "Unsere <b>Wasserkocher</b> kochen schnell &amp; leise.", "In den Warenkorb", "Ein blauer Wasserkocher", "Mehr lesen"),
    ("nl", "ltr", "Larchmoor Gereedschap", "Welkom bij Larchmoor", "Onze <b>waterkokers</b> koken snel en stil.", "In winkelwagen", "Een blauwe waterkoker", "Lees meer"),
    ("pt", "ltr", "Ferramentas Larchmoor", "Bem-vindo à Larchmoor", "As nossas <b>chaleiras</b> fervem rápido e em silêncio.", "Adicionar ao carrinho", "Uma chaleira azul", "Ler mais"),
    ("ja", "ltr", "ラーチムア工具", "ラーチムアへようこそ", "当社の<b>ケトル</b>は速くて静かです。", "カートに追加", "青いケトル", "続きを読む"),
    ("zh", "ltr", "拉奇摩尔工具", "欢迎来到拉奇摩尔", "我们的<b>水壶</b>烧得快而且安静。", "加入购物车", "一只蓝色水壶", "阅读更多"),
    ("ko", "ltr", "라치무어 도구", "라치무어에 오신 것을 환영합니다", "저희 <b>주전자</b>는 빠르고 조용합니다.", "장바구니에 담기", "파란 주전자", "더 읽기"),
    ("ar", "rtl", "أدوات لارشمور", "مرحبا بك في لارشمور", "<b>غلاياتنا</b> تغلي بسرعة وبهدوء.", "أضف إلى السلة", "غلاية زرقاء", "اقرأ المزيد"),
];

fn page(p: &Page) -> String {
    let (lang, dir, title, h1, para, button, alt, link) = *p;
    format!(
        "<!DOCTYPE html>\n<html lang=\"{lang}\" dir=\"{dir}\">\n<head>\n  <meta charset=\"utf-8\">\n  <title>{title}</title>\n  <meta name=\"description\" content=\"{h1}\">\n</head>\n<body>\n  <h1 class=\"hero\">{h1}</h1>\n  <p>{para}</p>\n  <img src=\"kettle.png\" alt=\"{alt}\">\n  <button type=\"button\" id=\"b1\">{button}</button>\n  <a href=\"/more\">{link}</a>\n</body>\n</html>\n"
    )
}

#[test]
fn eleven_languages_round_trip_and_translate() {
    for p in PAGES {
        let src = page(p);
        let d = round_trip(&src);
        assert_eq!(d.source_lang.as_deref(), Some(p.0));
        // title, meta content, h1, p, alt, button, link = 7 units
        assert_eq!(d.units.len(), 7, "{}: {:?}", p.0, texts(&d));
        let kinds: Vec<&str> = d.units.iter().map(|u| u.context.kind.as_deref().unwrap()).collect();
        assert_eq!(kinds, ["title", "meta", "heading", "paragraph", "alt", "button", "link"], "{}", p.0);
        // Text survives with entities decoded.
        assert!(texts(&d).iter().any(|t| t.contains(p.3)));

        let mut d = extract_to(&src, p.0);
        translate_all(&mut d);
        let out = merged(&d);
        let again = extract(&out);
        let expected: Vec<String> = texts(&d).iter().map(|t| format!("{t}~")).collect();
        // Inline tags are kept, so the mark is added after each text piece.
        assert_eq!(again.units.len(), 7);
        assert!(out.contains("kettle.png"));
        for (u, t) in again.units.iter().zip(&expected) {
            assert!(plain_text(&u.source()).contains('~'), "{}: {t}", p.0);
        }
    }
}

#[test]
fn arabic_target_sets_lang_and_rtl() {
    let src = page(&PAGES[0]);
    let mut d = extract_to(&src, "ar");
    // Nothing translated: byte for byte, no lang change.
    assert_eq!(merged(&d), src);
    translate_all(&mut d);
    let out = merged(&d);
    assert!(out.contains("<html lang=\"ar\" dir=\"rtl\">"), "{out}");
    // Back to a left-to-right language: dir is left as the original.
    let mut d = extract_to(&src, "fr");
    translate_all(&mut d);
    assert!(merged(&d).contains("<html lang=\"fr\" dir=\"ltr\">"));
}

#[test]
fn rtl_dir_added_when_missing() {
    let src = "<html lang=\"en\"><body><p>Hello there</p></body></html>";
    let mut d = extract_to(src, "ar-EG");
    translate_all(&mut d);
    assert!(merged(&d).starts_with("<html dir=\"rtl\" lang=\"ar-EG\">") || merged(&d).starts_with("<html lang=\"ar-EG\" dir=\"rtl\">"));
    // No lang attribute: none added.
    let src2 = "<html><body><p>Hello there</p></body></html>";
    let mut d2 = extract_to(src2, "fr");
    translate_all(&mut d2);
    assert!(merged(&d2).starts_with("<html><body>"));
}

#[test]
fn japanese_page_translates_in_place() {
    let src = page(&PAGES[7]);
    let mut d = extract_to(&src, "en");
    let h = d.units.iter().find(|u| u.context.kind.as_deref() == Some("heading")).unwrap().id.clone();
    d.set_target(&h, vec![Inline::text("Welcome to Larchmoor")]).unwrap();
    let out = merged(&d);
    assert!(out.contains("<h1 class=\"hero\">Welcome to Larchmoor</h1>"));
    assert!(out.contains("\u{9752}\u{3044}\u{30b1}\u{30c8}\u{30eb}"), "other text untouched");
    assert!(out.contains("<html lang=\"en\" dir=\"ltr\">"));
}

#[test]
fn nested_inline_tags() {
    let src = "<p>One <b>bold <i>and italic</i></b> and <a href=\"/x?a=1&amp;b=2\" title=\"Go there\">a <em>link</em></a>.<br>Next line.</p>\n";
    let d = round_trip(src);
    assert_eq!(d.units.len(), 2, "paragraph and the title attribute");
    let u = d.units[0].clone();
    let seg = u.segments[0].clone();
    let ntags = seg.source.iter().filter(|i| matches!(i, Inline::Tag { .. })).count();
    assert_eq!(ntags, 9, "{:?}", seg.source);
    assert_eq!(plain_text(&seg.source), "One bold and italic and a link.Next line.");
    assert_eq!(u.codes["1"].label.as_deref(), Some("bold"));
    // Translate the paragraph and the attribute.
    let mut d = d;
    let mut t = seg.source.clone();
    for i in t.iter_mut() {
        if let Inline::Text(x) = i {
            *x = x.to_uppercase();
        }
    }
    d.set_target("u1", t).unwrap();
    d.set_target("u2", vec![Inline::text("Ir all\u{ed} \"ya\"")]).unwrap();
    let out = merged(&d);
    assert_eq!(
        out,
        "<p>ONE <b>BOLD <i>AND ITALIC</i></b> AND <a href=\"/x?a=1&amp;b=2\" title=\"Ir all\u{ed} &quot;ya&quot;\">A <em>LINK</em></a>.<br>NEXT LINE.</p>\n"
    );
}

#[test]
fn entities_decode_and_encode() {
    let src = "<p>Fish &amp; chips &lt;2&gt; &copy; 2026 &#8212; caf&eacute; &nbsp;ok &unknown; &#x4e2d;</p>";
    let d = round_trip(src);
    assert_eq!(texts(&d)[0], "Fish & chips <2> \u{a9} 2026 \u{2014} caf\u{e9} \u{a0}ok &unknown; \u{4e2d}");
    let mut d = d;
    d.set_target("u1", vec![Inline::text("A & B < C > D")]).unwrap();
    assert_eq!(merged(&d), "<p>A &amp; B &lt; C &gt; D</p>");
}

#[test]
fn attributes_are_units() {
    let src = concat!(
        "<head><meta name=\"description\" content=\"Great kettles\"><meta name=\"keywords\" content=\"kettle, tea\">",
        "<meta property=\"og:title\" content=\"Kettle shop\"><meta property=\"og:description\" content=\"Buy now\">",
        "<meta name=\"viewport\" content=\"width=device-width\"><meta charset=\"utf-8\"></head>",
        "<form><input type=\"text\" placeholder=\"Your name\" aria-label=\"Name field\"><input type=\"submit\" value=\"Send it\">",
        "<input type=\"hidden\" value=\"secret-token\"><input type=\"checkbox\" value=\"on\"></form>",
        "<div title='Tip text'><img src=a.png alt=Unquoted><img alt=\"\" src=b.png><img alt=\"  \" src=c.png></div>"
    );
    let d = round_trip(src);
    let got: Vec<(String, String)> =
        d.units.iter().map(|u| (u.context.kind.clone().unwrap(), plain_text(&u.source()))).collect();
    assert_eq!(
        got,
        vec![
            ("meta".to_string(), "Great kettles".to_string()),
            ("meta".to_string(), "kettle, tea".to_string()),
            ("meta".to_string(), "Kettle shop".to_string()),
            ("meta".to_string(), "Buy now".to_string()),
            ("label".to_string(), "Your name".to_string()),
            ("label".to_string(), "Name field".to_string()),
            ("button".to_string(), "Send it".to_string()),
            ("title".to_string(), "Tip text".to_string()),
            ("alt".to_string(), "Unquoted".to_string()),
        ]
    );
    assert_eq!(d.units[4].context.style.as_deref(), Some("input"));
    assert_eq!(d.units[4].context.key.as_deref(), Some("placeholder"));
    let mut d = d;
    d.set_target("u9", vec![Inline::text("Sin comillas")]).unwrap();
    d.set_target("u8", vec![Inline::text("Consejo")]).unwrap();
    let out = merged(&d);
    assert!(out.contains("<img src=a.png alt=\"Sin comillas\">"), "{out}");
    assert!(out.contains("<div title=\"Consejo\">"), "{out}");
    assert!(out.contains("<input type=\"hidden\" value=\"secret-token\">"));
}

#[test]
fn script_style_comments_untouched() {
    let src = concat!(
        "<!-- Do not translate this comment -->\n<style>.a > p { content: \"Hello\"; }</style>\n",
        "<script>var s = \"<p>Not text</p>\"; if (a < b) { x(); }</script>\n",
        "<p>Real text</p><!-- another --><script type=\"text/template\"><p>Hidden</p></script>\n",
        "<noscript><p>Enable scripts</p></noscript>\n"
    );
    let d = round_trip(src);
    assert_eq!(texts(&d), ["Real text", "Enable scripts"]);
    let mut d = d;
    d.set_target("u1", vec![Inline::text("Texto real")]).unwrap();
    let out = merged(&d);
    assert!(out.contains("var s = \"<p>Not text</p>\"; if (a < b) { x(); }"));
    assert!(out.contains("<!-- Do not translate this comment -->"));
    assert!(out.contains("<p>Texto real</p><!-- another -->"));
}

#[test]
fn pre_code_and_notranslate_are_protected() {
    let src = concat!(
        "<p>Run <code>npm install --save</code> now, then <kbd>Ctrl</kbd>+<kbd>C</kbd>.</p>\n",
        "<pre>  line one\n  line two <b>bold</b>\n</pre>\n",
        "<p>Brand: <span class=\"notranslate\">Larchmoor&reg;</span> kettles.</p>\n",
        "<div translate=\"no\"><p>Locked paragraph</p></div>\n",
        "<p translate=\"no\">Also locked</p>\n",
        "<p>Free text</p>\n"
    );
    let d = round_trip(src);
    assert_eq!(texts(&d), ["Run  now, then +.", "Brand:  kettles.", "Free text"]);
    let first = &d.units[0].segments[0].source;
    assert!(first.iter().any(|i| matches!(i, Inline::Tag { kind: TagKind::Standalone, display, .. } if display == "<code>")));
    let mut d = d;
    let mut t = d.units[0].segments[0].source.clone();
    if let Inline::Text(x) = &mut t[0] {
        *x = "Ejecute ".into();
    }
    d.set_target("u1", t).unwrap();
    let out = merged(&d);
    assert!(out.starts_with("<p>Ejecute <code>npm install --save</code>"), "{out}");
    assert!(out.contains("<pre>  line one\n  line two <b>bold</b>\n</pre>"));

    // With locked units requested, locked text appears but cannot be edited.
    let mut dl = Html.extract(src.as_bytes(), &ExtractOptions { include_locked: true, ..Default::default() }).unwrap();
    let locked: Vec<String> = dl.units.iter().filter(|u| !u.translate).map(|u| u.segments[0].id.clone()).collect();
    assert_eq!(locked.len(), 2);
    assert!(dl.set_target(&locked[0], vec![Inline::text("x")]).is_err());
    assert_eq!(merged(&dl), src);
}

#[test]
fn malformed_markup() {
    let src = "<div><p>First paragraph\n<p>Second <b>bold\n</div></div><li>Item one<li>Item two</ul><td>cell</p><p>Last";
    let d = round_trip(src);
    assert_eq!(texts(&d), ["First paragraph", "Second bold", "Item one", "Item two", "cell", "Last"]);
    // The unclosed <b> becomes a standalone tag, not a broken pair.
    let tags: Vec<&Inline> = d.units[1].segments[0].source.iter().filter(|i| matches!(i, Inline::Tag { .. })).collect();
    assert_eq!(tags.len(), 1);
    assert!(matches!(tags[0], Inline::Tag { kind: TagKind::Standalone, .. }));
    let mut d = d;
    d.set_target("u6", vec![Inline::text("Ultimo")]).unwrap();
    assert!(merged(&d).ends_with("<p>Ultimo"));
}

#[test]
fn stray_and_crossed_tags_keep_ids_unique() {
    let src = "<p>a <b>b <i>c</b> d</i> e</i></p>";
    let mut d = round_trip(src);
    let seg = d.units[0].segments[0].source.clone();
    // Translating with the same inlines must be accepted by the model (ids unique, no invented tags).
    d.set_target("u1", seg).unwrap();
    assert_eq!(merged(&d), "<p>a <b>b <i>c</b> d</i> e</i></p>");
}

#[test]
fn tables_lists_and_kinds() {
    let src = concat!(
        "<table><caption>Sizes</caption><tr><th>Size</th><th>Price</th></tr><tr><td>Small</td><td>$5</td></tr></table>\n",
        "<ul><li>First</li><li><a href=\"/s\">Second link</a></li></ul>\n",
        "<dl><dt>Term</dt><dd>Definition</dd></dl><blockquote>Quote text</blockquote>\n",
        "<figure><img src=x.png alt=\"Chart\"><figcaption>Figure one</figcaption></figure>\n",
        "<select><option>Red</option><option value=b>Blue</option></select><label for=n>Name</label>\n",
        "<div><a href=\"/x\">Only a link</a></div><details><summary>More</summary>Body</details>\n"
    );
    let d = round_trip(src);
    let got: Vec<(String, String)> = d.units.iter().map(|u| (u.context.kind.clone().unwrap(), plain_text(&u.source()))).collect();
    let want = [
        ("caption", "Sizes"),
        ("cell", "Size"),
        ("cell", "Price"),
        ("cell", "Small"),
        ("cell", "$5"),
        ("list-item", "First"),
        ("list-item", "Second link"),
        ("list-item", "Term"),
        ("list-item", "Definition"),
        ("paragraph", "Quote text"),
        ("alt", "Chart"),
        ("caption", "Figure one"),
        ("option", "Red"),
        ("option", "Blue"),
        ("label", "Name"),
        ("link", "Only a link"),
        ("summary", "More"),
        ("paragraph", "Body"),
    ];
    let want: Vec<(String, String)> = want.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    assert_eq!(got, want);
    let td = d.units.iter().find(|u| plain_text(&u.source()) == "Small").unwrap();
    assert_eq!(td.context.style.as_deref(), Some("td"));
    assert!(td.context.location.as_deref().unwrap().ends_with("td:nth-of-type(1)"), "{:?}", td.context.location);
}

#[test]
fn heading_context() {
    let src = "<h2>Shipping</h2><p>Takes two days.</p><h2>Returns</h2><p>Thirty days.</p>";
    let d = round_trip(src);
    assert_eq!(d.units[1].context.heading.as_deref(), Some("Shipping"));
    assert_eq!(d.units[3].context.heading.as_deref(), Some("Returns"));
}

#[test]
fn bom_and_latin1() {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice("<p>Caf\u{e9}</p>".as_bytes());
    let d = Html.extract(&bytes, &Default::default()).unwrap();
    assert_eq!(Html.merge(&d).unwrap(), bytes);

    let latin: Vec<u8> = b"<html><head><meta charset=\"iso-8859-1\"></head><body><p>Caf\xe9 cr\xe8me</p></body></html>".to_vec();
    let mut d = Html.extract(&latin, &Default::default()).unwrap();
    assert_eq!(texts(&d), ["Caf\u{e9} cr\u{e8}me"]);
    assert_eq!(Html.merge(&d).unwrap(), latin);
    d.set_target("u1", vec![Inline::text("\u{4e2d}\u{6587} na\u{ef}ve")]).unwrap();
    let out = Html.merge(&d).unwrap();
    assert!(String::from_utf8_lossy(&out).contains("&#20013;&#25991; na"));
    assert!(out.contains(&0xEF));

    assert!(Html.extract(b"<meta charset=\"shift_jis\"><p>x</p>", &Default::default()).is_err());
    assert!(Html.extract(b"<p>\xff\xfe</p>", &Default::default()).is_err());
}

#[test]
fn xhtml_style_and_odd_input() {
    let src = "<?xml version=\"1.0\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xml:lang=\"en\" lang=\"en\"><body><p>Line<br/>break <img src=\"a.png\" alt=\"Pic\"/></p><p/></body></html>";
    let d = round_trip(src);
    assert_eq!(texts(&d)[0], "Linebreak ");
    for s in ["", "   ", "plain text only", "<", "<p", "<p>unterminated <b", "<!-- never closed", "<p a=\"x>text</p>", "&&&;;;"] {
        let d = round_trip(s);
        let _ = d;
    }
}

#[test]
fn sniff_and_ids() {
    assert!(Html.sniff(b"  <!DOCTYPE html><title>x</title>"));
    assert!(Html.sniff(b"<HTML><BODY>"));
    assert!(!Html.sniff(b"{\"a\":1}"));
    assert_eq!(Html.extensions(), ["html", "htm", "xhtml"]);
    let d = extract("<p>a</p><p>b</p><p>c</p>");
    let ids: Vec<&str> = d.units.iter().map(|u| u.id.as_str()).collect();
    assert_eq!(ids, ["u1", "u2", "u3"]);
    assert_eq!(extract("<p>a</p><p>b</p><p>c</p>"), d, "extraction is deterministic");
}

#[test]
fn partial_translation_keeps_the_rest_untouched() {
    let src = "<p>First &amp; second</p>\n<p>Third</p>\n";
    let mut d = extract(src);
    d.set_target("u2", vec![Inline::text("Tercero")]).unwrap();
    assert_eq!(merged(&d), "<p>First &amp; second</p>\n<p>Tercero</p>\n");
}

// ---------- preview ----------

#[test]
fn preview_has_a_span_for_every_segment() {
    let src = page(&PAGES[0]);
    let d = extract_to(&src, "es");
    let pv = Html.preview(&d).unwrap();
    for (_, seg) in d.segments() {
        let in_span = format!("data-bl-seg=\"{}\"", seg.id);
        let in_attr = format!("data-bl-attr-seg=\"{}\"", seg.id);
        assert!(pv.contains(&in_span) || pv.contains(&in_attr), "segment {} missing in {pv}", seg.id);
    }
    assert!(pv.contains("<h1 class=\"hero\"><span data-bl-seg=\"u3\">Welcome to Larchmoor</span></h1>"), "{pv}");
    assert!(pv.contains("<span data-bl-seg=\"u4\">Our <b>kettles</b> boil fast &amp; quietly.</span>"), "{pv}");
    assert!(pv.contains("[data-bl-seg]{outline:none}"));
    // Attribute units cannot hold spans.
    assert!(pv.contains("<img data-bl-attr-seg=\"u5\" data-bl-attr=\"alt\" src=\"kettle.png\" alt=\"A blue kettle\">"), "{pv}");
    // Title is plain text.
    assert!(pv.contains("<title data-bl-attr-seg=\"u1\" data-bl-attr=\"#text\">Larchmoor Tools</title>"), "{pv}");
}

#[test]
fn preview_strips_scripts_and_handlers() {
    let src = concat!(
        "<html><head><base href=\"http://evil.example/\"><script src=\"x.js\"></script><meta http-equiv=\"refresh\" content=\"0;url=http://x\"></head>",
        "<body onload=\"steal()\"><p onclick=\"go()\">Click <a href=\"javascript:alert(1)\" onmouseover=\"x()\">here</a></p>",
        "<script>alert('x')</script><img src=a.png onerror=\"bad()\" alt=\"Pic\"><a href=\"&#106;avascript:evil()\">enc</a></body></html>"
    );
    let d = extract(src);
    let pv = Html.preview(&d).unwrap();
    let low = pv.to_ascii_lowercase();
    assert!(!low.contains("<script"), "{pv}");
    assert!(!low.contains("alert("), "{pv}");
    assert!(!low.contains("onload") && !low.contains("onclick") && !low.contains("onerror") && !low.contains("onmouseover"), "{pv}");
    assert!(!low.contains("javascript:") && !low.contains("&#106;avascript"), "{pv}");
    assert!(!low.contains("<base"), "{pv}");
    assert!(!low.contains("refresh"), "{pv}");
    assert!(pv.contains("data-bl-seg"));
    // The style hook sits in the head.
    assert!(pv.find("<style>").unwrap() < pv.find("</head>").unwrap());
}

#[test]
fn preview_style_without_head_and_untrusted_text_is_escaped() {
    let d = extract("<p>Hello</p>");
    let mut d = d;
    d.set_target("u1", vec![Inline::text("<script>alert(1)</script> & \"x\"")]).unwrap();
    let pv = Html.preview(&d).unwrap();
    assert!(pv.starts_with("<style>"));
    assert!(pv.contains("&lt;script&gt;alert(1)&lt;/script&gt; &amp; \"x\""), "{pv}");
}

#[test]
fn preview_shows_targets_and_patching_one_span_works() {
    let src = "<h1>Title</h1><p>Hello <b>world</b> and <a href=\"/a\" title=\"Tip\">friends</a>.</p>";
    let mut d = extract(src);
    // u1 heading, u2 paragraph, u3 title attr
    assert_eq!(d.units.len(), 3);
    let pv0 = Html.preview(&d).unwrap();
    assert!(pv0.contains("<span data-bl-seg=\"u2\">Hello <b>world</b> and <a data-bl-attr-seg=\"u3\" data-bl-attr=\"title\" href=\"/a\" title=\"Tip\">friends</a>.</span>"), "{pv0}");

    let mut t = d.units[1].segments[0].source.clone();
    for i in t.iter_mut() {
        if let Inline::Text(x) = i {
            *x = match x.as_str() {
                "Hello " => "Hola ".into(),
                " and " => " y ".into(),
                "world" => "mundo".into(),
                "friends" => "amigos".into(),
                other => other.into(),
            };
        }
    }
    d.set_target("u2", t).unwrap();
    d.set_target("u3", vec![Inline::text("Consejo")]).unwrap();

    let inner = Html.render_segment(&d, "u2").unwrap();
    assert_eq!(inner, "Hola <b>mundo</b> y <a data-bl-attr-seg=\"u3\" data-bl-attr=\"title\" href=\"/a\" title=\"Consejo\">amigos</a>.");
    // Patch: replace the old span content in the old preview with the new inner HTML.
    let open = "<span data-bl-seg=\"u2\">";
    let a = pv0.find(open).unwrap() + open.len();
    let b = a + pv0[a..].find("</span>").unwrap();
    let patched = format!("{}{}{}", &pv0[..a], inner, &pv0[b..]);
    assert_eq!(patched, Html.preview(&d).unwrap());

    // Attribute segments render as the attribute text.
    assert_eq!(Html.render_segment(&d, "u3").unwrap(), "Consejo");
    assert_eq!(Html.render_segment(&d, "nope"), None);
}

#[test]
fn render_segment_is_safe_and_escaped() {
    let mut d = extract("<p>Say <i onclick=\"x()\">hi</i></p>");
    let src = d.units[0].segments[0].source.clone();
    let mut t = src.clone();
    if let Inline::Text(x) = &mut t[0] {
        *x = "A < B & C ".into();
    }
    d.set_target("u1", t).unwrap();
    let inner = Html.render_segment(&d, "u1").unwrap();
    assert_eq!(inner, "A &lt; B &amp; C <i >hi</i>");
    assert!(!sanitize_html("<p onclick=x>a</p><script>1</script>", false).contains("onclick"));
}

#[test]
fn preview_marks_two_attributes_on_one_element() {
    let d = extract("<div title=\"Tip one\" aria-label=\"Label two\"></div>");
    let pv = Html.preview(&d).unwrap();
    assert!(pv.contains("data-bl-attr-seg=\"u1 u2\" data-bl-attr=\"title aria-label\""), "{pv}");
}

#[test]
fn preview_with_segmented_units() {
    // Simulate sentence segmentation done by bilingua-seg: two segments, one separator.
    let mut d = extract("<p>One. Two.</p>");
    let u = &mut d.units[0];
    u.segments = vec![
        Segment { id: "u1#1".into(), source: vec![Inline::text("One.")], target: vec![], state: SegState::New },
        Segment { id: "u1#2".into(), source: vec![Inline::text("Two.")], target: vec![Inline::text("Dos.")], state: SegState::Draft },
    ];
    u.separators = vec![" ".into()];
    let pv = Html.preview(&d).unwrap();
    assert!(pv.contains("<p><span data-bl-seg=\"u1#1\">One.</span> <span data-bl-seg=\"u1#2\">Dos.</span></p>"), "{pv}");
    assert_eq!(Html.render_segment(&d, "u1#2").unwrap(), "Dos.");
    assert_eq!(merged(&d), "<p>One. Dos.</p>");
}

#[test]
fn every_prefix_of_a_page_round_trips_without_panicking() {
    let src = format!("{}<p onclick=\"a\">x <b>y</b> &amp; <img alt='q' src=z></p><script>if(a<b){{}}</script><!-- c -->", page(&PAGES[10]));
    for end in 0..=src.len() {
        if !src.is_char_boundary(end) {
            continue;
        }
        let part = &src[..end];
        // A cut inside the charset name is an unknown charset: an error, not a panic.
        let Ok(d) = Html.extract(part.as_bytes(), &ExtractOptions::default()) else { continue };
        assert_eq!(merged(&d), part, "prefix of {end} bytes");
        let _ = Html.preview(&d);
    }
}

#[test]
fn preview_of_every_language_page_has_all_segments() {
    for p in PAGES {
        let d = extract(&page(p));
        let pv = Html.preview(&d).unwrap();
        for (_, s) in d.segments() {
            assert!(pv.contains(&format!("\"{}\"", s.id)), "{}", p.0);
        }
        assert!(!pv.contains("<script"));
    }
}

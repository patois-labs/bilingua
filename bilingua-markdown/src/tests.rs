use super::*;

fn extract(s: &str) -> Document {
    MarkdownFilter.extract(s.as_bytes(), &ExtractOptions::default()).unwrap()
}

fn merged(d: &Document) -> String {
    String::from_utf8(MarkdownFilter.merge(d).unwrap()).unwrap()
}

fn texts(d: &Document) -> Vec<String> {
    d.units.iter().map(|u| plain_text(&u.source())).collect()
}

fn kinds(d: &Document) -> Vec<String> {
    d.units.iter().map(|u| u.context.kind.clone().unwrap_or_default()).collect()
}

const KITCHEN_SINK: &str = "---\ntitle: Harbor lamp guide\ndescription: \"A warm light for desks\"\nauthor: Nobody Real\ntags: [a, b]\n---\n\n# Welcome to *Larchmoor* ##\n\nA \\* b &amp; c **bold [link](http://x.test \"Link title\") end** `code`\nnext line  \nhard\n\n> quote one\n> quote two\n\n- item one\n- [ ] item two\n  - nested item\n\n1. first\n2. second\n\n| Name | Note |\n|------|------|\n| a *b* | c \\| d |\n\n![Alt text *x*](img/i.png \"Image title\") <https://a.test> <b>raw</b> ~~gone~~ text[^1]\n\n[^1]: The footnote text.\n\n```rust\nfn main() { println!(\"not translated\"); }\n```\n\n    indented code\n\n<div>\nhtml block\n</div>\n\nSetext heading\n==============\n\n[ref]: http://r.test \"Ref title\"\n\n***\n\nLast line without newline";

#[test]
fn kitchen_sink_round_trips_byte_for_byte() {
    let d = extract(KITCHEN_SINK);
    assert_eq!(merged(&d), KITCHEN_SINK);
    // every kind we promise shows up
    let k = kinds(&d);
    for want in ["title", "description", "heading", "paragraph", "list-item", "cell", "alt"] {
        assert!(k.iter().any(|x| x == want), "missing {want} in {k:?}");
    }
}

#[test]
fn code_and_html_blocks_and_references_are_not_translated() {
    let d = extract(KITCHEN_SINK);
    let all = texts(&d).join("\n");
    assert!(!all.contains("not translated"));
    assert!(!all.contains("indented code"));
    assert!(!all.contains("html block"));
    assert!(!all.contains("Ref title"));
    assert!(!all.contains("author") && !all.contains("Nobody"));
    assert!(all.contains("Harbor lamp guide"));
    assert!(all.contains("A warm light for desks"));
}

#[test]
fn headings_have_level_and_context() {
    let d = extract("# One\n\n## Two *x*\n\nBody under two.\n\nSetext\n---\n");
    assert_eq!(kinds(&d), vec!["heading", "heading", "paragraph", "heading"]);
    assert_eq!(d.units[0].context.style.as_deref(), Some("h1"));
    assert_eq!(d.units[1].context.style.as_deref(), Some("h2"));
    assert_eq!(d.units[3].context.style.as_deref(), Some("h2"));
    assert_eq!(d.units[2].context.heading.as_deref(), Some("Two x"));
    assert_eq!(d.units[0].context.location.as_deref(), Some("line 1"));
    assert_eq!(d.units[2].context.location.as_deref(), Some("line 5"));
}

#[test]
fn inline_markup_becomes_tags_without_urls_in_the_text() {
    let d = extract("Click **Save** or [go back](https://example.test/a?b=1) now, `x = 1`, *really* and ~~not~~.\n");
    let u = &d.units[0];
    assert_eq!(plain_text(&u.source()), "Click Save or go back now, , really and not.");
    let displays: Vec<String> = u.source().iter().filter_map(|i| if let Inline::Tag { display, .. } = i { Some(display.clone()) } else { None }).collect();
    assert_eq!(displays, vec!["<b>", "</b>", "<a>", "</a>", "`x = 1`", "<i>", "</i>", "<s>", "</s>"]);
    // the URL lives in the code data
    let close = u.codes.values().find_map(|c| c.close.clone().filter(|x| x.contains("example.test"))).unwrap();
    assert_eq!(close, "](https://example.test/a?b=1)");
    assert!(!plain_text(&u.source()).contains("example.test"));
}

#[test]
fn translating_text_keeps_the_markup_and_url() {
    let mut d = extract("Click **Save** or [go back](https://example.test/a \"Back title\") now.\n");
    let src = d.units[0].source();
    let t = |i: usize| src[i].clone();
    d.set_target("u1", vec![Inline::text("Pulse "), t(1), Inline::text("Guardar"), t(3), Inline::text(" o "), t(5), Inline::text("volver"), t(7), Inline::text(" ahora.")]).unwrap();
    assert_eq!(merged(&d), "Pulse **Guardar** o [volver](https://example.test/a \"Back title\") ahora.\n");
    // the title is its own unit
    assert_eq!(kinds(&d), vec!["paragraph", "title"]);
    assert_eq!(plain_text(&d.units[1].source()), "Back title");
    d.set_target("u2", vec![Inline::text("Volver atr\u{e1}s")]).unwrap();
    assert_eq!(merged(&d), "Pulse **Guardar** o [volver](https://example.test/a \"Volver atr\u{e1}s\") ahora.\n");
    // a quote in the new title is escaped
    d.set_target("u2", vec![Inline::text("Di \"hola\"")]).unwrap();
    assert!(merged(&d).contains("\"Di \\\"hola\\\"\""));
}

#[test]
fn alt_text_is_a_unit_and_translates_in_place() {
    let mut d = extract("See ![A harbor lamp](lamp.png) here.\n\n![Only image](only.png)\n");
    assert_eq!(kinds(&d), vec!["paragraph", "alt", "image", "alt"]);
    assert!(!d.units[2].translate, "an image-only paragraph is a locked carrier");
    assert_eq!(texts(&d)[1], "A harbor lamp");
    d.set_target("u2", vec![Inline::text("Una l\u{e1}mpara")]).unwrap();
    d.set_target("u4", vec![Inline::text("Solo imagen")]).unwrap();
    assert_eq!(merged(&d), "See ![Una l\u{e1}mpara](lamp.png) here.\n\n![Solo imagen](only.png)\n");
    assert!(d.set_target("u3", vec![Inline::text("x")]).is_err());
}

#[test]
fn paragraphs_without_text_are_left_alone() {
    let src = "`only code`\n\n12345\n\n<span></span>\n\n---\n\n![](empty-alt.png)\n";
    let d = extract(src);
    assert_eq!(d.units.len(), 0, "{:?}", texts(&d));
    assert_eq!(merged(&d), src);
}

#[test]
fn soft_breaks_stay_in_the_text_and_prefixes_become_tags() {
    let d = extract("line one\nline two\n\n> quoted one\n> quoted two\n\nhard  \nbreak\n");
    assert_eq!(texts(&d)[0], "line one\nline two");
    assert_eq!(texts(&d)[1], "quoted onequoted two");
    let tags = d.units[1].source().iter().filter(|i| matches!(i, Inline::Tag { .. })).count();
    assert_eq!(tags, 1);
    let code = d.units[1].codes.values().next().unwrap().standalone.clone().unwrap();
    assert_eq!(code, "\n> ");
    assert_eq!(merged(&d), "line one\nline two\n\n> quoted one\n> quoted two\n\nhard  \nbreak\n");
}

#[test]
fn tables_lists_and_nested_lists() {
    let src = "| H1 | H2 |\n|----|----|\n| one | two *x* |\n\n* top\n  * inner one\n  * inner two\n* second\n\n1. a first\n   continued here\n2. b second\n";
    let d = extract(src);
    assert_eq!(kinds(&d), vec!["cell", "cell", "cell", "cell", "list-item", "list-item", "list-item", "list-item", "list-item", "list-item"]);
    assert_eq!(texts(&d)[3], "two x");
    assert_eq!(merged(&d), src);
}

#[test]
fn translation_of_a_table_cell_and_a_list_item() {
    let mut d = extract("| Name | Note |\n|---|---|\n| Lamp | Warm |\n\n- Buy a lamp\n");
    d.set_target("u3", vec![Inline::text("L\u{e1}mpara")]).unwrap();
    d.set_target("u5", vec![Inline::text("Comprar una l\u{e1}mpara")]).unwrap();
    assert_eq!(merged(&d), "| Name | Note |\n|---|---|\n| L\u{e1}mpara | Warm |\n\n- Comprar una l\u{e1}mpara\n");
}

#[test]
fn front_matter() {
    let src = "---\ntitle: Plain title: with colon? no, ok\ndescription: 'It''s single'\nother: keep\n---\nBody text here.\n";
    let mut d = extract(src);
    assert_eq!(kinds(&d), vec!["title", "description", "paragraph"]);
    assert_eq!(texts(&d)[1], "It's single");
    assert_eq!(d.units[0].context.key.as_deref(), Some("title"));
    assert_eq!(merged(&d), src);
    d.set_target("u1", vec![Inline::text("T\u{ed}tulo: con dos puntos")]).unwrap();
    d.set_target("u2", vec![Inline::text("Es \u{201c}simple\u{201d} y it's")]).unwrap();
    d.set_target("u3", vec![Inline::text("Texto del cuerpo.")]).unwrap();
    assert_eq!(
        merged(&d),
        "---\ntitle: \"T\u{ed}tulo: con dos puntos\"\ndescription: 'Es \u{201c}simple\u{201d} y it''s'\nother: keep\n---\nTexto del cuerpo.\n"
    );
    // a double quoted value and an unterminated block
    let d = extract("---\ntitle: \"Say \\\"hi\\\"\"\n---\nx y\n");
    assert_eq!(texts(&d)[0], "Say \"hi\"");
    let d = extract("---\ntitle: never closed\n\nBody text.\n");
    assert_eq!(kinds(&d), vec!["paragraph", "paragraph"]);
}

#[test]
fn unsupported_front_matter_values_stay_in_the_skeleton() {
    let src = "---\ntitle: |\n  block text\ndescription: [a, b]\n---\nHello world.\n";
    let d = extract(src);
    assert_eq!(kinds(&d), vec!["paragraph"]);
    assert_eq!(merged(&d), src);
    let src = "---\ntitle: \"bad \\n escape\"\n---\nHello world.\n";
    assert_eq!(kinds(&extract(src)), vec!["paragraph"]);
}

#[test]
fn bom_and_crlf() {
    let src = "\u{feff}# Title here\r\n\r\nFirst line\r\nsecond line\r\n\r\n- item one\r\n- item two\r\n";
    let d = extract(src);
    assert_eq!(MarkdownFilter.merge(&d).unwrap(), src.as_bytes());
    assert_eq!(d.units.len(), 4);
    assert!(texts(&d)[1].contains("\r\n"));
}

#[test]
fn round_trip_in_every_primary_language() {
    let docs = [
        "# Welcome\n\nThe tide is out. **Come** in.\n",
        "# Bienvenido\n\n\u{bf}C\u{f3}mo est\u{e1}s? \u{a1}Muy bien!\n",
        "# Bienvenue\n\nVoir [la page](https://x.test/fr) \u{e0} bient\u{f4}t.\n",
        "# Benvenuti\n\nLa marea \u{e8} *bassa* oggi.\n",
        "# Willkommen\n\nDie Flut ist \u{fc}ber. Siehe `Abb. 3`.\n",
        "# Welkom\n\nHet is eb. ![Lamp](l.png \"Lamp\")\n",
        "# Bem-vindo\n\nA mar\u{e9} est\u{e1} **baixa**.\n",
        "# \u{3088}\u{3046}\u{3053}\u{305d}\n\n\u{6f6e}\u{304c}\u{5f15}\u{3044}\u{3066}\u{3044}\u{307e}\u{3059}\u{3002}**\u{592a}\u{5b57}**\u{3067}\u{3059}\u{3002}\n",
        "# \u{6b22}\u{8fce}\n\n\u{6f6e}\u{6c34}\u{9000}\u{4e86}\u{3002}[\u{94fe}\u{63a5}](https://x.test/zh)\u{3002}\n",
        "# \u{d658}\u{c601}\n\n\u{ac8c}\u{c774}\u{bc8c}\u{c774} \u{c379}\u{c2b5}\u{b2c8}\u{b2e4}. ~~\u{c0ad}\u{c81c}~~\n",
        "# \u{645}\u{631}\u{62d}\u{628}\u{627}\n\n\u{627}\u{644}\u{645}\u{62f}\u{62c}\u{632}\u{631} **\u{645}\u{646}\u{62e}\u{641}\u{636}**.\n\n| \u{627}\u{633}\u{645} | \u{645}\u{644}\u{627}\u{62d}\u{638}\u{629} |\n|---|---|\n| \u{645}\u{635}\u{628}\u{627}\u{62d} | \u{62f}\u{627}\u{641}\u{626} |\n",
    ];
    for s in docs {
        let d = extract(s);
        assert!(!d.units.is_empty(), "{s:?}");
        assert_eq!(merged(&d), s, "{s:?}");
    }
}

#[test]
fn translation_in_arabic_and_japanese_keeps_markup() {
    let mut d = extract("The lamp is **warm** today.\n");
    let src = d.units[0].source();
    d.set_target("u1", vec![Inline::text("\u{627}\u{644}\u{645}\u{635}\u{628}\u{627}\u{62d} "), src[1].clone(), Inline::text("\u{62f}\u{627}\u{641}\u{626}"), src[3].clone(), Inline::text(" \u{627}\u{644}\u{64a}\u{648}\u{645}.")]).unwrap();
    assert_eq!(merged(&d), "\u{627}\u{644}\u{645}\u{635}\u{628}\u{627}\u{62d} **\u{62f}\u{627}\u{641}\u{626}** \u{627}\u{644}\u{64a}\u{648}\u{645}.\n");
    let mut d = extract("The lamp is **warm** today.\n");
    let src = d.units[0].source();
    d.set_target("u1", vec![Inline::text("\u{4eca}\u{65e5}\u{306e}\u{30e9}\u{30f3}\u{30d7}\u{306f}"), src[1].clone(), Inline::text("\u{6e29}\u{304b}\u{3044}"), src[3].clone(), Inline::text("\u{3067}\u{3059}\u{3002}")]).unwrap();
    assert_eq!(merged(&d), "\u{4eca}\u{65e5}\u{306e}\u{30e9}\u{30f3}\u{30d7}\u{306f}**\u{6e29}\u{304b}\u{3044}**\u{3067}\u{3059}\u{3002}\n");
}

#[test]
fn escapes_and_entities_stay_as_written() {
    let src = "Use \\*stars\\* &amp; &copy; 2026 \\[not a link\\] and a_b_c.\n";
    let d = extract(src);
    assert_eq!(texts(&d)[0], "Use \\*stars\\* &amp; &copy; 2026 \\[not a link\\] and a_b_c.");
    assert_eq!(merged(&d), src);
}

#[test]
fn reference_links_and_footnotes() {
    let src = "See [the guide][g] and [another] one[^n].\n\n[g]: http://g.test \"Guide\"\n[another]: http://a.test\n[^n]: A note here.\n";
    let d = extract(src);
    assert_eq!(merged(&d), src);
    assert!(texts(&d).iter().any(|t| t.contains("A note here")));
}

#[test]
fn link_variants_round_trip() {
    for src in [
        "[a](<url with spaces> \"T\") x\n",
        "[a](url 'single title') x\n",
        "[a](url (paren title)) x\n",
        "[a](url) and [b](u2 \"B\") and ![c](i.png 'C') y\n",
        "<https://auto.test> and <me@example.test> and https://bare.test word\n",
        "[**bold link**](u) and **[link bold](u)** and *a **b** c*\n",
        "[a]\n\n[a]: http://x\n",
        "[](empty) text\n",
        "[ ](space) text\n",
    ] {
        let d = extract(src);
        assert_eq!(merged(&d), src, "{src:?}");
    }
}

#[test]
fn preview_wraps_every_segment_and_blocks_scripts() {
    let mut d = extract("# Title\n\nFirst sentence. **Second** one.\n\n<script>alert(1)</script>\n\nClick [here](javascript:alert(1)) and <img src=x onerror=alert(2)> and ![x](data:text/html;base64,AAAA).\n");
    // split the paragraph into two segments by hand
    let u = &mut d.units[1];
    let src = u.source();
    u.segments = vec![
        Segment::new("u2#1", vec![src[0].clone()].into_iter().map(|_| Inline::text("First sentence.")).collect()),
        Segment::new("u2#2", src[1..].to_vec()),
    ];
    u.separators = vec![" ".into()];
    d.set_target("u1", vec![Inline::text("T\u{ed}tulo")]).unwrap();
    d.set_target("u2#1", vec![Inline::text("Primera frase.")]).unwrap();
    let html = MarkdownFilter.preview(&d).unwrap();
    assert!(html.contains("<span data-bl-seg=\"u1\">T\u{ed}tulo</span>"), "{html}");
    assert!(html.contains("<span data-bl-seg=\"u2#1\">Primera frase.</span> <span data-bl-seg=\"u2#2\"><strong>Second</strong> one.</span>"), "{html}");
    assert!(!html.to_lowercase().contains("<script"));
    assert!(!html.contains("<img src=x"));
    assert!(!html.contains("javascript:"));
    assert!(!html.contains("data:text/html"));
    assert!(html.contains("&lt;script&gt;"));
    assert!(html.contains("<h1>"));
}

#[test]
fn preview_has_no_front_matter_and_keeps_alt_text() {
    let mut d = extract("---\ntitle: My title\n---\n\nHello ![A lamp](l.png) world.\n");
    d.set_target("u3", vec![Inline::text("Una l\u{e1}mpara")]).unwrap();
    let html = MarkdownFilter.preview(&d).unwrap();
    assert!(!html.contains("My title"));
    assert!(html.contains("alt=\"Una l\u{e1}mpara\""), "{html}");
    assert!(html.contains("<span data-bl-seg=\"u2\">Hello"));
}

#[test]
fn render_segment_gives_inner_html() {
    let mut d = extract("Click **Save** now.\n");
    assert_eq!(MarkdownFilter.render_segment(&d, "u1").unwrap(), "Click <strong>Save</strong> now.");
    let src = d.units[0].source();
    d.set_target("u1", vec![Inline::text("Pulse <"), src[1].clone(), Inline::text("Guardar"), src[3].clone()]).unwrap();
    assert_eq!(MarkdownFilter.render_segment(&d, "u1").unwrap(), "Pulse &lt;<strong>Guardar</strong>");
    assert!(MarkdownFilter.render_segment(&d, "nope").is_none());
}

#[test]
fn bad_input() {
    assert!(matches!(MarkdownFilter.extract(&[0x23, 0x20, 0xFF, 0], &Default::default()), Err(Error::Encoding(_))));
    assert!(MarkdownFilter.extract(&[0xFF, 0xFE, 0x23, 0], &Default::default()).is_ok());
    let d = extract("");
    assert_eq!(d.units.len(), 0);
    assert_eq!(merged(&d), "");
    let mut d = extract("x y\n");
    d.skeleton = Skeleton::Zip { entries: vec![], parser: None };
    assert!(matches!(MarkdownFilter.merge(&d), Err(Error::Write(_))));
}

#[test]
fn awkward_documents_always_round_trip() {
    let docs = [
        "",
        "\n\n\n",
        "# \n",
        "#\n",
        "text\n\n\n\n   \n\ntext two\n",
        "- \n- a b\n-\n",
        "> > nested quote\n> > more\n",
        "> - list in quote\n>   continued\n",
        "* * *\n\n- - -\n\n___\n",
        "| a |\n|---|\n",
        "|a|b|\n|-|-|\n|1|\n",
        "Title\n=====\n\nSub\n---\n",
        "```\nunclosed code\n",
        "~~~md\n# not a heading\n~~~\n",
        "<!-- comment -->\n\ntext after comment\n",
        "text <!-- inline comment --> more text\n",
        "<details>\n<summary>Sum</summary>\n\nInside text.\n\n</details>\n",
        "1) one\n2) two\n",
        "a\\\nb\n",
        "Term\n: definition\n",
        "$$ math $$\n\nword\n",
        "\u{200f}\u{645}\u{631}\u{62d}\u{628}\u{627} **\u{628}\u{643}\u{645}**\n",
        "emoji \u{1f600} and \u{4e2d}\u{6587}\n",
        "- [x] done task\n- [ ] open task\n",
        "[link](url \"title with \\\"quotes\\\"\") z\n",
        "![img **bold alt**](x.png)\n\ntext after\n",
        "Hello  \nworld  \n",
        "    \n    code\n\ntext\n",
        "***bold italic*** and _under_ and __strong__\n",
        "a * b * c and 2 < 3 > 1 and a_b and snake_case_word\n",
        "footnote[^a] text\n\n[^a]: Note with **bold**.\n    More note text.\n",
    ];
    for s in docs {
        let d = extract(s);
        assert_eq!(merged(&d), s, "{s:?}");
    }
}

#[test]
fn works_with_segmentation_separators() {
    let mut d = extract("One. Two.\n");
    d.units[0].segments = vec![
        Segment::new("u1#1", vec![Inline::text("One.")]).with_target(vec![Inline::text("Uno.")]).with_state(SegState::Draft),
        Segment::new("u1#2", vec![Inline::text("Two.")]),
    ];
    d.units[0].separators = vec![" ".into()];
    assert_eq!(merged(&d), "Uno. Two.\n");
}

#[test]
fn fuzz_fragments_always_round_trip() {
    let frags = [
        "# ", "## ", "- ", "1. ", "> ", "| ", " | ", "*", "**", "_", "~~", "`", "[", "](", ")", "![", "<", ">", "\\", "&amp;", "\n", "\n\n",
        "  \n", "word", "Two words", "\u{4e2d}\u{6587}", "\u{645}\u{631}\u{62d}\u{628}\u{627}", "```", "---", "http://x.test", " \"t\" ", "[^1]", "[^1]: ", "    ", "\t",
        "<b>", "</b>", "<!--", "-->", ":", "$", "\r\n",
    ];
    let mut seed: u64 = 0x9E3779B97F4A7C15;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for _ in 0..3000 {
        let n = 1 + (next() % 24) as usize;
        let s: String = (0..n).map(|_| frags[(next() % frags.len() as u64) as usize]).collect();
        let d = MarkdownFilter.extract(s.as_bytes(), &ExtractOptions::default()).unwrap_or_else(|e| panic!("{s:?}: {e}"));
        assert_eq!(merged(&d), s, "{s:?}");
        // the preview never panics and never lets a script through
        if let Some(h) = MarkdownFilter.preview(&d) {
            assert!(!h.to_lowercase().contains("<script"));
        }
    }
}

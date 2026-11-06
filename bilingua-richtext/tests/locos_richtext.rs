//! The Localization OS walker cases (`tests/verify_contentful_richtext.py`, `verify_sanity.py` section 9,
//! `verify_jira_p1.py`, `verify_jira_1010.py` #644, `verify_notion.py` walker cases and finding 3), as Bilingua units.
//! The cases that need the connectors (the fake CMA, hashes, parking) belong to the connectors, not to the walkers.
mod common;
use bilingua_model::*;
use bilingua_richtext::*;
use common::*;
use serde_json::json;

fn bytes(v: &serde_json::Value) -> Vec<u8> {
    serde_json::to_vec_pretty(v).unwrap()
}

fn t(v: &str, marks: &[&str]) -> serde_json::Value {
    json!({"nodeType": "text", "value": v, "marks": marks.iter().map(|m| json!({"type": m})).collect::<Vec<_>>(), "data": {}})
}

fn blk(kind: &str, content: Vec<serde_json::Value>) -> serde_json::Value {
    json!({"nodeType": kind, "data": {}, "content": content})
}

fn doc(content: Vec<serde_json::Value>) -> Vec<u8> {
    bytes(&blk("document", content))
}

// ---------------------------------------------------------------- Contentful

#[test]
fn contentful_walker_units_and_identity_rebuild() {
    let f = Contentful;
    let mut d = open(&f, CONTENTFUL);
    let tx = texts(&d);
    // one unit per text block: heading, paragraph, 3 list paragraphs, quote, code paragraph, 4 table cells
    assert_eq!(tx.len(), 11, "{tx:?}");
    assert!(tx[1].starts_with("Read the ⟦g1⟧quick guide⟦/g1⟧ and visit "), "{}", tx[1]);
    assert!(tx[1].contains("⟦g2⟧our ⟦g3⟧pricing page⟦/g3⟧⟦/g2⟧"), "{}", tx[1]);
    assert!(tx[1].contains("⟦x4⟧") && tx[1].contains("⟦g5⟧related post⟦/g5⟧"), "{}", tx[1]);
    assert!(tx.iter().any(|s| s.starts_with("Run ") && s.contains("⟦x1⟧") && !s.contains("install")));
    assert!(!tx.join(" ").contains("npm install"), "code spans are never sent to MT");
    assert_eq!(d.units[0].context.kind.as_deref(), Some("heading-2"));
    assert_eq!(d.units[1].context.key.as_deref(), Some("/content/1"));

    prefix_all(&mut d, "[xx] ");
    let out = json(&f.merge(&d).unwrap());
    let src = json(CONTENTFUL);
    for val in ["embedded-entry-inline", "embedded-entry-block", "embedded-asset-block", "x-future-block", "entry-hyperlink"] {
        assert_eq!(nodes(&out, "nodeType", val), nodes(&src, "nodeType", val), "{val} must be byte identical");
    }
    assert_eq!(nodes(&out, "nodeType", "hyperlink")[0]["data"], json!({"uri": "https://example.com/pricing"}));
    let texts_out: Vec<&str> = nodes(&out, "nodeType", "text").iter().map(|n| n["value"].as_str().unwrap()).collect();
    assert!(texts_out.contains(&"npm install"));
    let bolds: Vec<&str> = nodes(&out, "nodeType", "text").iter().filter(|n| n["marks"] == json!([{"type": "bold"}])).map(|n| n["value"].as_str().unwrap()).collect();
    assert_eq!(bolds, ["quick guide"]);
    let link = &nodes(&out, "nodeType", "hyperlink")[0]["content"];
    assert_eq!(link[0]["value"], "our ");
    assert_eq!(link[1]["marks"], json!([{"type": "italic"}]));
    // a rebuilt text node has LocOS's exact shape
    for n in nodes(&out, "nodeType", "text") {
        assert_eq!(n.as_object().unwrap().len(), 4, "{n}");
    }
    let raw = String::from_utf8(f.merge(&d).unwrap()).unwrap();
    assert!(raw.contains("\"nodeType\": \"text\",\n"), "the file's layout is kept (2 space indent)");
    assert!(raw.ends_with("}\n"));
    // numbers keep their spelling in what was not translated
    assert!(raw.contains("12345678901234567890") && raw.contains("\"n\": 1.50"), "{raw}");
}

#[test]
fn contentful_cjk_reorder_and_nested_tokens() {
    let f = Contentful;
    let d0 = open(&f, CONTENTFUL);
    let cjk = "⟦g2⟧料金ページ⟦/g2⟧を見て、⟦g1⟧クイックガイド⟦/g1⟧を読んでください。⟦x4⟧⟦g5⟧関連記事⟦/g5⟧";
    // the nested italic token (g3) is dropped: refused
    assert!(refused(&f, d0.clone(), 1, cjk));
    let ok = cjk.replace("⟦g2⟧", "⟦g2⟧⟦g3⟧").replace("⟦/g2⟧", "⟦/g3⟧⟦/g2⟧");
    let mut d = d0.clone();
    set(&mut d, 1, &ok).unwrap();
    let out = json(&f.merge(&d).unwrap());
    let p = &out["content"][1]["content"];
    assert_eq!(p[0]["nodeType"], "hyperlink");
    assert_eq!(p[0]["data"], json!({"uri": "https://example.com/pricing"}));
    assert_eq!(p[0]["content"][0]["marks"], json!([{"type": "italic"}]));
    assert_eq!(p[2]["marks"], json!([{"type": "bold"}]));
}

#[test]
fn contentful_token_damage_is_refused() {
    let f = Contentful;
    let d0 = open(&f, CONTENTFUL);
    let base = texts(&d0)[1].clone();
    let cases = [
        ("dropped open+close", base.replace("⟦g1⟧", "").replace("⟦/g1⟧", "")),
        ("dropped point token", base.replace("⟦x4⟧", "")),
        ("duplicated token", format!("{base}⟦x4⟧")),
        ("unopened close", base.replace("⟦g1⟧", "")),
        ("crossed", base.replacen("⟦/g1⟧", "⟦/g2⟧", 1).replacen("⟦/g2⟧⟦/g2⟧", "⟦/g2⟧", 1)),
    ];
    for (name, text) in cases {
        assert!(refused(&f, d0.clone(), 1, &text), "{name}");
    }
    // a unit with no translation is simply left as it is (LocOS holds the whole field instead)
    let mut d = d0.clone();
    set(&mut d, 0, "Los geht es").unwrap();
    let out = json(&f.merge(&d).unwrap());
    let src = json(CONTENTFUL);
    assert_eq!(out["content"][0]["content"][0]["value"], "Los geht es");
    assert_eq!(out["content"][1], src["content"][1]);
}

#[test]
fn contentful_unit_rules() {
    let f = Contentful;
    // text holding the placeholder brackets is not a unit; a non-document is refused
    assert!(open(&f, &doc(vec![blk("paragraph", vec![t("a ⟦g1⟧ b", &[])])])).units.is_empty());
    assert!(matches!(f.extract(b"{\"nodeType\": \"paragraph\"}", &opts()), Err(Error::Parse { .. })));
    assert!(matches!(f.extract(b"null", &opts()), Err(Error::Parse { .. })));
    // unknown block types are never entered; an unknown inline rides along
    let unknown_block = json!({"nodeType": "x-callout", "data": {"tone": "warn"}, "content": [blk("paragraph", vec![t("Do not translate me", &[])])]});
    let unknown_inline = json!({"nodeType": "x-mention", "data": {"id": 7}, "content": [t("@zed", &[])]});
    let src = doc(vec![unknown_block.clone(), blk("paragraph", vec![t("Hello ", &[]), unknown_inline.clone(), t(" world", &[])])]);
    let mut d = open(&f, &src);
    assert_eq!(texts(&d), ["Hello ⟦x1⟧ world"]);
    set(&mut d, 0, "Hallo ⟦x1⟧ Welt").unwrap();
    let out = json(&f.merge(&d).unwrap());
    assert_eq!(out["content"][0], unknown_block, "unknown block byte identical");
    assert_eq!(nodes(&out, "nodeType", "x-mention")[0], &unknown_inline);
    assert!(refused(&f, open(&f, &src), 0, "Hallo Welt"));
    // nothing translatable: no units, the merge is the original bytes
    let nothing = doc(vec![
        blk("paragraph", vec![t("", &[])]),
        json!({"nodeType": "embedded-entry-block", "data": {"target": {"sys": {"id": "e8"}}}, "content": []}),
        blk("hr", vec![]),
        blk("paragraph", vec![t("2026", &[])]),
        blk("paragraph", vec![json!({"nodeType": "embedded-entry-inline", "data": {}, "content": []})]),
    ]);
    let d = open(&f, &nothing);
    assert!(d.units.is_empty());
    assert_eq!(f.merge(&d).unwrap(), nothing);
}

#[test]
fn contentful_sibling_reorder_and_malformed_children() {
    let f = Contentful;
    let link = json!({"nodeType": "hyperlink", "data": {"uri": "https://example.com/pricing"}, "content": [t("our ", &[]), t("pricing page", &["italic"])]});
    let embed = json!({"nodeType": "embedded-entry-inline", "data": {"target": {"sys": {"id": "e7"}}}, "content": []});
    let src = doc(vec![blk("paragraph", vec![t("See ", &[]), link.clone(), t(" and ", &[]), embed.clone(), t(".", &[])])]);
    let d0 = open(&f, &src);
    assert!(texts(&d0)[0].contains("⟦x3⟧"), "{:?}", texts(&d0));
    let mut d = d0.clone();
    set(&mut d, 0, "⟦x3⟧と⟦g1⟧⟦g2⟧料金⟦/g2⟧ページ⟦/g1⟧を参照。").unwrap();
    let out = json(&f.merge(&d).unwrap());
    let para = &out["content"][0]["content"];
    assert_eq!(para[0], embed);
    assert_eq!(para[2]["data"], link["data"]);
    assert!(refused(&f, d0, 0, "⟦x3⟧⟦x3⟧⟦g1⟧⟦g2⟧a⟦/g2⟧b⟦/g1⟧"));
    // a malformed (non-object) child rides along verbatim
    let bad = doc(vec![blk("paragraph", vec![t("Hello world", &[]), json!("junk"), json!(null)])]);
    let mut d = open(&f, &bad);
    assert_eq!(texts(&d), ["Hello world⟦x1⟧⟦x2⟧"]);
    set(&mut d, 0, "Hallo Welt⟦x1⟧⟦x2⟧").unwrap();
    let out = json(&f.merge(&d).unwrap());
    assert_eq!(out["content"][0]["content"][1], json!("junk"));
    assert_eq!(out["content"][0]["content"][2], json!(null));
    assert_eq!(out["content"][0]["content"][0]["value"], "Hallo Welt");
}

#[test]
fn contentful_empty_link_is_kept_whole() {
    // LocOS would hold such a field; here the empty wrapper is a verbatim tag, so a merge cannot fail on it
    let f = Contentful;
    let src = doc(vec![blk("paragraph", vec![t("Open ", &[]), json!({"nodeType": "hyperlink", "data": {"uri": "u"}, "content": []})])]);
    let mut d = open(&f, &src);
    prefix_all(&mut d, "~");
    assert!(f.merge(&d).is_ok());
}

// ---------------------------------------------------------------- Sanity

#[test]
fn sanity_walker() {
    let f = Sanity;
    let mut d = open(&f, PORTABLE);
    let tx = texts(&d);
    assert_eq!(tx, ["Read ⟦g1⟧the docs⟦/g1⟧ now.⟦x2⟧", "Heading", "Run ⟦x1⟧ first"]);
    assert_eq!(d.units[2].context.kind.as_deref(), Some("list-item"));
    assert_eq!(d.units[0].codes["1"].label.as_deref(), Some("strong link"));
    set(&mut d, 0, "Lisez ⟦g1⟧la doc⟦/g1⟧ maintenant.⟦x2⟧").unwrap();
    let out = json(&f.merge(&d).unwrap());
    let src = json(PORTABLE);
    assert_eq!(out[1], src[1], "image block verbatim");
    assert_eq!(out[4], src[4], "code block verbatim");
    assert_eq!(out[0]["markDefs"], src[0]["markDefs"]);
    let kids = out[0]["children"].as_array().unwrap();
    let marked: Vec<_> = kids.iter().filter(|c| c["marks"].as_array().is_some_and(|m| !m.is_empty())).collect();
    assert_eq!(marked.len(), 1);
    assert_eq!(marked[0]["text"], "la doc");
    assert_eq!(marked[0]["_key"], "s2");
    assert_eq!(marked[0]["marks"], json!(["strong", "lk1"]));
    assert_eq!(kids[0]["_key"], "a1t1", "new spans get unique keys seeded by the block key");
    let keys: std::collections::HashSet<_> = kids.iter().map(|c| c["_key"].as_str().unwrap().to_string()).collect();
    assert_eq!(keys.len(), kids.len(), "span keys unique");
    assert_eq!(kids[3], src[0]["children"][3], "the inline object is copied");
    // the other blocks were not translated: byte identical
    assert_eq!(out[2], src[2]);
    assert_eq!(out[3], src[3]);
}

#[test]
fn sanity_broken_tokens_are_refused() {
    let f = Sanity;
    let d0 = open(&f, PORTABLE);
    for broken in ["Lisez la doc maintenant.⟦x2⟧", "⟦g1⟧a⟦g1⟧b⟦/g1⟧⟦x2⟧", "⟦/g1⟧x⟦g1⟧⟦x2⟧", "⟦g1⟧⟦x2⟧z⟦/g1⟧"] {
        assert!(refused(&f, d0.clone(), 0, broken), "{broken}");
    }
    // text holding the placeholder brackets stays verbatim
    let code = bytes(&json!([{"_type": "block", "_key": "c", "children": [{"_type": "span", "_key": "x", "text": "a⟦b", "marks": []}]}]));
    assert!(open(&f, &code).units.is_empty());
    // LocOS walks any list: an empty field or a list without text blocks has no units (the sniff still says no)
    assert!(open(&f, b"[1, 2]").units.is_empty());
    assert!(open(&f, b"[]").units.is_empty());
    assert!(!f.sniff(b"[]"));
    assert!(matches!(f.extract(b"{}", &opts()), Err(Error::Parse { .. })));
}

// ---------------------------------------------------------------- Jira ADF

fn leaf_opts() -> ExtractOptions {
    opts().with_format_option("adf.units", "leaf")
}

fn adf_doc(content: serde_json::Value) -> Vec<u8> {
    bytes(&json!({"type": "doc", "version": 1, "content": content}))
}

fn at(x: &str) -> serde_json::Value {
    json!({"type": "text", "text": x})
}

fn atm(x: &str, marks: serde_json::Value) -> serde_json::Value {
    json!({"type": "text", "text": x, "marks": marks})
}

fn para(c: Vec<serde_json::Value>) -> serde_json::Value {
    json!({"type": "paragraph", "content": c})
}

fn reverse_all(d: &mut Document) {
    for i in 0..d.units.len() {
        let s: String = plain_text(&d.units[i].source()).chars().rev().collect();
        let id = d.units[i].segments[0].id.clone();
        d.set_target(&id, vec![Inline::text(s)]).unwrap();
    }
}

#[test]
fn adf_inline_one_unit_per_paragraph() {
    let f = Adf;
    let mut d = open(&f, JIRA);
    assert_eq!(texts(&d)[0], "Click ⟦g1⟧here⟦/g1⟧ to reset, ⟦g2⟧now⟦/g2⟧⟦x3⟧ thanks");
    assert_eq!(texts(&d).len(), 3, "the code block is not a unit: {:?}", texts(&d));
    // Japanese style reorder: marks follow the words
    set(&mut d, 0, "⟦g2⟧今⟦/g2⟧⟦x3⟧ ここを⟦g1⟧クリック⟦/g1⟧").unwrap();
    let out = json(&f.merge(&d).unwrap());
    let src = json(JIRA);
    let kids = out["content"][0]["content"].as_array().unwrap();
    let by_text = |s: &str| kids.iter().find(|k| k["text"] == s).unwrap();
    assert_eq!(by_text("今")["marks"], json!([{"type": "strong"}]));
    assert_eq!(by_text("クリック")["marks"][0]["attrs"]["href"], "https://acme.co/reset");
    assert!(kids.iter().any(|k| k["type"] == "mention" && k["attrs"]["id"] == "u1"));
    assert_eq!(out["content"][1], src["content"][1], "code block untouched");
    assert_eq!(out["content"][2], src["content"][2]);
    // dropped, invented or nested tokens are refused
    for broken in ["Haga clic aqui", "⟦g1⟧a⟦/g1⟧ ⟦g2⟧b ⟦x3⟧", "⟦g1⟧a ⟦g2⟧b⟦/g2⟧⟦/g1⟧ ⟦x3⟧"] {
        assert!(refused(&f, open(&f, JIRA), 0, broken), "{broken}");
    }
    // a paragraph without marks stays plain text
    let one = adf_doc(json!([para(vec![at("Just words")])]));
    let plain = open(&f, &one);
    assert_eq!(texts(&plain), ["Just words"]);
    assert_eq!(f.merge(&plain).unwrap(), one);
}

#[test]
fn adf_leaf_mode_cases() {
    let f = Adf;
    let nested = adf_doc(json!([{"type": "bulletList", "content": [
        {"type": "listItem", "content": [para(vec![at("First item")]), {"type": "orderedList", "content": [
            {"type": "listItem", "content": [para(vec![at("Nested one")])]}, {"type": "listItem", "content": [para(vec![at("Nested two")])]}]}]},
        {"type": "listItem", "content": [para(vec![at("Second item")])]}]}]));
    let mut d = f.extract(&nested, &leaf_opts()).unwrap();
    assert_eq!(texts(&d), ["First item", "Nested one", "Nested two", "Second item"]);
    prefix_all(&mut d, "[T]");
    let out = json(&f.merge(&d).unwrap());
    assert_eq!(out["content"][0]["content"][0]["content"][1]["type"], "orderedList");
    assert_eq!(out["content"][0]["content"][0]["content"][1]["content"][0]["content"][0]["content"][0]["text"], "[T]Nested one");

    let table = adf_doc(json!([{"type": "table", "content": [{"type": "tableRow", "content": [
        {"type": "tableHeader", "content": [para(vec![atm("Head", json!([{"type": "strong"}]))])]},
        {"type": "tableCell", "content": [para(vec![atm("bold", json!([{"type": "strong"}])), at(" and "), atm("italic", json!([{"type": "em"}]))])]}]}]}]));
    let mut d = f.extract(&table, &leaf_opts()).unwrap();
    assert_eq!(texts(&d), ["Head", "bold", " and ", "italic"]);
    reverse_all(&mut d);
    let out = json(&f.merge(&d).unwrap());
    let cell = &out["content"][0]["content"][0]["content"][1]["content"][0]["content"];
    assert_eq!((cell[0]["text"].as_str(), &cell[0]["marks"]), (Some("dlob"), &json!([{"type": "strong"}])));
    assert_eq!((cell[2]["text"].as_str(), &cell[2]["marks"]), (Some("cilati"), &json!([{"type": "em"}])));

    let mention = json!({"type": "mention", "attrs": {"id": "557058:abc", "text": "@Alex"}});
    let emoji = json!({"type": "emoji", "attrs": {"shortName": ":tada:", "id": "1f389", "text": "🎉"}});
    let me = adf_doc(json!([para(vec![at("Hey "), mention.clone(), at(" check "), emoji.clone(), at(" now")])]));
    let mut d = f.extract(&me, &leaf_opts()).unwrap();
    assert_eq!(texts(&d), ["Hey ", " check ", " now"]);
    reverse_all(&mut d);
    let out = json(&f.merge(&d).unwrap());
    let inline = &out["content"][0]["content"];
    assert_eq!((&inline[1], &inline[3]), (&mention, &emoji));
    assert_eq!((inline[0]["text"].as_str(), inline[2]["text"].as_str()), (Some(" yeH"), Some(" kcehc ")));

    let code = adf_doc(json!([para(vec![at("Run this:")]), {"type": "codeBlock", "attrs": {"language": "python"}, "content": [at("print('hello world')")]}]));
    let mut d = f.extract(&code, &leaf_opts()).unwrap();
    assert_eq!(texts(&d), ["Run this:"]);
    reverse_all(&mut d);
    let out = json(&f.merge(&d).unwrap());
    assert_eq!(out["content"][1]["content"][0]["text"], "print('hello world')");
    assert_eq!(out["content"][0]["content"][0]["text"], ":siht nuR");

    let link = adf_doc(json!([para(vec![at("See "), atm("the docs", json!([{"type": "link", "attrs": {"href": "https://example.com/docs"}}])), at(" here")])]));
    let mut d = f.extract(&link, &leaf_opts()).unwrap();
    assert_eq!(texts(&d), ["See ", "the docs", " here"]);
    reverse_all(&mut d);
    let out = json(&f.merge(&d).unwrap());
    assert_eq!(out["content"][0]["content"][1]["text"], "scod eht");
    assert_eq!(out["content"][0]["content"][1]["marks"][0]["attrs"]["href"], "https://example.com/docs");
}

#[test]
fn adf_options() {
    let f = Adf;
    assert!(matches!(f.extract(JIRA, &opts().with_format_option("adf.units", "words")), Err(Error::FormatOption { .. })));
    assert!(matches!(f.extract(JIRA, &opts().with_format_option("adf.unts", "leaf")), Err(Error::FormatOption { .. })));
    // another format's key is left alone
    assert!(f.extract(JIRA, &opts().with_format_option("csv.columns", "1")).is_ok());
    // the mode is remembered by the document: merge needs no option
    let mut d = f.extract(JIRA, &leaf_opts()).unwrap();
    assert!(d.units.len() > 3);
    prefix_all(&mut d, "x");
    assert!(f.merge(&d).is_ok());
}

// ---------------------------------------------------------------- Notion

fn run(c: &str, bold: bool) -> serde_json::Value {
    json!({"type": "text", "text": {"content": c, "link": null}, "annotations": {"bold": bold}, "plain_text": c, "href": null})
}

#[test]
fn notion_walker_round_trip() {
    let f = Notion;
    let mut d = open(&f, NOTION);
    assert_eq!(texts(&d), ["Hello ", "world", " site", "More", "Nested step.", "Sample code", "Plan", "Price"]);
    assert_eq!(d.units[1].context.extra["block_id"], "b1");
    for (i, s) in ["Hallo ", "Welt", " Seite", "Mehr", "Verschachtelt.", "Beispielcode", "Plan", "Preis"].iter().enumerate() {
        set(&mut d, i, s).unwrap();
    }
    let out = json(&f.merge(&d).unwrap());
    let src = json(NOTION);
    let runs = out[0]["paragraph"]["rich_text"].as_array().unwrap();
    let contents: Vec<_> = runs.iter().map(|r| r["text"]["content"].as_str().map(str::to_string)).collect();
    assert_eq!(contents, [Some("Hallo ".into()), Some("Welt".into()), Some(" Seite".into()), None]);
    assert_eq!(runs[1]["annotations"]["bold"], true);
    assert_eq!(runs[1]["plain_text"], "Welt");
    assert_eq!(runs[3], src[0]["paragraph"]["rich_text"][3], "a mention is kept");
    assert_eq!(out[1]["toggle"]["rich_text"][0]["text"]["content"], "Mehr");
    assert_eq!(out[1]["toggle"]["children"][0]["paragraph"]["rich_text"][0]["text"]["content"], "Verschachtelt.");
    // the code text is not a unit, its caption is
    assert_eq!(out[2]["code"]["rich_text"], src[2]["code"]["rich_text"]);
    assert_eq!(out[2]["code"]["caption"][0]["text"]["content"], "Beispielcode");
    assert_eq!(out[3]["table_row"]["cells"][1][0]["text"]["content"], "Preis");
    assert_eq!(out[2]["code"]["language"], "python");
}

#[test]
fn notion_run_over_the_cap_splits_with_annotations() {
    let f = Notion;
    let src = bytes(&json!([{"object": "block", "id": "b", "type": "paragraph", "paragraph": {"rich_text": [run("x", false), run("y", true)]}}]));
    let mut d = open(&f, &src);
    let long = "A".repeat(4500);
    let id = d.units[1].segments[0].id.clone();
    d.set_target(&id, vec![Inline::text(long.clone())]).unwrap();
    let out = json(&f.merge(&d).unwrap());
    let runs = out[0]["paragraph"]["rich_text"].as_array().unwrap();
    assert_eq!(runs.len(), 4, "x + 3 pieces");
    let lens: Vec<usize> = runs[1..].iter().map(|r| r["text"]["content"].as_str().unwrap().chars().count()).collect();
    assert_eq!(lens, [2000, 2000, 500]);
    assert!(runs[1..].iter().all(|r| r["annotations"]["bold"] == true));
    let joined: String = runs[1..].iter().map(|r| r["text"]["content"].as_str().unwrap()).collect();
    assert_eq!(joined, long);
    assert_eq!(runs[0], json(&src)[0]["paragraph"]["rich_text"][0]);
    // characters, not bytes
    let mut d = open(&f, &src);
    let id = d.units[0].segments[0].id.clone();
    d.set_target(&id, vec![Inline::text("日".repeat(2001))]).unwrap();
    let out = json(&f.merge(&d).unwrap());
    assert_eq!(out[0]["paragraph"]["rich_text"].as_array().unwrap().len(), 3);
}

#[test]
fn notion_code_children_and_list_response() {
    let f = Notion;
    // everything inside a code block is code; its caption is not
    let src = bytes(&json!({"object": "list", "results": [{"object": "block", "id": "c", "type": "code", "code": {"rich_text": [run("let x", false)], "caption": [run("A caption", false)], "children": [{"object": "block", "id": "n", "type": "paragraph", "paragraph": {"rich_text": [run("nested in code", false)], "caption": [run("nested caption", false)]}}]}}]}));
    let mut d = open(&f, &src);
    assert_eq!(texts(&d), ["A caption", "nested caption"]);
    prefix_all(&mut d, "~");
    let out = json(&f.merge(&d).unwrap());
    assert_eq!(out["results"][0]["code"]["caption"][0]["text"]["content"], "~A caption");
    assert_eq!(out["results"][0]["code"]["children"][0]["paragraph"]["rich_text"][0]["text"]["content"], "nested in code");
    assert!(matches!(f.extract(b"{\"a\": 1}", &opts()), Err(Error::Parse { .. })));
}

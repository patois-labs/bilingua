//! Regressions found by running real files from the Internet through the pseudo round trip (hand-written inputs).
mod common;
use bilingua_model::*;
use bilingua_richtext::*;
use common::*;
use serde_json::json;

#[test]
fn contentful_link_without_text_can_be_translated() {
    let doc = json!({"nodeType": "document", "data": {}, "content": [{"nodeType": "paragraph", "data": {}, "content": [
        {"nodeType": "text", "value": "Deploy it here:", "marks": [], "data": {}},
        {"nodeType": "hyperlink", "data": {"uri": "https://example.test/x"}, "content": [{"nodeType": "text", "value": "", "marks": [], "data": {}}]},
        {"nodeType": "text", "value": "", "marks": [], "data": {}}]}]});
    let bytes = serde_json::to_vec(&doc).unwrap();
    let mut d = open(&Contentful, &bytes);
    d.target_lang = Some("de-DE".into());
    prefix_all(&mut d, "Zq");
    let out = Contentful.merge(&d).expect("merge");
    let v = json(&out);
    assert!(v.to_string().contains("ZqDeploy it here:"));
    assert_eq!(nodes(&v, "nodeType", "hyperlink").len(), 1);
}

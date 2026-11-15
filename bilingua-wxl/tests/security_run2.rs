//! Follow-up of security review run 2: the XML guard accepts a quote inside a comment of a DOCTYPE internal subset
//! (BILI-60), so writing `Culture` must skip comments and processing instructions in the subset the same way
//! instead of pairing that quote and missing the root tag.
mod common;
use common::*;

fn culture_after(data: &[u8]) -> String {
    let mut t = ex(data);
    pseudo(&mut t);
    t.target_lang = Some("fr-FR".into());
    String::from_utf8(merged(&t)).unwrap()
}

#[test]
fn culture_is_written_after_a_subset_comment_with_a_quote() {
    for subset in ["<!-- it's a comment -->", "<!-- \"one -->", "<!-- ' --><!-- ' -->", "<?pi data?><!-- don't -->"] {
        let data = format!(
            "<?xml version=\"1.0\"?>\n<!DOCTYPE WixLocalization [{subset}]>\n<WixLocalization Culture=\"en-us\" xmlns=\"urn:x\">\n<String Id=\"a\" Value=\"Hello\"/></WixLocalization>"
        );
        let out = culture_after(data.as_bytes());
        assert!(out.contains("<WixLocalization Culture=\"fr-fr\" xmlns=\"urn:x\">"), "{subset}: {out}");
        assert!(out.contains(&format!("<!DOCTYPE WixLocalization [{subset}]>")), "{subset}: {out}");
    }
}

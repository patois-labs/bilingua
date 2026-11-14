//! Regression test for the VSDX fix of the 0.9.1 security audit: the duplicate-attribute check of the XML reader
//! is linear in the number of attributes of one start tag.

use bilingua_model::{Error, ExtractOptions, Filter};
use bilingua_vsdx::Vsdx;
use bilingua_zip::{write_zip, EntryMeta};
use std::time::{Duration, Instant};

const NS: &str = "http://schemas.microsoft.com/office/visio/2012/main";

fn package(pages: String) -> Vec<u8> {
    write_zip(&[(EntryMeta { name: "visio/pages/pages.xml".into(), stored: false, dir: false, modified: None }, pages.into_bytes())]).unwrap()
}

fn pages(extra_attrs: &str) -> String {
    format!("<Pages xmlns='{NS}'><Page ID='0' Name='Overview'{extra_attrs}/></Pages>")
}

/// One start tag with 60,000 attributes: about fifteen seconds of scanning in a debug build before the fix (it
/// grew fourfold per doubling), a fraction of a second after.
#[test]
fn a_tag_with_many_attributes_is_read_in_linear_time() {
    let attrs: String = (0..60_000).map(|i| format!(" a{i:07}=''")).collect();
    let bytes = package(pages(&attrs));
    let start = Instant::now();
    let doc = Vsdx.extract(&bytes, &ExtractOptions::default()).unwrap();
    let took = start.elapsed();
    assert_eq!(doc.units.len(), 1);
    assert!(took < Duration::from_secs(5), "took {took:?}");
}

/// A repeated attribute name is still refused as a parse error, among few attributes and among many.
#[test]
fn a_repeated_attribute_is_still_refused() {
    let many: String = (0..40).map(|i| format!(" a{i}=''")).collect();
    for attrs in [" x='1' x='2'".to_string(), format!("{many} a3=''"), format!("{many} a39=''"), format!(" Name='again'{many}")] {
        match Vsdx.extract(&package(pages(&attrs)), &ExtractOptions::default()) {
            Err(Error::Parse { .. }) => {}
            Err(e) => panic!("expected a parse error, got {e}"),
            Ok(_) => panic!("a repeated attribute was accepted: {}", &attrs[..attrs.len().min(40)]),
        }
    }
    // the same number of different names is fine
    assert_eq!(Vsdx.extract(&package(pages(&many)), &ExtractOptions::default()).unwrap().units.len(), 1);
}

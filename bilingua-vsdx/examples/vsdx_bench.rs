//! `vsdx_bench gen OUT.vsdx [shapes]` writes a large drawing; `vsdx_bench time FILE` times extract and merge.
use bilingua_model::*;
use bilingua_zip::{write_zip, EntryMeta};
use std::time::Instant;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    match a[0].as_str() {
        "gen" => {
            let n: usize = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(40_000);
            let ns = "http://schemas.microsoft.com/office/visio/2012/main";
            let mut p = format!("<?xml version='1.0' encoding='utf-8' ?>\r\n<PageContents xmlns='{ns}' xml:space='preserve'><Shapes>");
            for i in 0..n {
                p.push_str(&format!(
                    "<Shape ID='{i}' Type='Shape'><Cell N='PinX' V='1.5'/><Cell N='PinY' V='2.5'/><Section N='Character'><Row IX='0'><Cell N='LangID' V='1033'/></Row></Section><Text><cp IX='0'/><pp IX='0'/>Shape number {i} says hello to the whole wide world of localization, <cp IX='1'/>with a bold part<cp IX='0'/> and a closing sentence of some length.\n</Text></Shape>"
                ));
            }
            p.push_str("</Shapes></PageContents>");
            let pages = format!("<Pages xmlns='{ns}' xmlns:r='http://schemas.openxmlformats.org/officeDocument/2006/relationships'><Page ID='0' NameU='Page-1' Name='Big page'><Rel r:id='rId1'/></Page></Pages>");
            let rels = "<Relationships xmlns='http://schemas.openxmlformats.org/package/2006/relationships'><Relationship Id='rId1' Type='x/page' Target='page1.xml'/></Relationships>";
            let m = |n: &str| EntryMeta { name: n.into(), stored: false, dir: false, modified: None };
            let z = write_zip(&[
                (m("visio/pages/pages.xml"), pages.into_bytes()),
                (m("visio/pages/_rels/pages.xml.rels"), rels.as_bytes().to_vec()),
                (m("visio/pages/page1.xml"), p.into_bytes()),
            ])
            .unwrap();
            std::fs::write(&a[1], z).unwrap();
        }
        _ => {
            let bytes = std::fs::read(&a[1]).unwrap();
            let t = Instant::now();
            let mut doc = bilingua_vsdx::Vsdx.extract(&bytes, &ExtractOptions::default()).unwrap();
            let t_ex = t.elapsed();
            let n = doc.units.len();
            for u in &mut doc.units {
                let mut t = u.source();
                if let Some(Inline::Text(s)) = t.first_mut() {
                    s.insert_str(0, "[x] ");
                }
                u.segments[0].target = t;
            }
            let t = Instant::now();
            let out = bilingua_vsdx::Vsdx.merge(&doc).unwrap();
            let t_mg = t.elapsed();
            println!("{{\"units\": {n}, \"extract_s\": {:.3}, \"merge_s\": {:.3}, \"out_bytes\": {}}}", t_ex.as_secs_f64(), t_mg.as_secs_f64(), out.len());
        }
    }
}

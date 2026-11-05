#![allow(dead_code)]
use bilingua_htmlhelp::{HhcFilter, HhkFilter, HhpFilter, MergeOptions};
use bilingua_model::encoding_rs::{Encoding, WINDOWS_1252};
use bilingua_model::*;

pub fn cp1252(s: &str) -> Vec<u8> {
    let (b, _, bad) = WINDOWS_1252.encode(s);
    assert!(!bad, "not Windows-1252: {s:?}");
    b.into_owned()
}

pub fn enc(e: &'static Encoding, s: &str) -> Vec<u8> {
    let (b, _, bad) = e.encode(s);
    assert!(!bad, "not {}: {s:?}", e.name());
    b.into_owned()
}

pub fn dec(e: &'static Encoding, b: &[u8]) -> String {
    e.decode_without_bom_handling(b).0.into_owned()
}

pub fn extract(f: &dyn Filter, data: &[u8]) -> Result<Document, Error> {
    f.extract(data, &ExtractOptions::default())
}

pub fn texts(f: &dyn Filter, data: &[u8]) -> Vec<String> {
    extract(f, data).expect("extract").units.iter().map(|u| plain_text(&u.source())).collect()
}

/// Sets targets by unit index and merges with an optional target language and options.
pub fn merge_with(f: &dyn Filter, data: &[u8], map: &[(usize, &str)], target: Option<&str>, mo: MergeOptions) -> Result<Vec<u8>, Error> {
    let mut doc = extract(f, data)?;
    doc.target_lang = target.map(str::to_string);
    for (i, t) in map {
        let id = doc.units[*i].segments[0].id.clone();
        doc.set_target(&id, vec![Inline::text(*t)]).expect("set_target");
    }
    match doc.format.as_str() {
        "hhc" => HhcFilter.merge_with(&doc, &mo),
        "hhk" => HhkFilter.merge_with(&doc, &mo),
        _ => HhpFilter.merge_with(&doc, &mo),
    }
}

pub fn build(f: &dyn Filter, data: &[u8], map: &[(usize, &str)]) -> Vec<u8> {
    merge_with(f, data, map, None, MergeOptions::default()).expect("merge")
}

pub fn build_loc(f: &dyn Filter, data: &[u8], map: &[(usize, &str)], target: &str) -> Result<Vec<u8>, Error> {
    merge_with(f, data, map, Some(target), MergeOptions::default())
}

pub fn build_loc_utf8(f: &dyn Filter, data: &[u8], map: &[(usize, &str)], target: &str) -> Vec<u8> {
    merge_with(f, data, map, Some(target), MergeOptions { utf8_fallback: true }).expect("merge")
}

pub const HHC: &str = concat!(
    "<!DOCTYPE HTML PUBLIC \"-//IETF//DTD HTML//EN\">\r\n",
    "<HTML>\r\n<HEAD>\r\n",
    "<meta name=\"GENERATOR\" content=\"Microsoft&reg; HTML Help Workshop 4.1\">\r\n",
    "<!-- Sitemap 1.0 -->\r\n",
    "</HEAD><BODY>\r\n",
    "<OBJECT type=\"text/site properties\">\r\n",
    "\t<param name=\"ImageType\" value=\"Folder\">\r\n",
    "\t<param name=\"Font\" value=\"Tahoma,8,0\">\r\n",
    "</OBJECT>\r\n",
    "<UL>\r\n",
    "\t<LI> <OBJECT type=\"text/sitemap\">\r\n",
    "\t\t<param name=\"Name\" value=\"Getting started\">\r\n",
    "\t\t<param name=\"Local\" value=\"html\\start.htm\">\r\n",
    "\t\t<param name=\"ImageNumber\" value=\"1\">\r\n",
    "\t\t</OBJECT>\r\n",
    "\t<UL>\r\n",
    "\t\t<LI> <OBJECT type=\"text/sitemap\">\r\n",
    "\t\t\t<param name=\"Name\" value=\"Caf\u{e9} &amp; &quot;Bar&quot; &lt;tips&gt;\">\r\n",
    "\t\t\t<param name=\"Local\" value=\"html/caf\u{e9}.htm#top\">\r\n",
    "\t\t\t</OBJECT>\r\n",
    "\t\t<LI> <OBJECT type=\"text/sitemap\">\r\n",
    "\t\t\t<PARAM NAME=name VALUE='Use %1 and {0} here'>\r\n",
    "\t\t\t<param name=URL value=http://example.test/x.htm>\r\n",
    "\t\t\t</OBJECT>\r\n",
    "\t\t<LI> <OBJECT type=\"text/sitemap\">\r\n",
    "\t\t\t<param name=\"Name\" value=\"---\">\r\n",
    "\t\t\t<param name=\"Comment\" value=\"Not translated\">\r\n",
    "\t\t\t</OBJECT>\r\n",
    "\t</UL>\r\n",
    "\t<LI> <OBJECT type=\"text/sitemap\">\r\n",
    "\t\t<param name=\"Name\" value=\"Unquoted\">\r\n",
    "\t\t<param name=\"Merge\" value=\"other.chm::/toc.hhc\">\r\n",
    "\t\t</OBJECT>\r\n",
    "</UL>\r\n",
    "<param name=\"Name\" value=\"Outside any object\">\r\n",
    "</BODY></HTML>\r\n",
);

pub const HHC_UNITS: [&str; 4] = ["Getting started", "Caf\u{e9} & \"Bar\" <tips>", "Use %1 and {0} here", "Unquoted"];

pub const HHK: &str = concat!(
    "<!DOCTYPE HTML PUBLIC \"-//IETF//DTD HTML//EN\">\n",
    "<HTML><HEAD><meta http-equiv=\"Content-Type\" content=\"text/html; charset=windows-1252\">\n",
    "<!-- Sitemap 1.0 -->\n</HEAD><BODY>\n<UL>\n",
    "\t<LI> <OBJECT type=\"text/sitemap\">\n",
    "\t\t<param name=\"Name\" value=\"printing\">\n",
    "\t\t<param name=\"Name\" value=\"How to print\">\n",
    "\t\t<param name=\"Local\" value=\"print.htm\">\n",
    "\t\t<param name=\"Name\" value=\"Print settings\">\n",
    "\t\t<param name=\"Local\" value=\"settings.htm\">\n",
    "\t\t</OBJECT>\n",
    "\t<UL>\n",
    "\t\t<LI> <OBJECT type=\"text/sitemap\">\n",
    "\t\t\t<param name=\"Keyword\" value=\"page setup\">\n",
    "\t\t\t<param name=\"Local\" value=\"setup.htm\">\n",
    "\t\t\t</OBJECT>\n",
    "\t</UL>\n",
    "\t<LI> <OBJECT type=\"text/sitemap\">\n",
    "\t\t<param name=\"Name\" value=\"output\">\n",
    "\t\t<param name=\"See Also\" value=\"printing\">\n",
    "\t\t</OBJECT>\n",
    "\t<LI> <OBJECT type=\"text/sitemap\">\n",
    "\t\t<param name=\"Name\" value=\"paper\">\n",
    "\t\t<param name=\"See Also\" value=\"no such keyword\">\n",
    "\t\t</OBJECT>\n",
    "</UL>\n</BODY></HTML>\n",
);

pub const HHP: &str = concat!(
    "[OPTIONS]\r\n",
    "Compatibility=1.1 or later\r\n",
    "Compiled file=manual.chm\r\n",
    "Contents file=toc.hhc\r\n",
    "Default topic=start.htm\r\n",
    "Display compile progress=No\r\n",
    "Index file=index.hhk\r\n",
    "Language=0x409 English (United States)\r\n",
    "Title=Product Manual\r\n",
    "\r\n",
    "[WINDOWS]\r\n",
    "main=\"Product Help\",\"toc.hhc\",\"index.hhk\",\"start.htm\",\"start.htm\",,,,,0x23520,,",
    "0x387e,,,,,,,,0\r\n",
    "\r\n",
    "[FILES]\r\n",
    "start.htm\r\n",
    "; Title=not an option here\r\n",
    "\r\n",
    "[INFOTYPES]\r\n",
);

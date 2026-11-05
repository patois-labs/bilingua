//! Gate: encodings (Windows-1252, UTF-16 with a BOM), `merge_with` options and the `wiki.template_params` format
//! option.
mod common;
use bilingua_model::*;
use bilingua_wiki::{DokuWiki, MediaWiki, TEMPLATE_PARAMS_KEY};
use common::*;

const MW_PAGE: &str = "== Caf\u{e9} ==\nPrix: 5 \u{20ac} pour un caf\u{e9} '''cr\u{e8}me'''.\r\n* Na\u{ef}ve [[Page|\u{e9}l\u{e8}ve]]\n";
const DW_PAGE: &str = "====== Caf\u{e9} ======\nPrix: 5 \u{20ac} pour un **caf\u{e9}**.\r\n  * Na\u{ef}ve [[ns:page|\u{e9}l\u{e8}ve]]\n";

fn utf16(s: &str, le: bool) -> Vec<u8> {
    let mut v = if le { vec![0xFF, 0xFE] } else { vec![0xFE, 0xFF] };
    for u in s.encode_utf16() {
        v.extend(if le { u.to_le_bytes() } else { u.to_be_bytes() });
    }
    v
}

fn cp1252(s: &str) -> Vec<u8> {
    encoding_rs::WINDOWS_1252.encode(s).0.into_owned()
}

fn translate_all(f: &dyn Filter, data: &[u8], word: &str) -> Document {
    let mut doc = extract(f, data).unwrap();
    for u in doc.units.iter_mut() {
        let mut t = vec![Inline::text(format!("{word} "))];
        t.extend(u.source());
        u.segments[0].target = t;
    }
    doc
}

fn filters() -> Vec<(Box<dyn Filter>, &'static str)> {
    vec![(Box::new(MediaWiki::default()), MW_PAGE), (Box::new(DokuWiki), DW_PAGE)]
}

#[test]
fn utf16_with_a_bom_reads_like_utf8_and_stays_utf16() {
    for (f, page) in filters() {
        let want = texts(f.as_ref(), page.as_bytes());
        for le in [true, false] {
            let data = utf16(page, le);
            assert_eq!(texts(f.as_ref(), &data), want);
            assert_identity(f.as_ref(), &data);
            let doc = translate_all(f.as_ref(), &data, "\u{65e5}\u{672c}");
            let out = f.merge(&doc).unwrap();
            assert_eq!(&out[..2], &data[..2], "BOM kept");
            let back = texts(f.as_ref(), &out);
            assert!(back[0].starts_with("\u{65e5}\u{672c} "), "{back:?}");
            // LocOS behaviour: UTF-8 out, no UTF-16 BOM
            let u8out = f.merge_with(&doc, &MergeOptions::new().with_output_encoding(OutputEncoding::Utf8)).unwrap();
            let utf8_doc = translate_all(f.as_ref(), page.as_bytes(), "\u{65e5}\u{672c}");
            assert_eq!(u8out, f.merge(&utf8_doc).unwrap());
        }
    }
}

#[test]
fn windows_1252_is_kept_and_utf8_on_request() {
    for (f, page) in filters() {
        let data = cp1252(page);
        assert!(std::str::from_utf8(&data).is_err());
        assert_eq!(texts(f.as_ref(), &data), texts(f.as_ref(), page.as_bytes()));
        assert_identity(f.as_ref(), &data);
        let doc = translate_all(f.as_ref(), &data, "Pr\u{e9}\u{20ac}");
        let out = f.merge(&doc).unwrap();
        assert!(std::str::from_utf8(&out).is_err(), "still Windows-1252");
        assert!(out.windows(2).any(|w| w == b"\xe9\x80"));
        let u8out = f.merge_with(&doc, &bilingua_model::profile::locos().merge_options("xml")).unwrap();
        let utf8_doc = translate_all(f.as_ref(), page.as_bytes(), "Pr\u{e9}\u{20ac}");
        assert_eq!(u8out, f.merge(&utf8_doc).unwrap(), "LocOS writes UTF-8");
        // stamp_target_always has nothing to stamp: an untouched merge stays the original bytes
        let mut d = extract(f.as_ref(), &data).unwrap();
        d.target_lang = Some("de-DE".into());
        let keep = MergeOptions::new().with_stamp_target_always(true);
        assert_eq!(f.merge_with(&d, &keep).unwrap(), data);
    }
}

#[test]
fn template_params_from_format_options() {
    let page = "{{Quote|text=To be or not|author=Someone}}\nWords {{Note|Remember this}} here.\n";
    let plain = texts(&MediaWiki::default(), page.as_bytes());
    assert_eq!(plain.len(), 1, "{plain:?}");
    let mut opts = ExtractOptions::default();
    opts.format_options.insert(TEMPLATE_PARAMS_KEY.into(), "Quote:text,note:1".into());
    let doc = MediaWiki::default().extract(page.as_bytes(), &opts).unwrap();
    let got: Vec<String> = doc.units.iter().map(token_text).collect();
    let want = texts(&MediaWiki::with_template_params("Quote:text,note:1"), page.as_bytes());
    assert_eq!(got, want);
    assert_eq!(got.len(), 2, "{got:?}");
    assert_eq!(got[0], "To be or not");
    // the option adds to the filter's own list
    let doc = MediaWiki::with_template_params("Quote:text").extract(page.as_bytes(), &{
        let mut o = ExtractOptions::default();
        o.format_options.insert(TEMPLATE_PARAMS_KEY.into(), "Note:1".into());
        o
    });
    assert_eq!(doc.unwrap().units.iter().map(token_text).collect::<Vec<_>>(), want);
    // an empty option changes nothing; the option means nothing to DokuWiki
    opts.format_options.insert(TEMPLATE_PARAMS_KEY.into(), " ".into());
    assert_eq!(MediaWiki::default().extract(page.as_bytes(), &opts).unwrap().units.len(), 1);
    // a template parameter merges back escaped
    let mut doc = MediaWiki::with_template_params("Quote:text").extract(page.as_bytes(), &ExtractOptions::default()).unwrap();
    let id = doc.units[0].segments[0].id.clone();
    doc.set_target(&id, vec![Inline::text("a|b=c")]).unwrap();
    let out = String::from_utf8(MediaWiki::default().merge(&doc).unwrap()).unwrap();
    assert!(out.starts_with("{{Quote|text=a{{!}}b{{=}}c|author=Someone}}"), "{out}");
}

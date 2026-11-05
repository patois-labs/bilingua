//! A large generated EPUB (over 5 MB of text), extracted and merged. Run
//! `cargo test --release -p bilingua-epub --test speed -- --nocapture` for the timings. Set `EPUB_SPEED_OUT` to a
//! folder to also save `big.epub`, so the LocOS filter can be timed on the same bytes (`tools/time_locos.py`).
mod common;
use bilingua_epub::Epub;
use bilingua_model::*;
use common::*;
use std::time::Instant;

pub fn big() -> Vec<u8> {
    let words = ["harbor", "lantern", "bread", "window", "morning", "garden", "ticket", "river", "paper", "market"];
    let chapters = 60usize;
    let paras = 450usize;
    let mut members: Vec<(String, Vec<u8>)> = Vec::new();
    let mut manifest = String::new();
    let mut spine = String::new();
    let mut ncx_points = String::new();
    for c in 0..chapters {
        let mut s = String::with_capacity(160_000);
        s.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" lang=\"en\" xml:lang=\"en\">\n<head><title>Chapter ");
        s.push_str(&format!("{c}</title></head>\n<body>\n<h1>Chapter {c} of the guide</h1>\n"));
        for p in 0..paras {
            let w = |k: usize| words[(c * 31 + p * 7 + k) % words.len()];
            s.push_str(&format!(
                "<p>Open the <b>{}</b> {} near the <a href=\"c{c}.xhtml#p{p}\">{}</a> at 9 and bring {} for number {p} of chapter {c}, then walk to the {} &amp; wait. <img src=\"i.png\" alt=\"Picture of the {} {p}\"/></p>\n",
                w(1), w(2), w(3), w(4), w(5), w(6)
            ));
        }
        s.push_str("</body>\n</html>\n");
        members.push((format!("OEBPS/c{c}.xhtml"), s.into_bytes()));
        manifest.push_str(&format!("<item id=\"c{c}\" href=\"c{c}.xhtml\" media-type=\"application/xhtml+xml\"/>\n"));
        spine.push_str(&format!("<itemref idref=\"c{c}\"/>\n"));
        ncx_points.push_str(&format!("<navPoint id=\"p{c}\" playOrder=\"{}\"><navLabel><text>Chapter {c} of the guide</text></navLabel><content src=\"c{c}.xhtml\"/></navPoint>\n", c + 1));
    }
    let opf = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<package xmlns=\"http://www.idpf.org/2007/opf\" version=\"2.0\" unique-identifier=\"uid\"><metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:identifier id=\"uid\">urn:x</dc:identifier><dc:title>The big guide</dc:title><dc:language>en</dc:language></metadata><manifest>\n<item id=\"ncx\" href=\"toc.ncx\" media-type=\"application/x-dtbncx+xml\"/>\n{manifest}</manifest><spine toc=\"ncx\">\n{spine}</spine></package>"
    );
    let ncx = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<ncx xmlns=\"http://www.daisy.org/z3986/2005/ncx/\" version=\"2005-1\"><docTitle><text>The big guide</text></docTitle><navMap>\n{ncx_points}</navMap></ncx>");
    let mut all = vec![
        ("META-INF/container.xml".to_string(), CONTAINER.as_bytes().to_vec()),
        ("OEBPS/content.opf".to_string(), opf.into_bytes()),
        ("OEBPS/toc.ncx".to_string(), ncx.into_bytes()),
    ];
    all.extend(members);
    use std::io::Write;
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    w.start_file("mimetype", zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored)).unwrap();
    w.write_all(b"application/epub+zip").unwrap();
    for (n, d) in &all {
        w.start_file(n.as_str(), zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated)).unwrap();
        w.write_all(d).unwrap();
    }
    w.finish().unwrap().into_inner()
}

#[test]
#[cfg_attr(debug_assertions, ignore = "timed in release: cargo test --release -p bilingua-epub --test speed")]
fn large_book_round_trips_and_is_timed() {
    let bytes = big();
    let (m, _) = members(&bytes);
    let text: usize = m.values().map(Vec::len).sum();
    assert!(text >= 5_000_000, "only {text} bytes of content");
    if let Ok(dir) = std::env::var("EPUB_SPEED_OUT") {
        std::fs::write(format!("{dir}/big.epub"), &bytes).unwrap();
    }
    let o = ExtractOptions { target_lang: Some("ru".into()), limits: Some(Limits { max_units: 500_000, ..Limits::default() }), ..Default::default() };
    let t = Instant::now();
    let mut d = Epub.extract(&bytes, &o).unwrap();
    let t_extract = t.elapsed();
    let n = d.units.len();
    assert!(n > 18_000, "{n} units");
    let t = Instant::now();
    assert_eq!(Epub.merge(&d).unwrap(), bytes);
    let t_same = t.elapsed();
    for i in 0..d.units.len() {
        let u = &d.units[i];
        let tgt: Vec<Inline> = u
            .source()
            .iter()
            .map(|x| match x {
                Inline::Text(t) => Inline::text(format!("RU {t}")),
                o => o.clone(),
            })
            .collect();
        let id = u.segments[0].id.clone();
        d.set_target(&id, tgt).unwrap();
    }
    let t = Instant::now();
    let out = Epub.merge(&d).unwrap();
    let t_merge = t.elapsed();
    let again = Epub.extract(&out, &o).unwrap();
    assert_eq!(again.units.len(), n);
    eprintln!(
        "EPUB speed: {} MB of content, {n} units: extract {:.0} ms, untouched merge {:.0} ms, translated merge {:.0} ms",
        text / 1_000_000,
        t_extract.as_secs_f64() * 1000.0,
        t_same.as_secs_f64() * 1000.0,
        t_merge.as_secs_f64() * 1000.0
    );
}

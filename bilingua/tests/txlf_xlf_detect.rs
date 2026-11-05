//! A Wordfast Pro TXLF saved as `.xlf` goes to the TXLF filter (gs4tr rules); a plain `.xlf` stays XLIFF.

use bilingua::*;

const TXLF: &[u8] = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\" xmlns:gs4tr=\"http://www.gs4tr.org/schema/xliff-ext\">\n<file original=\"a.txt\" source-language=\"en-US\" target-language=\"de-DE\" datatype=\"plaintext\"><body>\n<trans-unit id=\"1\"><source>Open the file.</source></trans-unit>\n<trans-unit id=\"2\" gs4tr:locked=\"true\"><source>Locked line.</source></trans-unit>\n</body></file></xliff>\n";
const PLAIN: &[u8] = b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\">\n<file original=\"a.txt\" source-language=\"en-US\" target-language=\"de-DE\" datatype=\"plaintext\"><body>\n<trans-unit id=\"1\"><source>Open the file.</source><note>Exported from gs4tr tools.</note></trans-unit>\n</body></file></xliff>\n";

#[test]
fn a_gs4tr_xlf_goes_to_txlf() {
    for name in ["job_de.xlf", "JOB.XLF", "job.xliff"] {
        assert_eq!(detect(name, TXLF).unwrap().id(), "txlf", "{name}");
    }
    let mut doc = extract("job_de.xlf", TXLF, &ExtractOptions::default()).unwrap();
    assert_eq!(doc.format, "txlf");
    assert_eq!(doc.units.len(), 1, "the gs4tr-locked unit is protected");
    assert_eq!(merge(&doc).unwrap(), TXLF);
    let id = doc.units[0].segments[0].id.clone();
    doc.set_target(&id, vec![Inline::text("Datei öffnen.")]).unwrap();
    let out = String::from_utf8(merge(&doc).unwrap()).unwrap();
    assert!(out.contains("<target>Datei öffnen.</target>"), "{out}");
}

#[test]
fn a_plain_xlf_stays_xliff() {
    assert_eq!(detect("job_de.xlf", PLAIN).unwrap().id(), "xliff");
    assert_eq!(detect("job_de.xliff", PLAIN).unwrap().id(), "xliff");
    // and a .txlf is always TXLF
    assert_eq!(detect("job_de.txlf", PLAIN).unwrap().id(), "txlf");
}

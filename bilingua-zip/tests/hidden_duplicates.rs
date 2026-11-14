//! Two members with one name, in the layouts a hand scan of the central directory misreads (a zip64 end record,
//! bytes before the archive). The zip reader keeps one entry per name, so without a check the second copy would
//! vanish; every layout must be refused with a typed error.

use bilingua_model::{Error, LimitKind};
use bilingua_zip::hostile::{layouts, stored_zip};
use bilingua_zip::{read_zip, refuse_duplicate_names, ZipLimits};

const A: &[u8] = b"<a>first</a>";
const B: &[u8] = b"<a>second</a>";

fn refused(z: &[u8], what: &str) {
    match read_zip("docx", z, &ZipLimits::default()) {
        Err(Error::Limit(e)) => {
            assert_eq!(e.kind, LimitKind::ZipMalformed, "{what}: {}", e.msg);
            assert!(e.msg.contains("twice"), "{what}: {}", e.msg);
        }
        Err(e) => panic!("{what}: expected a typed zip error, got {e}"),
        Ok(es) => panic!("{what}: read with {} entries", es.len()),
    }
}

#[test]
fn every_layout_of_a_sound_archive_reads() {
    let z = stored_zip(&[("word/document.xml", A), ("word/styles.xml", B)]);
    for (layout, z) in layouts(&z) {
        let es = read_zip("docx", &z, &ZipLimits::default()).unwrap_or_else(|e| panic!("{layout}: {e}"));
        assert_eq!(es.len(), 2, "{layout}");
        assert_eq!(es[0].data, A, "{layout}");
    }
}

#[test]
fn the_same_name_twice_is_refused_in_every_layout() {
    let z = stored_zip(&[("word/document.xml", A), ("word/document.xml", B)]);
    for (layout, z) in layouts(&z) {
        refused(&z, layout);
        let zip = zip::ZipArchive::new(std::io::Cursor::new(&z)).unwrap();
        assert_eq!(zip.len(), 1, "{layout}: the reader shows one of the two");
        assert!(refuse_duplicate_names(&zip).is_err(), "{layout}");
    }
}

#[test]
fn case_and_slash_twins_are_refused_in_every_layout() {
    for twin in ["WORD/Document.xml", "word\\document.xml", "./word/document.xml", "word//document.xml"] {
        let z = stored_zip(&[("word/document.xml", A), (twin, B)]);
        for (layout, z) in layouts(&z) {
            refused(&z, &format!("{twin} ({layout})"));
        }
    }
}

#[test]
fn a_duplicate_among_many_members_is_refused() {
    let names: Vec<String> = (0..40).map(|i| format!("p/{i}.xml")).collect();
    let mut members: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), A)).collect();
    members.push(("p/17.xml", B));
    for (layout, z) in layouts(&stored_zip(&members)) {
        refused(&z, layout);
    }
}

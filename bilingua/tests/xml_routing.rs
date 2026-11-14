//! `.xml` routing by root element (LocOS `XmlResourceDispatch`): only a `<resources>` or `<properties>` root goes
//! to the Android or Java XML properties filter. Checked over the whole oracle corpus (no file of another format is
//! taken) and over `.xml` files of the other XML vocabularies.

use std::path::PathBuf;

fn corpus() -> Vec<(String, String, Vec<u8>)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus");
    let mut out = Vec::new();
    for dir in std::fs::read_dir(&root).unwrap().flatten() {
        if !dir.path().is_dir() {
            continue;
        }
        let folder = dir.file_name().to_string_lossy().to_string();
        for f in std::fs::read_dir(dir.path()).unwrap().flatten() {
            let name = f.file_name().to_string_lossy().to_string();
            if !name.starts_with('.') {
                out.push((folder.clone(), name, std::fs::read(f.path()).unwrap()));
            }
        }
    }
    assert!(out.len() > 100, "corpus went missing");
    out
}

fn is_xmlres(id: &str) -> bool {
    matches!(id, "android" | "javaxml")
}

#[test]
fn no_corpus_file_of_another_format_is_taken() {
    let mut taken = 0;
    for (folder, name, bytes) in corpus() {
        // as uploaded, and renamed to .xml (an XML file of any vocabulary may arrive as .xml)
        let stem = name.rsplit_once('.').map_or(name.as_str(), |(s, _)| s);
        for n in [name.clone(), format!("{stem}.xml"), stem.to_string()] {
            let Some(f) = bilingua::detect(&n, &bytes) else { continue };
            if folder == "xmlres" {
                if n.ends_with(".xml") {
                    assert!(is_xmlres(f.id()), "{folder}/{name} as {n}: {}", f.id());
                    taken += 1;
                }
            } else {
                assert!(!is_xmlres(f.id()), "{folder}/{name} as {n} went to {}", f.id());
            }
        }
    }
    assert!(taken >= 7, "the xmlres corpus should route by root: {taken}");
}

#[test]
fn other_xml_vocabularies_in_a_dot_xml_file_stay_where_they_were() {
    for (what, xml) in [
        ("xliff", &b"<?xml version=\"1.0\"?>\n<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\"><file source-language=\"en\" datatype=\"plaintext\" original=\"a\"><body><trans-unit id=\"1\"><source>Hi</source></trans-unit></body></file></xliff>\n"[..]),
        ("ttml", b"<?xml version=\"1.0\"?>\n<tt xmlns=\"http://www.w3.org/ns/ttml\"><body><div><p begin=\"0s\" end=\"1s\">Hi</p></div></body></tt>\n"),
        ("sdlproj", b"<?xml version=\"1.0\"?>\n<Project Guid=\"x\"><LanguageDirections/></Project>\n"),
        ("dita", b"<?xml version=\"1.0\"?>\n<!DOCTYPE topic PUBLIC \"-//OASIS//DTD DITA Topic//EN\" \"topic.dtd\">\n<topic id=\"t\"><title>Hi</title><body><p>Text with resources and properties.</p></body></topic>\n"),
        ("resx", b"<?xml version=\"1.0\"?>\n<root><data name=\"a\"><value>Hi</value></data></root>\n"),
        ("its", b"<?xml version=\"1.0\"?>\n<doc><!-- <resources> --><properties-list><resources>Hi</resources></properties-list></doc>\n"),
    ] {
        let f = bilingua::detect("file.xml", xml).unwrap_or_else(|| panic!("{what}: refused"));
        assert!(!is_xmlres(f.id()), "{what} went to {}", f.id());
    }
    // the roots themselves, with a prolog, a DOCTYPE and a prefix, do route
    for (xml, id) in [
        (&b"\xef\xbb\xbf<?xml version=\"1.0\"?>\n<!-- c -->\n<resources><string name=\"a\">Hi</string></resources>\n"[..], "android"),
        (b"<?xml version=\"1.0\"?>\n<!DOCTYPE properties SYSTEM \"http://java.sun.com/dtd/properties.dtd\">\n<properties><entry key=\"a\">Hi</entry></properties>\n", "javaxml"),
    ] {
        assert_eq!(bilingua::detect("strings.xml", xml).unwrap().id(), id);
    }
}

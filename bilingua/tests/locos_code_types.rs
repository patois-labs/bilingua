//! BILI-118 (unit model step 5): under the LocOS profile no inline code has a type or attributes, so LocOS's dicts
//! keep today's shape (design rule 7). Without the profile the same files do get types, so the check is not empty.
use bilingua::*;
use std::path::Path;

fn files(fmt: &str) -> Vec<std::path::PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/locos").join(fmt);
    let mut v: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some(fmt))
        .collect();
    v.sort();
    v
}

#[test]
fn locos_profile_leaves_code_types_unset() {
    let mut typed = 0;
    for fmt in ["docx", "pptx", "html", "odt", "md"] {
        let mut seen = 0;
        for p in files(fmt) {
            let name = p.file_name().unwrap().to_string_lossy().to_string();
            let bytes = std::fs::read(&p).unwrap();
            let Ok(doc) = extract(&name, &bytes, &profile::locos().extract_options()) else { continue };
            seen += 1;
            for u in &doc.units {
                for (k, c) in u.codes() {
                    assert!(c.code_type().is_none() && c.attrs().is_empty(), "{name} unit {} code {k}: {c:?}", u.id);
                }
            }
            if let Ok(plain) = extract(&name, &bytes, &ExtractOptions::default()) {
                typed += plain.units.iter().flat_map(|u| u.codes().values()).filter(|c| c.code_type().is_some()).count();
            }
        }
        assert!(seen > 0, "no {fmt} file extracted under the profile");
    }
    assert!(typed > 0, "no file has a typed code without the profile");
}

//! The merged files open in the real application: when LibreOffice is installed, translated corpus files and an
//! Okapi package are converted to PDF headless (skipped, with a note, when `soffice` is not found).
mod common;
use bilingua_model::*;
use common::*;

fn soffice() -> Option<&'static str> {
    [
        r"C:\Program Files\LibreOffice\program\soffice.exe",
        r"C:\Program Files (x86)\LibreOffice\program\soffice.exe",
        "/usr/bin/soffice",
        "/usr/local/bin/soffice",
        "/Applications/LibreOffice.app/Contents/MacOS/soffice",
    ]
    .into_iter()
    .find(|p| std::path::Path::new(p).exists())
}

fn translate_all(ext: &str, bytes: &[u8], lang: &str) -> Vec<u8> {
    let f = filter_for(ext);
    let mut d = f.extract(bytes, &opts(Some(lang))).unwrap();
    for (i, u) in d.units.iter_mut().enumerate() {
        let sample = ["Hello harbor", "\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}", "\u{645}\u{631}\u{62d}\u{628}\u{627} \u{628}\u{643}\u{645}", "Gr\u{fc}\u{df}e aus dem Hafen"][i % 4];
        // paired codes are dropped and point codes (frames, footnotes) are kept by the merge
        let target = vec![Inline::text(format!("{sample} {i}"))];
        u.segments[0].target = target;
    }
    f.merge(&d).unwrap()
}

#[test]
fn libreoffice_opens_the_merged_files_when_installed() {
    let Some(soffice) = soffice() else {
        eprintln!("skipped: LibreOffice is not installed");
        return;
    };
    let dir = std::env::temp_dir().join(format!("bilingua-odf-lo-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let jobs = [
        ("../oracle/corpus/odf/guide.odt", "ar"),
        ("../oracle/corpus/odf/sales.ods", "he"),
        ("../oracle/corpus/odf/slides.odp", "ja"),
        ("../oracle/corpus/odf/flow.odg", "de-DE"),
        ("tests/data/okapi/TestDocument01.odt", "ko"),
        ("tests/data/okapi/TestSpreadsheet01.ods", "zh-Hans"),
    ];
    let profile = dir.join("profile");
    let url = format!("-env:UserInstallation=file:///{}", profile.to_string_lossy().replace('\\', "/"));
    for (path, lang) in jobs {
        let src = std::fs::read(root.join(path)).unwrap();
        let ext = path.rsplit('.').next().unwrap();
        let out = translate_all(ext, &src, lang);
        let stem = format!("m-{}-{lang}", path.rsplit('/').next().unwrap().replace('.', "_"));
        let file = dir.join(format!("{stem}.{ext}"));
        std::fs::write(&file, &out).unwrap();
        let o = std::process::Command::new(soffice).arg(&url).args(["--headless", "--convert-to", "pdf", "--outdir"]).arg(&dir).arg(&file).output().unwrap();
        let pdf = dir.join(format!("{stem}.pdf"));
        assert!(o.status.success() && pdf.exists(), "{path} {lang}: {}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
        assert!(std::fs::metadata(&pdf).unwrap().len() > 1000, "{path}: empty pdf");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

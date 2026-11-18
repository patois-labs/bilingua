//! Writes the golden JSON of `bilingua-model/tests/golden/` (unit model step 1). Ignored: run it on purpose, from
//! the commit whose output the fixtures should pin, then copy the files over:
//!
//! `M5_TEST_ARGS="-p bilingua-cli --test golden_gen -- --ignored" m5-test.sh <branch> test`
//!
//! It writes to `$BILINGUA_GOLDEN_OUT`, or to `golden-gen` under cargo's test temp dir (on the M5,
//! `~/bili/targets/<branch>/tmp/golden-gen`), and never into the repository. The checked-in fixtures were made
//! this way by main `626e6f04`; the README there names the sources.
use std::path::PathBuf;

#[test]
#[ignore = "writes the golden fixtures on purpose; see the module docs"]
fn write_golden_fixtures() {
    let out = std::env::var_os("BILINGUA_GOLDEN_OUT").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("golden-gen"));
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let corpus = root.join("oracle/corpus");
    // CLI skeletons, made with the memb bench command (`extract <file> -t fr -o x.xlf`)
    for f in ["docx/features.docx", "xlsx/menu.xlsx", "pptx/launch.pptx", "po/lumen.po", "arb/app_en.arb", "xcstrings/bakery.xcstrings", "json/strings.json"] {
        let xlf = out.join(format!("{}.xlf", f.replace('/', "__")));
        let args: Vec<String> = vec!["extract".into(), corpus.join(f).to_string_lossy().into(), "-t".into(), "fr".into(), "-o".into(), xlf.to_string_lossy().into()];
        bilingua_cli::run(&args, &mut Vec::new()).unwrap();
        std::fs::remove_file(&xlf).unwrap();
        let skl = out.join(format!("{}.xlf.skl.json", f.replace('/', "__")));
        std::fs::rename(&skl, out.join(format!("{}.skl.json", f.replace('/', "__")))).unwrap();
    }
    // a ChatterCAT state.json: the server's extract options, four segments edited, serde_json::to_vec as persist() does
    let bytes = std::fs::read(corpus.join("docx/features.docx")).unwrap();
    let opts = bilingua::ExtractOptions::new().with_source_lang("en").with_target_lang("fr").with_granularity(bilingua::Granularity::Sentence).with_include_locked(true);
    let mut doc = bilingua::extract("features.docx", &bytes, &opts).unwrap();
    let segs: Vec<(String, Vec<bilingua::Inline>)> = doc.segments().map(|(_, s)| (s.id.clone(), s.source.clone())).take(4).collect();
    let states = [bilingua::SegState::Translated, bilingua::SegState::Confirmed, bilingua::SegState::Draft, bilingua::SegState::Reviewed];
    for ((id, src), st) in segs.into_iter().zip(states) {
        doc.set_target(&id, src).unwrap();
        doc.set_state(&id, st).unwrap();
    }
    std::fs::write(out.join("chattercat_docx.state.json"), serde_json::to_vec(&doc).unwrap()).unwrap();
    // stored by the old server (image 5620cb3e), read and written again by this commit
    let old = root.join("../core/catos-server/tests/fixtures/old-skeletons/data/files");
    for (dir, name) in [("f1a1db279dad005", "old_docx"), ("f1a1db279daf007", "old_po")] {
        let raw = std::fs::read(old.join(dir).join("state.json")).unwrap();
        std::fs::write(out.join(format!("{name}.state.json")), &raw).unwrap();
        let d: bilingua::Document = serde_json::from_slice(&raw).unwrap();
        std::fs::write(out.join(format!("{name}.state.resaved.json")), serde_json::to_vec(&d).unwrap()).unwrap();
    }
    eprintln!("golden fixtures written to {}", out.display());
}

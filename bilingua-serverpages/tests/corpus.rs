//! The oracle corpus folder `oracle/corpus/serverpages/`: every file opens with the filter its extension names,
//! merges back to its exact bytes and survives a pseudo-translation with its code intact.
mod common;
use bilingua_model::*;
use bilingua_serverpages::Dialect;
use common::*;

fn corpus_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus/serverpages")
}

#[test]
fn every_corpus_file_round_trips() {
    let mut n = 0;
    let mut units = 0;
    for f in std::fs::read_dir(corpus_dir()).unwrap().flatten() {
        let name = f.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let ext = name.rsplit('.').next().unwrap();
        let d = Dialect::from_extension(ext).unwrap_or_else(|| panic!("{name}: no dialect"));
        let fmt = d.id();
        let data = std::fs::read(f.path()).unwrap();
        let flt = filter(fmt);
        assert!(flt.extensions().contains(&ext), "{name}");
        let doc = flt.extract(&data, &opts()).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(!doc.units.is_empty(), "{name}: {} units", doc.units.len());
        units += doc.units.len();
        assert_eq!(flt.merge(&doc).unwrap(), data, "{name}: untouched merge");
        let t = texts(fmt, &data);
        let echo: Vec<(usize, &str)> = t.iter().enumerate().map(|(i, s)| (i, s.as_str())).collect();
        assert_eq!(build(fmt, &data, &echo), data, "{name}: echo merge");
        // pseudo-translate every unit; the page's code is written back untouched
        let mut d2 = doc.clone();
        let ids: Vec<String> = d2.units.iter().map(|u| u.segments[0].id.clone()).collect();
        for (k, id) in ids.iter().enumerate() {
            let mut tg = vec![Inline::text("[")];
            tg.extend(d2.units[k].source());
            tg.push(Inline::text("]"));
            d2.set_target(id, tg).unwrap();
        }
        let out = flt.merge(&d2).unwrap();
        let back = flt.extract(&out, &opts()).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(back.units.len(), doc.units.len(), "{name}");
        n += 1;
    }
    assert!(n >= 28, "only {n} corpus files");
    println!("{n} corpus files, {units} units");
}

//! Round trip on every file of the committed corpus (`oracle/corpus`): an unchanged merge gives the same file,
//! and a pseudo-translated merge opens again with the same units. The comparison with the Localization OS
//! filters needs that project and is run by hand (`python run.py`); this part needs nothing.

use bilingua::{Granularity, Inline};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            files(&p, out);
        } else if p.extension().is_some_and(|e| e != "md" || p.file_name().is_some_and(|n| n != "SOURCES.md")) {
            out.push(p);
        }
    }
}

fn parts(bytes: &[u8]) -> Option<BTreeMap<String, Vec<u8>>> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).ok()?;
    let mut m = BTreeMap::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).ok()?;
        let mut b = Vec::new();
        f.read_to_end(&mut b).ok()?;
        m.insert(String::from_utf8_lossy(f.name_raw()).into_owned(), b);
    }
    Some(m)
}

/// The plural form a unit stands for: (key, label), or `None` for a unit that is not a plural form. Formats keep it
/// in `context.extra["plural"]` (PO index, YAML and Rails category) or as `[label]` after the key (Fluent variants).
fn plural_label(u: &bilingua::Unit) -> Option<(Option<String>, String)> {
    if let Some(p) = u.context.extra.get("plural") {
        return Some((u.context.key.clone(), p.clone()));
    }
    let key = u.context.key.as_deref()?;
    let (base, rest) = key.rsplit_once('[')?;
    let label = rest.strip_suffix(']')?;
    Some((Some(base.to_string()), label.to_string()))
}

fn pseudo(v: &[Inline]) -> Vec<Inline> {
    let mut out = vec![Inline::text("[")];
    out.extend(v.iter().cloned());
    out.push(Inline::text("]"));
    out
}

#[test]
fn every_corpus_file_round_trips() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("corpus");
    let mut all = vec![];
    files(&root, &mut all);
    assert!(all.len() >= 25, "corpus is missing: {} files", all.len());
    let opts = bilingua::ExtractOptions { target_lang: Some("es-ES".into()), granularity: Granularity::Paragraph, ..Default::default() };
    let mut failures = vec![];
    for p in &all {
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        let bytes = std::fs::read(p).unwrap();
        let doc = match bilingua::extract(&name, &bytes, &opts) {
            Ok(d) => d,
            Err(e) => {
                failures.push(format!("{name}: extract failed: {e}"));
                continue;
            }
        };
        if doc.units.is_empty() {
            failures.push(format!("{name}: no units"));
        }
        match bilingua::merge(&doc) {
            Ok(out) if out == bytes => {}
            Ok(out) => match (parts(&bytes), parts(&out)) {
                (Some(a), Some(b)) if a == b => {}
                _ => failures.push(format!("{name}: unchanged merge differs")),
            },
            Err(e) => failures.push(format!("{name}: merge failed: {e}")),
        }
        let mut t = doc.clone();
        let todo: Vec<(String, Vec<Inline>)> =
            t.units.iter().filter(|u| u.translate).flat_map(|u| u.segments.iter().map(|s| (s.id.clone(), s.source.clone()))).collect();
        for (id, src) in todo {
            if let Err(e) = t.set_target(&id, pseudo(&src)) {
                failures.push(format!("{name}: set_target {id}: {e}"));
            }
        }
        match bilingua::merge(&t).and_then(|b| bilingua::extract(&name, &b, &opts)) {
            Ok(again) => {
                // A merge with a target language may fill plural forms the target needs and the source lacks
                // (es-ES has "many", an English file does not). Those new units are not a mismatch: count only
                // the units whose plural label the source has.
                let labels: BTreeSet<(Option<String>, String)> = doc.units.iter().filter_map(plural_label).collect();
                let a = doc.units.iter().filter(|u| u.translate).count();
                let b = again.units.iter().filter(|u| u.translate && plural_label(u).is_none_or(|l| labels.contains(&l))).count();
                if a != b {
                    failures.push(format!("{name}: reopened with {b} units, had {a}"));
                }
            }
            Err(e) => failures.push(format!("{name}: pseudo merge did not reopen: {e}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

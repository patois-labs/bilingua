//! `bl-dump FILE`: prints one JSON object with Bilingua's units for FILE and the result of its own round-trip
//! checks. Used by `oracle/run.py` to compare Bilingua with other filters. Not shipped.

use bilingua::{Granularity, Inline, TagKind};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::Read;

/// Text with one marker per inline code. A line break code is written as a newline, since other filters
/// keep a line break as white space.
fn render(u: &bilingua::Unit) -> (String, usize) {
    let mut s = String::new();
    let mut tags = 0;
    for i in u.source() {
        match i {
            Inline::Text(t) => s.push_str(&t),
            Inline::Tag { id, .. } if u.codes.get(&id).and_then(|c| c.label.as_deref()).is_some_and(|l| l.contains("line break")) => {
                s.push(' ')
            }
            Inline::Tag { kind, .. } => {
                tags += 1;
                s.push(match kind {
                    TagKind::Open => '\u{E010}',
                    TagKind::Close => '\u{E011}',
                    TagKind::Standalone => '\u{E012}',
                });
            }
        }
    }
    (s, tags)
}

fn units(doc: &bilingua::Document) -> Vec<Value> {
    // A bilingual file has its own segments (LocOS lists one unit per segment), so those are listed one by one.
    let own_segments = matches!(doc.format.as_str(), "xliff" | "sdlxliff" | "mqxliff" | "mxliff" | "sdlppx" | "mqxlz");
    let mut out = Vec::new();
    for u in &doc.units {
        if own_segments {
            for s in &u.segments {
                let mut one = u.clone();
                one.segments = vec![s.clone()];
                one.separators.clear();
                let (text, tags) = render(&one);
                out.push(json!({"id": s.id, "text": text, "tags": tags, "locked": !u.translate, "kind": u.context.kind, "state": s.state}));
            }
        } else {
            let (text, tags) = render(u);
            out.push(json!({"id": u.id, "text": text, "tags": tags, "locked": !u.translate, "kind": u.context.kind, "state": u.segments.first().map(|s| s.state)}));
        }
    }
    out
}

fn zip_parts(bytes: &[u8]) -> Option<BTreeMap<String, Vec<u8>>> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).ok()?;
    let mut m = BTreeMap::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).ok()?;
        if f.is_dir() {
            continue;
        }
        let mut b = Vec::new();
        f.read_to_end(&mut b).ok()?;
        m.insert(String::from_utf8_lossy(f.name_raw()).into_owned(), b);
    }
    Some(m)
}

fn same_package(a: &[u8], b: &[u8]) -> Result<(), String> {
    let (Some(x), Some(y)) = (zip_parts(a), zip_parts(b)) else { return Err("not a readable zip".into()) };
    let mut diffs = vec![];
    for (k, v) in &x {
        match y.get(k) {
            None => diffs.push(format!("missing part {k}")),
            Some(w) if w != v => diffs.push(format!("part {k} differs")),
            _ => {}
        }
    }
    for k in y.keys() {
        if !x.contains_key(k) {
            diffs.push(format!("extra part {k}"));
        }
    }
    if diffs.is_empty() {
        Ok(())
    } else {
        Err(diffs.join("; "))
    }
}

/// The pseudo-translation wraps each unit in letters, not brackets: `[` and `{` mean something in LaTeX, AsciiDoc,
/// wiki markup, WiX, MicroDVD and ASS, so a bracket would be read back as markup and look like a round-trip bug.
const PRE: &str = "Zq";
const POST: &str = "qZ";

/// Text with the tag markers removed: a merge may move the pseudo letters across a tag that opens or closes the
/// unit (`<b>Zq...` vs `Zq<b>...`), which is not a difference in text.
fn no_tags(s: &str) -> String {
    s.chars().filter(|c| !('\u{E010}'..='\u{E012}').contains(c)).collect()
}

/// The plural form a unit stands for (the same rule as `tests/corpus.rs`): a merge with a target language fills
/// the plural forms es-ES needs and the source lacks, and those new units are not a mismatch.
fn plural_label(u: &bilingua::Unit) -> Option<(Option<String>, String)> {
    if let Some(p) = u.context.extra.get("plural") {
        return Some((u.context.key.clone(), p.clone()));
    }
    let key = u.context.key.as_deref()?;
    if let Some((base, last)) = key.rsplit_once('/') {
        if matches!(last, "zero" | "one" | "two" | "few" | "many" | "other") {
            return Some((Some(base.to_string()), last.to_string()));
        }
    }
    let (base, rest) = key.rsplit_once('[')?;
    let label = rest.strip_suffix(']')?;
    Some((Some(base.to_string()), label.to_string()))
}

/// Formats whose translated merge is refused by design (see `tests/corpus.rs`).
const MERGE_UNSUPPORTED_FORMATS: &[&str] = &["pdf"];

fn pseudo(v: &[Inline]) -> Vec<Inline> {
    let mut out = vec![Inline::text(PRE)];
    for i in v {
        match i {
            Inline::Text(t) => out.push(Inline::Text(t.clone())),
            tag => out.push(tag.clone()),
        }
    }
    out.push(Inline::text(POST));
    out
}

fn main() {
    let path = std::env::args().nth(1).unwrap_or_default();
    if path == "--formats" {
        // The parsers this build has and the extensions each claims (used by the scoreboard).
        let list: Vec<Value> = bilingua::filters().iter().map(|f| json!({"id": f.id(), "extensions": f.extensions()})).collect();
        println!("{}", Value::Array(list));
        return;
    }
    let name = path.rsplit(['/', '\\']).next().unwrap_or("").to_string();
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            println!("{}", json!({"error": format!("read: {e}")}));
            return;
        }
    };
    let opts = bilingua::ExtractOptions::new().with_target_lang("es-ES").with_granularity(Granularity::Paragraph);
    let doc = match bilingua::extract(&name, &bytes, &opts) {
        Ok(d) => d,
        Err(e) => {
            println!("{}", json!({"error": format!("extract: {e}")}));
            return;
        }
    };
    let mut out = json!({"format": doc.format, "units": units(&doc)});
    // Round trip 1: nothing translated must give the same file.
    let unchanged = match bilingua::merge(&doc) {
        Err(e) => format!("merge failed: {e}"),
        Ok(b) if b == bytes => "identical".to_string(),
        Ok(b) => match same_package(&bytes, &b) {
            Ok(()) => "identical-parts".to_string(),
            Err(e) => format!("differs: {e}"),
        },
    };
    out["unchanged"] = json!(unchanged);
    // Round trip 2: pseudo-translate every unit, merge, reopen, compare.
    let mut p = doc.clone();
    let mut set_errors = 0;
    let ids: Vec<(String, Vec<Inline>)> = p
        .units
        .iter()
        .filter(|u| u.translate)
        .flat_map(|u| u.segments.iter().map(|s| (s.id.clone(), s.source.clone())))
        .collect();
    for (id, src) in &ids {
        if p.set_target(id, pseudo(src)).is_err() {
            set_errors += 1;
        }
    }
    let merged = bilingua::merge(&p);
    if let (Ok(b), Some(dest)) = (&merged, std::env::args().nth(2)) {
        let _ = std::fs::write(dest, b);
    }
    let pseudo_res = match merged {
        Err(bilingua::Error::Unsupported(e)) if MERGE_UNSUPPORTED_FORMATS.contains(&doc.format.as_str()) => {
            json!({"status": format!("n/a: a translated merge is refused by design ({e})")})
        }
        Err(e) => json!({"status": format!("merge failed: {e}")}),
        Ok(b) => match bilingua::extract(&name, &b, &opts) {
            Err(e) => json!({"status": format!("reopen failed: {e}")}),
            Ok(d2) if matches!(doc.format.as_str(), "po" | "xliff" | "sdlxliff" | "mqxliff" | "mxliff" | "sdlppx" | "mqxlz" | "xcloc") => {
                // A bilingual file keeps its source; the translation comes back as the target.
                let srcs = |d: &bilingua::Document| d.units.iter().filter(|u| u.translate).map(|u| render(u).0).collect::<Vec<_>>();
                let (a, b2) = (srcs(&doc), srcs(&d2));
                let want: Vec<String> = doc
                    .units
                    .iter()
                    .filter(|u| u.translate)
                    .flat_map(|u| u.segments.iter().map(|s| format!("{PRE}{}{POST}", bilingua::plain_text(&s.source))))
                    .collect();
                let got: Vec<String> = d2
                    .units
                    .iter()
                    .filter(|u| u.translate)
                    .flat_map(|u| u.segments.iter().map(|s| bilingua::plain_text(&s.target)))
                    .collect();
                if a != b2 {
                    json!({"status": "sources changed after merge"})
                } else if want != got {
                    let first = want.iter().zip(&got).find(|(x, y)| x != y).map(|(x, y)| format!("{x:?} -> {y:?}"));
                    json!({"status": "targets not read back", "first": first})
                } else {
                    json!({"status": "ok"})
                }
            }
            Ok(d2) if matches!(doc.format.as_str(), "ts" | "xcstrings") => {
                // Qt Linguist and String Catalogs write the translation into the target language and keep the
                // source as it was; their extract reads the source only, so the source must come back unchanged.
                let srcs = |d: &bilingua::Document| d.units.iter().filter(|u| u.translate).map(|u| render(u).0).collect::<Vec<_>>();
                let (a, b2) = (srcs(&doc), srcs(&d2));
                if a == b2 && set_errors == 0 {
                    json!({"status": "ok"})
                } else {
                    json!({"status": format!("source changed after merge ({} units, {} set_target errors)", a.len(), set_errors)})
                }
            }
            Ok(d2) => {
                let labels: std::collections::BTreeSet<(Option<String>, String)> = doc.units.iter().filter_map(plural_label).collect();
                // Android drops the plural forms the target never uses (es-ES has no "two"), as LocOS
                let kept: std::collections::BTreeSet<(Option<String>, String)> = d2.units.iter().filter_map(plural_label).collect();
                let a: Vec<String> = doc
                    .units
                    .iter()
                    .filter(|u| u.translate && (doc.format != "android" || plural_label(u).is_none_or(|l| kept.contains(&l))))
                    .map(|u| no_tags(&render(u).0))
                    .collect();
                let b2: Vec<String> = d2
                    .units
                    .iter()
                    .filter(|u| u.translate && plural_label(u).is_none_or(|l| labels.contains(&l)))
                    .map(|u| no_tags(&render(u).0))
                    .collect();
                if a.len() != b2.len() {
                    json!({"status": format!("reopened with {} units, expected {}", b2.len(), a.len())})
                } else {
                    // Markdown writes a typed markup character with a backslash; the text reads back as the raw slice.
                    let md = matches!(doc.format.as_str(), "markdown" | "mdx");
                    // These formats write the translation beside the source (MO msgstr, desktop `Name[es]`, catkeys,
                    // CSV and STF target columns, TTX/TXML/TXLF/XLZ targets, multilingual XML), so reopening the
                    // merged file gives the source back unchanged.
                    let beside_ok = matches!(doc.format.as_str(), "mo" | "desktop" | "catkeys" | "csv" | "tsv" | "stf" | "ttx" | "txml" | "txlf" | "xlz" | "xml");
                    // Flare topics and EPUB chapters are XHTML read without a DTD: a translation that types an
                    // HTML-only named reference (`&eacute;`, kept as typed in the source text) is written as a
                    // numeric one (`&#233;`), as LocOS's `_xml_named_refs` does, so that is the text to expect.
                    let xml_refs = matches!(doc.format.as_str(), "flprjzip" | "epub");
                    let want = |x: &String| {
                        let w = format!("{PRE}{x}{POST}");
                        if xml_refs { bilingua_html::xml_named_refs(&w) } else { w }
                    };
                    // HTML pages and server pages keep character references as typed in unit text, and the writer
                    // escapes `'`, `"`, `>` and `&` in a translation where a browser needs it (LocOS #672), so the
                    // re-read unit holds `&#x27;` for `'`. Both render the same text: compare after decoding.
                    let html_refs = matches!(doc.format.as_str(), "html" | "aspx" | "jsp" | "phtml" | "shtml" | "erb");
                    let same = |x: &String, y: &String| {
                        if md {
                            want(x).replace('\\', "") == y.replace('\\', "")
                        } else if html_refs {
                            want(x) == *y || bilingua_htmlcore::pyx::unescape(&want(x)) == bilingua_htmlcore::pyx::unescape(y)
                        } else {
                            want(x) == *y
                        }
                    };
                    let beside = a.iter().zip(&b2).filter(|(x, y)| !same(x, y) && beside_ok && x == y).count();
                    let bad = a.iter().zip(&b2).filter(|(x, y)| !same(x, y) && (!beside_ok || x != y)).count();
                    if bad == 0 && set_errors == 0 && beside > 0 {
                        json!({"status": format!("ok ({beside} units keep the source; the translation is written beside it)")})
                    } else if bad == 0 && set_errors == 0 {
                        json!({"status": "ok"})
                    } else {
                        let first = a.iter().zip(&b2).find(|(x, y)| !same(x, y) && (!beside_ok || x != y)).map(|(x, y)| format!("{x:?} -> {y:?}"));
                        json!({"status": format!("{bad} units differ, {set_errors} set_target errors"), "first": first})
                    }
                }
            }
        },
    };
    out["pseudo"] = pseudo_res;
    println!("{out}");
}

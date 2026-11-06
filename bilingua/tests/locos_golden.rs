//! Track D: the LocOS-profile goldens. Each `tests/locos/<format>/<file>` has a `<file>.golden.json` written by
//! `oracle/locos_golden.py` with the Localization OS venv: the units LocOS's filter gives (p_index, text with its
//! tokens, style) and its builds under a few mappings. This test does what the LocOS adapter does: extract under
//! the `locos` profile at paragraph granularity, number the translatable segments (`locos.p_index` when every unit
//! has a distinct one, else the position), turn each segment into LocOS text with `profile::tokens::to_locos_text_for`
//! (pair letter `r` for docx, pptx and xlsx text boxes, whose tokens carry LocOS's numbers, as the adapter does; html
//! keeps LocOS's own tag tokens), then set each mapping as targets and merge with the profile's merge options. Units and bytes must be equal.
//!
//! Zip packages (docx, pptx, xlsx, the XLIFF packages) are compared entry by entry, by name: an entry LocOS left as
//! it was must be byte for byte the input's, a rewritten entry must be the same canonical XML as LocOS's
//! (attributes sorted, empty elements expanded, references resolved, the XML declaration dropped), since zip
//! compression, dates and lxml's serialiser are not what the adapter promises. LocOS's rewritten entries are kept,
//! deflated, in the sidecar `<file>.golden.zip` under their SHA-1.
use bilingua::profile::tokens;
use bilingua::*;
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

struct Numbered {
    p: i64,
    seg: String,
    text: String,
    map: tokens::LocosMap,
    style: String,
}

fn opts(locale: Option<&str>) -> ExtractOptions {
    profile::locos().extract_options().with_granularity(Granularity::Paragraph).with_target_lang(locale.map(|l| l.replace('_', "-")))
}

fn numbered(doc: &Document) -> Vec<Numbered> {
    let segs: Vec<(&Unit, &Segment)> = doc.units.iter().filter(|u| u.translate).flat_map(|u| u.segments.iter().map(move |s| (u, s))).collect();
    let metas: Vec<Option<i64>> = segs.iter().map(|(u, _)| u.context.extra.get(profile::P_INDEX).and_then(|p| p.parse().ok())).collect();
    let distinct = metas.iter().flatten().collect::<BTreeSet<_>>().len() == metas.len();
    let use_meta = !segs.is_empty() && metas.iter().all(Option::is_some) && distinct;
    segs.iter()
        .enumerate()
        .map(|(i, (u, s))| {
            let (text, map) = tokens::to_locos_text_for(&doc.format, &s.source);
            let style = u
                .context
                .extra
                .get(profile::STYLE)
                .filter(|s| !s.is_empty())
                .cloned()
                .or_else(|| u.context.style.clone())
                .unwrap_or_default();
            Numbered { p: if use_meta { metas[i].unwrap() } else { i as i64 }, seg: s.id.clone(), text, map, style }
        })
        .collect()
}

/// The goldens' root: `tests/locos`, or `BILI_LOCOS_ROOT` (a corpus from `oracle/locos_fuzz.py`).
fn root() -> PathBuf {
    match std::env::var_os("BILI_LOCOS_ROOT") {
        Some(r) if !r.is_empty() => PathBuf::from(r),
        _ => Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("locos"),
    }
}

fn goldens() -> Vec<PathBuf> {
    let root = root();
    let mut out = Vec::new();
    for fmt in std::fs::read_dir(&root).unwrap() {
        let fmt = fmt.unwrap().path();
        if !fmt.is_dir() {
            continue;
        }
        for f in std::fs::read_dir(&fmt).unwrap() {
            let f = f.unwrap().path();
            if f.to_string_lossy().ends_with(".golden.json") {
                out.push(f);
            }
        }
    }
    out.sort();
    out
}

/// The non-directory entries of a zip as (name, bytes) in archive order, or None when `data` is not a zip.
fn zip_entries(data: &[u8]) -> Option<Vec<(String, Vec<u8>)>> {
    if !data.starts_with(b"PK") {
        return None;
    }
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(data)).ok()?;
    let mut out = Vec::new();
    for i in 0..z.len() {
        let mut f = z.by_index(i).ok()?;
        if f.is_dir() {
            continue;
        }
        let mut b = Vec::new();
        std::io::Read::read_to_end(&mut f, &mut b).ok()?;
        out.push((f.name().ok()?.to_string(), b));
    }
    Some(out)
}

fn xml_entity(name: &str) -> Option<char> {
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        _ => return None,
    })
}

fn xml_escape(s: &str, attr: bool) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' if attr => o.push_str("&quot;"),
            '\n' if attr => o.push_str("&#10;"),
            '\t' if attr => o.push_str("&#9;"),
            '\r' => o.push_str("&#13;"),
            c => o.push(c),
        }
    }
    o
}

/// Canonical form of an XML document for comparison: no declaration, attributes (namespace declarations included)
/// sorted by name with resolved values, every element written as a start and an end tag, character and entity
/// references and CDATA resolved to text, adjacent text joined, text outside the root dropped; comments and
/// processing instructions kept. None when the bytes are not well-formed UTF-8 XML.
fn canonical_xml(data: &[u8]) -> Option<String> {
    use quick_xml::events::Event;
    let text = std::str::from_utf8(data.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(data)).ok()?;
    let mut r = quick_xml::Reader::from_str(text);
    let mut out = String::new();
    let mut pending = String::new();
    let mut stack: Vec<String> = Vec::new();
    fn flush(out: &mut String, pending: &mut String) {
        if !pending.is_empty() {
            out.push_str(&xml_escape(pending, false));
            pending.clear();
        }
    }
    loop {
        let ev = r.read_event().ok()?;
        match ev {
            Event::Eof => break,
            Event::Decl(_) | Event::DocType(_) => {}
            Event::Start(ref e) | Event::Empty(ref e) => {
                flush(&mut out, &mut pending);
                let name = String::from_utf8(e.name().as_ref().to_vec()).ok()?;
                let mut attrs: Vec<(String, String)> = Vec::new();
                for a in e.attributes() {
                    let a = a.ok()?;
                    let k = String::from_utf8(a.key.as_ref().to_vec()).ok()?;
                    let v = a.unescape_value().ok()?.to_string();
                    attrs.push((k, v));
                }
                attrs.sort();
                out.push('<');
                out.push_str(&name);
                for (k, v) in attrs {
                    out.push_str(&format!(" {k}=\"{}\"", xml_escape(&v, true)));
                }
                out.push('>');
                if matches!(ev, Event::Empty(_)) {
                    out.push_str(&format!("</{name}>"));
                } else {
                    stack.push(name);
                }
            }
            Event::End(_) => {
                flush(&mut out, &mut pending);
                out.push_str(&format!("</{}>", stack.pop()?));
            }
            Event::Text(t) => {
                if !stack.is_empty() {
                    pending.push_str(&t.decode().ok()?);
                }
            }
            Event::CData(t) => pending.push_str(std::str::from_utf8(&t).ok()?),
            Event::GeneralRef(g) => {
                let c = match g.resolve_char_ref().ok()? {
                    Some(c) => c,
                    None => xml_entity(&g.decode().ok()?)?,
                };
                if !stack.is_empty() {
                    pending.push(c);
                }
            }
            Event::Comment(c) => {
                flush(&mut out, &mut pending);
                out.push_str(&format!("<!--{}-->", std::str::from_utf8(&c).ok()?));
            }
            Event::PI(p) => {
                flush(&mut out, &mut pending);
                out.push_str(&format!("<?{}?>", std::str::from_utf8(&p).ok()?));
            }
        }
    }
    flush(&mut out, &mut pending);
    stack.is_empty().then_some(out)
}

/// The first difference between two strings, with some context.
fn first_diff(a: &str, b: &str) -> String {
    let i = a.char_indices().zip(b.chars()).find(|((_, x), y)| x != y).map(|((i, _), _)| i).unwrap_or(a.len().min(b.len()));
    let lo = a[..i].char_indices().rev().nth(80).map(|(j, _)| j).unwrap_or(0);
    let cut = |s: &str| s.chars().take(260).collect::<String>();
    format!("at {i}\n    locos:    {:?}\n    bilingua: {:?}", cut(&a[lo..]), cut(b.get(lo..).unwrap_or("")))
}

/// Compares Bilingua's package with LocOS's entry list.
fn compare_zip(label: &str, input: &[(String, Vec<u8>)], want: &[Value], blobs: &HashMap<String, Vec<u8>>, have: &[u8]) -> Result<(), String> {
    let Some(have) = zip_entries(have) else { return Err(format!("{label}: Bilingua did not write a zip")) };
    let have: HashMap<&str, &Vec<u8>> = have.iter().map(|(n, b)| (n.as_str(), b)).collect();
    let mut errs = Vec::new();
    let names: BTreeSet<&str> = want.iter().map(|e| e["name"].as_str().unwrap()).collect();
    let mut extra: Vec<&&str> = have.keys().filter(|n| !names.contains(**n)).collect();
    extra.sort();
    for n in extra {
        errs.push(format!("{label}: entry {n} only in Bilingua's package"));
    }
    for e in want {
        let name = e["name"].as_str().unwrap();
        let Some(h) = have.get(name) else {
            errs.push(format!("{label}: entry {name} missing from Bilingua's package"));
            continue;
        };
        if e["same"].as_bool() == Some(true) {
            let orig = input.iter().find(|(n, _)| n == name).map(|(_, b)| b);
            if orig != Some(*h) {
                errs.push(format!("{label}: entry {name} changed, LocOS keeps it as it was"));
            }
            continue;
        }
        let w = &blobs[e["sha1"].as_str().unwrap()];
        if w == *h {
            continue;
        }
        match (canonical_xml(w), canonical_xml(h)) {
            (Some(a), Some(b)) if a == b => {}
            (Some(a), Some(b)) => errs.push(format!("{label}: entry {name} differs {}", first_diff(&a, &b))),
            _ => errs.push(format!("{label}: entry {name} bytes differ (not XML)")),
        }
    }
    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs.join("\n"))
    }
}

/// The sidecar's rewritten entries by SHA-1 (empty when there is none).
fn sidecar(golden: &Path) -> HashMap<String, Vec<u8>> {
    let side = PathBuf::from(golden.to_string_lossy().replace(".golden.json", ".golden.zip"));
    std::fs::read(side).ok().and_then(|b| zip_entries(&b)).unwrap_or_default().into_iter().collect()
}

/// Checks one golden; returns the failures as text (empty when it matches).
fn check(golden: &Path) -> (usize, usize, usize, Vec<String>) {
    let g: Value = serde_json::from_str(&std::fs::read_to_string(golden).unwrap()).unwrap();
    let input = PathBuf::from(golden.to_string_lossy().trim_end_matches(".golden.json"));
    let name = input.file_name().unwrap().to_string_lossy().to_string();
    let data = std::fs::read(&input).unwrap();
    let mut fails = Vec::new();
    // LocOS refused the file (fuzz corpus): Bilingua must refuse it too, or say so
    if let Some(e) = g["parse_error"].as_str() {
        return match extract(&name, &data, &opts(None)) {
            Err(_) => (1, 0, 0, Vec::new()),
            // LocOS refuses a file with nothing to translate; a document without units says the same
            Ok(d) if e.starts_with("EmptyDocumentError") && numbered(&d).is_empty() => (1, 0, 0, Vec::new()),
            Ok(d) => (0, 0, 0, vec![format!("{name}: LocOS refuses ({e}), Bilingua extracts {} units", numbered(&d).len())]),
        };
    }
    let doc = match extract(&name, &data, &opts(None)) {
        Ok(d) => d,
        Err(e) => return (0, 0, 0, vec![format!("{name}: extract failed: {e}")]),
    };
    let have: Vec<(i64, String, String)> = numbered(&doc).into_iter().map(|n| (n.p, n.text, n.style)).collect();
    let want: Vec<(i64, String, String)> = g["units"]
        .as_array()
        .unwrap()
        .iter()
        .map(|u| (u["p_index"].as_i64().unwrap(), u["text"].as_str().unwrap().to_string(), u["style"].as_str().unwrap().to_string()))
        .collect();
    let units_ok = have == want;
    if !units_ok {
        fails.push(format!("{name}: units differ\n  locos:    {want:?}\n  bilingua: {have:?}"));
    }
    let (mut builds, mut built) = (0, 0);
    let input_zip = zip_entries(&data).unwrap_or_default();
    let blobs = sidecar(golden);
    for b in g["builds"].as_array().unwrap() {
        let want_zip = b["zip"].as_array();
        let want = match (b["out_b64"].as_str(), want_zip) {
            (Some(out), _) => b64::decode(out).unwrap(),
            (None, Some(_)) => Vec::new(),
            (None, None) => continue,
        };
        builds += 1;
        let locale = b["locale"].as_str();
        let label = format!("{name} build {}", b["name"].as_str().unwrap());
        let map = b["map"].as_object().unwrap();
        // LocOS's B-bytes guard (base.py `_byte_guard`) wraps every filter class, the adapter's included: a plain
        // build that writes nothing returns the upload without calling the parser
        // ... and so does one whose every text is its unit's own (the identity mapping), in a monolingual format
        let units = g["units"].as_array().unwrap();
        let identity = !map.is_empty()
            && g["format"].as_str() != Some("po")
            && map.iter().all(|(k, v)| units.iter().any(|u| u["p_index"].as_i64().map(|p| p.to_string()).as_deref() == Some(k.as_str()) && u["text"] == *v));
        if locale.is_none() && (map.is_empty() || identity) {
            let untouched = match want_zip {
                Some(z) => z.len() == input_zip.len() && z.iter().all(|e| e["same"].as_bool() == Some(true)),
                None => want == data,
            };
            if untouched {
                built += 1;
                continue;
            }
            // a filter without the guard (`passthrough_untouched = False`, the Trados return package) builds anyway
        }
        let mut doc = match extract(&name, &data, &opts(locale)) {
            Ok(d) => d,
            Err(e) => {
                fails.push(format!("{label}: extract failed: {e}"));
                continue;
            }
        };
        let mut set_fail = None;
        for n in numbered(&doc) {
            if let Some(t) = map.get(&n.p.to_string()).and_then(Value::as_str) {
                let inl = tokens::from_locos_text(t, &n.map);
                if let Err(e) = profile::locos_set_target(&mut doc, &n.seg, inl) {
                    set_fail = Some(format!("{label}: set_target {}: {e}", n.p));
                }
            }
        }
        if let Some(f) = set_fail {
            fails.push(f);
            continue;
        }
        let merged = merge_with(&doc, &profile::locos().merge_options(&doc.format));
        match (merged, want_zip) {
            (Ok(have), Some(z)) => match compare_zip(&label, &input_zip, z, &blobs, &have) {
                Ok(()) => built += 1,
                Err(e) => fails.push(e),
            },
            (Ok(have), None) if have == want => built += 1,
            (Ok(have), None) => fails.push(format!(
                "{label}: bytes differ\n  locos:    {:?}\n  bilingua: {:?}",
                String::from_utf8_lossy(&want),
                String::from_utf8_lossy(&have)
            )),
            (Err(e), _) => fails.push(format!("{label}: merge failed: {e}")),
        }
    }
    (units_ok as usize, builds, built, fails)
}

/// Goldens that still differ, with the reason; their failures are printed, not fatal. Remove a line when its
/// file matches.
const KNOWN: &[(&str, &str)] = &[];

/// Formats whose goldens do not match yet: reported, not fatal. Remove a format when all its files match. Empty:
/// every Track D format matches.
const PENDING: &[&str] = &[];

#[test]
fn locos_profile_goldens() {
    let mut all: Vec<String> = Vec::new();
    let mut summary = std::collections::BTreeMap::<String, [usize; 4]>::new();
    for g in goldens() {
        let fmt = g.parent().unwrap().file_name().unwrap().to_string_lossy().to_string();
        let (u, b, ok, f) = check(&g);
        let file = g.file_name().unwrap().to_string_lossy().trim_end_matches(".golden.json").to_string();
        if let Some((_, why)) = KNOWN.iter().find(|(k, _)| *k == file) {
            println!("LOCOS-GOLDEN known difference {file} ({why}): units {u}/1 builds {ok}/{b}");
            for x in &f {
                println!("  {x}");
            }
            continue;
        }
        let s = summary.entry(fmt.clone()).or_default();
        s[0] += u;
        s[1] += 1;
        s[2] += ok;
        s[3] += b;
        if PENDING.contains(&fmt.as_str()) {
            for x in &f {
                println!("LOCOS-PENDING {x}");
            }
        } else {
            all.extend(f);
        }
    }
    for (fmt, s) in &summary {
        println!("LOCOS-GOLDEN {fmt}: units {}/{} builds {}/{}", s[0], s[1], s[2], s[3]);
    }
    assert!(!summary.is_empty(), "no goldens found");
    assert!(all.is_empty(), "{} LocOS golden failures:\n{}", all.len(), all.join("\n"));
}

#[test]
fn canonical_xml_ignores_serialiser_choices() {
    let a = canonical_xml(b"<?xml version='1.0'?>\n<a y='1' x=\"&lt;\"><b/>t&amp;u<![CDATA[<v>]]></a>").unwrap();
    let b = canonical_xml(b"<a x=\"&#60;\" y=\"1\"><b></b>t&#38;u&lt;v&gt;</a>\n").unwrap();
    assert_eq!(a, b);
    assert_ne!(canonical_xml(b"<a>x</a>"), canonical_xml(b"<a>x </a>"));
    assert_ne!(canonical_xml(b"<a x='1'/>"), canonical_xml(b"<a x='2'/>"));
    assert!(canonical_xml(b"<a><b></a>").is_none());
}

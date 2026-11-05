//! The model contract every format must keep (D27): paragraph mode, stable unit ids, a parser stamp in the
//! skeleton, and a one-to-one mapping of inline codes onto Localization OS text tokens. All checks run over the
//! made-up corpus in `oracle/corpus`, one or more files per format.

use bilingua::*;
use bilingua_model::locos::{from_locos_text, to_locos_text, to_locos_text_as};
use std::path::PathBuf;

fn corpus() -> Vec<(String, Vec<u8>)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../oracle/corpus");
    let mut out = Vec::new();
    for dir in std::fs::read_dir(&root).unwrap().flatten() {
        if !dir.path().is_dir() {
            continue;
        }
        for f in std::fs::read_dir(dir.path()).unwrap().flatten() {
            let name = f.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue; // dot files (.gitattributes and the like) are never fixtures
            }
            out.push((name, std::fs::read(f.path()).unwrap()));
        }
    }
    out.sort();
    assert!(out.len() >= 30, "corpus went missing: {} files", out.len());
    out
}

fn opts(mode: Granularity) -> ExtractOptions {
    ExtractOptions { source_lang: Some("en".into()), target_lang: Some("es-ES".into()), granularity: mode, include_locked: true, limits: None }
}

fn open(name: &str, bytes: &[u8], mode: Granularity) -> Document {
    extract(name, bytes, &opts(mode)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn is_zip(name: &str) -> bool {
    name.ends_with(".docx") || name.ends_with(".xlsx") || name.ends_with(".pptx") || name.ends_with(".xcloc")
}

#[test]
fn paragraph_mode_is_one_segment_per_unit_on_every_format() {
    for (name, bytes) in corpus() {
        let sent = open(&name, &bytes, Granularity::Sentence);
        let para = open(&name, &bytes, Granularity::Paragraph);
        assert_eq!(para.granularity, Granularity::Paragraph, "{name}");
        // Paragraph mode never splits, and never changes which units exist or their ids.
        let ids = |d: &Document| d.units.iter().map(|u| u.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&sent), ids(&para), "{name}: unit ids differ between modes");
        for u in &para.units {
            assert_eq!(u.segments.len(), 1, "{name}: unit {} has {} segments", u.id, u.segments.len());
            assert_eq!(u.segments[0].id, u.id, "{name}: segment id must equal unit id in paragraph mode");
            assert!(u.separators.is_empty(), "{name}: unit {} has separators", u.id);
        }
        // The one segment holds the whole unit text, so it equals the sentence-mode unit source.
        for (a, b) in sent.units.iter().zip(&para.units) {
            assert_eq!(a.source(), b.source(), "{name}: unit {} text differs between modes", a.id);
        }
        // An untouched paragraph document still merges to the original bytes (zip containers may re-compress).
        if !is_zip(&name) {
            assert_eq!(merge(&para).unwrap(), bytes, "{name}: paragraph merge changed the file");
        } else {
            assert!(merge(&para).is_ok(), "{name}");
        }
    }
}

#[test]
fn paragraph_mode_after_sentence_mode_rejoins() {
    for (name, bytes) in corpus() {
        let mut doc = open(&name, &bytes, Granularity::Sentence);
        resegment(&mut doc, "en", Granularity::Paragraph);
        assert!(doc.units.iter().all(|u| u.segments.len() == 1), "{name}");
    }
}

#[test]
fn every_tag_maps_to_exactly_one_locos_token_and_back() {
    let mut tags_seen = 0usize;
    let mut free_seen = 0usize;
    for (name, bytes) in corpus() {
        for mode in [Granularity::Paragraph, Granularity::Sentence] {
            let doc = open(&name, &bytes, mode);
            for (_, s) in doc.segments() {
                for side in [&s.source, &s.target] {
                    let want = merge_text(side.clone());
                    let n_tags = want.iter().filter(|i| matches!(i, Inline::Tag { .. })).count();
                    for letter in ['g', 'r'] {
                        let (text, map) = to_locos_text_as(&want, letter);
                        // Each tag is one token: count the brackets in the text.
                        assert_eq!(text.matches('\u{27E6}').count(), n_tags, "{name} {}: {text:?}", s.id);
                        assert_eq!(text.matches('\u{27E7}').count(), n_tags, "{name} {}", s.id);
                        assert_eq!(from_locos_text(&text, &map), want, "{name} {} did not round trip", s.id);
                        assert!(locos_well_formed(&text), "{name} {}: LocOS would call {text:?} malformed", s.id);
                        // The map survives JSON, as LocOS stores it.
                        let json = serde_json::to_string(&map).unwrap();
                        let back: bilingua_model::locos::LocosMap = serde_json::from_str(&json).unwrap();
                        assert_eq!(back, map);
                    }
                    tags_seen += n_tags;
                    let n_free = want.iter().filter(|i| i.is_free()).count();
                    free_seen += n_free;
                    // Free codes are standalone and use the p letter; nothing else does.
                    let (text, _) = to_locos_text(&want);
                    assert_eq!(text.matches("\u{27E6}p").count(), n_free, "{name}");
                }
            }
        }
    }
    assert!(tags_seen > 100, "the corpus should exercise many tags, saw {tags_seen}");
    assert!(free_seen > 0, "no free placeholder in the corpus");
}

/// LocOS `placeholders_well_formed`: paired tokens nest by number; `x` and `p` tokens never pair.
fn locos_well_formed(t: &str) -> bool {
    let mut stack = Vec::new();
    for tok in t.split('\u{27E6}').skip(1) {
        let tok = tok.split('\u{27E7}').next().unwrap_or_default();
        if let Some(rest) = tok.strip_prefix('/') {
            if stack.pop().as_deref() != Some(rest.trim_start_matches(|c: char| c.is_ascii_lowercase())) {
                return false;
            }
        } else if !tok.starts_with(['x', 'p']) {
            stack.push(tok.trim_start_matches(|c: char| c.is_ascii_lowercase()).to_string());
        }
    }
    stack.is_empty()
}

fn tags_of(d: &Document) -> Vec<(String, bool)> {
    d.units[0]
        .source()
        .iter()
        .filter_map(|i| match i {
            Inline::Tag { display, free, .. } => Some((display.clone(), *free)),
            _ => None,
        })
        .collect()
}

#[test]
fn free_flag_marks_named_placeholders_only() {
    let po = b"msgid \"\"\nmsgstr \"\"\n\"Language: es\\n\"\n\nmsgid \"Hi %1$s, you have %d items and {name} ({0}) %s\"\nmsgstr \"\"\n";
    let d = open("a.po", po, Granularity::Paragraph);
    let want: Vec<(String, bool)> =
        [("%1$s", true), ("%d", false), ("{name}", true), ("{0}", true), ("%s", false)].iter().map(|(a, b)| (a.to_string(), *b)).collect();
    assert_eq!(tags_of(&d), want);
    let j = open("a.json", br#"{"a": "Hola {name}, <b>x</b> %s %1$s"}"#, Granularity::Paragraph);
    let want: Vec<(String, bool)> = [("{name}", true), ("<b>", false), ("</b>", false), ("%s", false), ("%1$s", true)]
        .iter()
        .map(|(a, b)| (a.to_string(), *b))
        .collect();
    assert_eq!(tags_of(&j), want);
}

fn crate_of(format: &str) -> &'static str {
    match format {
        "docx" => "bilingua-docx",
        "xlsx" => "bilingua-xlsx",
        "pptx" => "bilingua-pptx",
        "idml" => "bilingua-idml",
        "html" => "bilingua-html",
        "json" => "bilingua-json",
        "resjson" => "bilingua-resjson",
        "wxl" => "bilingua-wxl",
        "json5" => "bilingua-json5",
        "yaml" | "yml" => "bilingua-yaml",
        "markdown" | "md" => "bilingua-markdown",
        "po" => "bilingua-po",
        "ini" => "bilingua-ini",
        "dtd" => "bilingua-dtd",
        "ftl" => "bilingua-ftl",
        "rc" => "bilingua-rc",
        "strings" => "bilingua-strings",
        "plist" | "stringsdict" => "bilingua-plist",
        "toml" => "bilingua-toml",
        "properties" => "bilingua-properties",
        "resx" => "bilingua-resx",
        "xcstrings" | "xcloc" => "bilingua-xcstrings",
        "js" => "bilingua-jsmodule",
        "php" => "bilingua-php",
        "arb" => "bilingua-arb",
        "xaml" => "bilingua-xaml",
        "xmb" | "xtb" => "bilingua-xmb",
        "ts" => "bilingua-qtts",
        "xml" => "bilingua-xml",
        "srt" | "vtt" => "bilingua-subtitle",
        "txt" | "text" => "bilingua-text",
        "xliff" | "sdlxliff" | "mqxliff" | "mxliff" => "bilingua-xliff",
        "sdlppx" | "mqxlz" => "bilingua-sdl",
        other => panic!("unknown format id {other}"),
    }
}

fn with_parser(d: &mut Document, f: impl FnOnce(&mut Option<ParserInfo>)) {
    match &mut d.skeleton {
        Skeleton::Text { parser, .. } | Skeleton::Zip { parser, .. } => f(parser),
    }
}

#[test]
fn every_skeleton_records_its_parser_and_merge_checks_it() {
    let mut formats = std::collections::BTreeSet::new();
    for (name, bytes) in corpus() {
        let doc = open(&name, &bytes, Granularity::Paragraph);
        let p = doc.skeleton.parser().unwrap_or_else(|| panic!("{name}: skeleton has no parser record")).clone();
        assert_eq!(p.name, crate_of(&doc.format), "{name}");
        assert_eq!(p.version, env!("CARGO_PKG_VERSION"), "{name}");
        assert!(p.schema >= 1, "{name}: schema must be a positive number");
        formats.insert(doc.format.clone());

        // A newer schema than this crate knows is refused with a clear error.
        let mut newer = doc.clone();
        with_parser(&mut newer, |q| q.as_mut().unwrap().schema = p.schema + 1);
        let err = merge(&newer).expect_err(&format!("{name}: merge accepted a newer schema"));
        assert!(matches!(err, Error::ParserVersion { .. }), "{name}: {err:?}");
        let msg = err.to_string();
        assert!(msg.contains(&format!("schema {}", p.schema + 1)) && msg.contains(p.name.as_str()), "{msg}");

        // Another parser's skeleton is refused too.
        let mut foreign = doc.clone();
        with_parser(&mut foreign, |q| q.as_mut().unwrap().name = "bilingua-other".into());
        assert!(matches!(merge(&foreign), Err(Error::ParserVersion { .. })), "{name}");

        // A new crate release with the same schema merges, whatever its version number says.
        for version in ["0.2.0", "0.1.77", "1.0.0", "9.9.9"] {
            let mut bumped = doc.clone();
            with_parser(&mut bumped, |q| q.as_mut().unwrap().version = version.into());
            assert!(merge(&bumped).is_ok(), "{name}: version {version} with the same schema refused");
        }
        // An older schema merges (the crate still reads it); schema 1 is what every stamp without a schema means.
        let mut older = doc.clone();
        with_parser(&mut older, |q| q.as_mut().unwrap().schema = 1);
        if p.schema > 1 {
            assert!(merge(&older).is_ok(), "{name}: older schema refused");
        }
        // A skeleton from before stamps existed merges.
        let mut legacy = doc.clone();
        with_parser(&mut legacy, |q| *q = None);
        assert!(merge(&legacy).is_ok(), "{name}: unstamped skeleton refused");

        // The stamp is in the JSON, and the document reads back equal.
        let json = serde_json::to_value(&doc).unwrap();
        let skel = json["skeleton"].as_object().unwrap().values().next().unwrap();
        assert_eq!(skel["parser"]["name"], p.name.as_str(), "{name}");
        assert_eq!(skel["parser"]["schema"], p.schema, "{name}");
        let back: Document = serde_json::from_value(json).unwrap();
        assert_eq!(back, doc, "{name}: JSON round trip");
    }
    for f in ["docx", "xlsx", "pptx", "html", "json", "json5", "po", "srt", "vtt"] {
        assert!(formats.contains(f), "no corpus file for {f}: {formats:?}");
    }
}

#[test]
fn stamps_written_before_schemas_still_read_as_schema_1() {
    // The stamp as released: crate name and version only.
    let old = r#"{"name":"bilingua-text","version":"0.1.0"}"#;
    let p: ParserInfo = serde_json::from_str(old).unwrap();
    assert_eq!(p.schema, 1);
    let mut d = open("a.txt", b"One. Two.
", Granularity::Sentence);
    with_parser(&mut d, |q| *q = Some(p));
    assert_eq!(merge(&d).unwrap(), b"One. Two.
");
    // A whole old document, skeleton included, reads and merges.
    let mut v = serde_json::to_value(open("a.txt", b"Hi.
", Granularity::Sentence)).unwrap();
    let parser = v["skeleton"].as_object_mut().unwrap().values_mut().next().unwrap()["parser"].as_object_mut().unwrap();
    parser.remove("schema");
    parser.insert("version".into(), "0.1.0".into());
    let back: Document = serde_json::from_value(v).unwrap();
    assert_eq!(merge(&back).unwrap(), b"Hi.
");
}

#[test]
fn old_json_without_new_fields_still_reads() {
    let old = r#"{"id":"u1","segments":[{"id":"u1","source":[{"text":"Hi"},{"tag":{"id":"1","kind":"open","display":"<b>"}}],"target":[],"state":"draft"}],"separators":[],"codes":{},"context":{},"translate":true}"#;
    let u: Unit = serde_json::from_str(old).unwrap();
    assert!(u.notes.is_empty() && u.lock_reason.is_none() && u.segments[0].origin.is_none());
    assert!(!u.segments[0].source[1].is_free());
    // And nothing new appears in the JSON when it is empty.
    let v = serde_json::to_value(&u).unwrap();
    assert!(v.get("notes").is_none() && v.get("lock_reason").is_none());
    assert!(v["segments"][0].get("origin").is_none());
    assert!(v["segments"][0]["source"][1]["tag"].get("free").is_none());
}

#[test]
fn bilingual_fields_round_trip_in_json() {
    let mut u = Unit::new("u1", vec![Inline::text("Hola"), Inline::free_tag("2", "{n}")]);
    u.notes.push(Note { author: Some("Ana".into()), text: "\u{6ce8}\u{91c8}".into(), kind: Some("comment".into()) });
    u.lock_reason = Some("context match".into());
    u.translate = false;
    u.segments[0].target = vec![Inline::text("\u{645}\u{631}\u{62d}\u{628}\u{627}")];
    u.segments[0].state = SegState::SignedOff;
    u.segments[0].origin = Some(Origin::new(OriginKind::Tm).with_name("Acme TM").with_match(94));
    let j = serde_json::to_value(&u).unwrap();
    assert_eq!(j["segments"][0]["state"], "signed-off");
    assert_eq!(j["segments"][0]["origin"]["kind"], "tm");
    assert_eq!(j["segments"][0]["origin"]["match_pct"], 94);
    assert_eq!(j["notes"][0]["author"], "Ana");
    assert_eq!(j["lock_reason"], "context match");
    assert_eq!(j["segments"][0]["source"][1]["tag"]["free"], true);
    let back: Unit = serde_json::from_value(j).unwrap();
    assert_eq!(back, u);
    for s in [
        SegState::New,
        SegState::Draft,
        SegState::Translated,
        SegState::Confirmed,
        SegState::Reviewed,
        SegState::SignedOff,
        SegState::Rejected,
        SegState::Locked,
    ] {
        assert_eq!(SegState::from_rank(s.rank()), s);
        let t = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<SegState>(&t).unwrap(), s);
    }
    let o: Origin = serde_json::from_str(r#"{"kind":"carrier-pigeon"}"#).unwrap();
    assert_eq!(o.kind, OriginKind::Other);
}

/// FNV-1a over `id:source text` of every unit, so a change in which id goes with which text shows up too.
fn fingerprint(d: &Document) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for u in &d.units {
        for b in format!("{}:{}\n", u.id, plain_text(&u.source())).bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    format!("{h:016x}")
}

/// Snapshot of one corpus file per format: unit count, first and last ids and a fingerprint of id + text.
/// A change here means existing projects would lose their segment-to-unit links: do not update it casually.
#[test]
fn unit_ids_are_pinned_per_format() {
    let snapshots: &[(&str, &str)] = &[
        ("edge.docx", "18 p1 footnotes-p1 02aae0df09ac5bb9"),
        ("edge.html", "33 u1 u33 bf3041ad14ff0e2c"),
        ("strings.json", "20 u1 u20 54c1ad5a27488507"),
        ("edge.md", "27 u1 u27 8b3892ac25e501c8"),
        ("edge.po", "9 e2.0 e9 ad2c6e2b97586bd6"),
        ("launch.pptx", "21 slide1-p1 slideLayout1-p2 3deb63f836cce400"),
        ("edge.srt", "7 c1 c7 07f8bd29564c9f18"),
        ("lines.txt", "2 p1 p2 8d4f2fbdc5107536"),
        ("edge.vtt", "4 c1 c4 15fa4b87a4e36541"),
        ("menu.xlsx", "20 ss0 is1-A6 55fe64e61bbfd53d"),
    ];
    let all = corpus();
    let mut bad = Vec::new();
    for (file, want) in snapshots {
        let (name, bytes) = all.iter().find(|(n, _)| n == file).unwrap_or_else(|| panic!("{file} missing"));
        // Same ids in sentence mode, where a unit keeps its id and its segments get #n.
        let sent = open(name, bytes, Granularity::Sentence);
        let d = open(name, bytes, Granularity::Paragraph);
        assert_eq!(fingerprint(&sent), fingerprint(&d), "{file}: modes disagree");
        let ids: Vec<&str> = d.units.iter().map(|u| u.id.as_str()).collect();
        let got = format!("{} {} {} {}", ids.len(), ids.first().unwrap(), ids.last().unwrap(), fingerprint(&d));
        if got != *want {
            bad.push(format!("(\"{file}\", \"{got}\"),"));
        }
    }
    assert!(bad.is_empty(), "snapshot differs:\n{}", bad.join("\n"));
}

#[test]
fn ids_are_unique_and_do_not_depend_on_the_run() {
    for (name, bytes) in corpus() {
        let a = open(&name, &bytes, Granularity::Sentence);
        let b = open(&name, &bytes, Granularity::Sentence);
        assert_eq!(a, b, "{name}: two extractions differ");
        let mut seen = std::collections::HashSet::new();
        for u in &a.units {
            assert!(seen.insert(u.id.clone()), "{name}: duplicate unit id {}", u.id);
        }
        let mut segs = std::collections::HashSet::new();
        for (_, s) in a.segments() {
            assert!(segs.insert(s.id.clone()), "{name}: duplicate segment id {}", s.id);
        }
    }
}

fn raw_entries(v: &mut serde_json::Value) -> Vec<&mut serde_json::Value> {
    let mut out = Vec::new();
    if let Some(entries) = v["skeleton"]["zip"]["entries"].as_array_mut() {
        for e in entries {
            if e["data"].get("raw").is_some() {
                out.push(&mut e["data"]["raw"]);
            }
        }
    }
    out
}

#[test]
fn raw_zip_bytes_are_base64_and_the_old_array_form_still_merges() {
    for (name, bytes) in corpus().into_iter().filter(|(n, _)| is_zip(n)) {
        let doc = open(&name, &bytes, Granularity::Paragraph);
        let expected = merge(&doc).unwrap();
        let mut json = serde_json::to_value(&doc).unwrap();
        let mut n = 0;
        for raw in raw_entries(&mut json) {
            assert!(raw.is_string(), "{name}: raw bytes should be a base64 string");
            n += 1;
        }
        assert!(n > 0, "{name}: no raw entries");
        // New form round trips.
        let back: Document = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(back, doc, "{name}");
        assert_eq!(merge(&back).unwrap(), expected, "{name}");
        // Old form: arrays of integers.
        for raw in raw_entries(&mut json) {
            let b = bilingua_model::b64::decode(raw.as_str().unwrap()).unwrap();
            *raw = serde_json::Value::Array(b.into_iter().map(serde_json::Value::from).collect());
        }
        let old: Document = serde_json::from_value(json).unwrap();
        assert_eq!(old, doc, "{name}: old array form differs");
        assert_eq!(merge(&old).unwrap(), expected, "{name}: old skeleton does not merge");
    }
}

//! Regression tests for the shared guard fixes of the 0.9.1 security audit: the size gate runs before detection,
//! the context budget bounds per-unit copies, and a target language that is not a language code is never stamped.

use bilingua::*;
use bilingua_model::{LimitKind, Limits};

fn opts() -> ExtractOptions {
    ExtractOptions::new().with_source_lang("en")
}

fn limit_kind(r: Result<Document, Error>) -> LimitKind {
    match r {
        Err(Error::Limit(e)) => e.kind,
        Err(e) => panic!("expected a limit error, got {e}"),
        Ok(_) => panic!("expected a limit error, the file was accepted"),
    }
}

/// An over-size file is refused for its size whatever its name: with no extension (every sniffer would read it),
/// with `.json` (the ARB sniffer would), and with an extension no parser knows.
#[test]
fn the_size_gate_runs_before_detection() {
    let small = Limits::new().with_max_input_bytes(1024);
    let xml = format!("<r>{}</r>", "<a/>".repeat(2000));
    let json = format!("{{\"@@locale\":\"en\"{}}}", ",\"a\":\"b\"".repeat(2000));
    for (name, body) in [("x", &xml), ("x", &json), ("x.json", &json), ("x.nosuchformat", &xml)] {
        let r = extract(name, body.as_bytes(), &opts().with_limits(small));
        assert_eq!(limit_kind(r), LimitKind::InputBytes, "{name}");
    }
}

/// One long key over many values: the copies of the key path count against `max_context_bytes`.
#[test]
fn context_copies_are_budgeted() {
    let key = "k".repeat(2000);
    let yaml = format!("{key}: [{}]\n", "x, ".repeat(500));
    let json = format!("{{\"{key}\": [{}\"x\"]}}", "\"x\", ".repeat(499));
    let ini = format!("[{key}]\n{}", (0..500).map(|i| format!("k{i}=word\n")).collect::<String>());
    let tight = Limits::new().with_max_context_bytes(100_000);
    for (name, body) in [("f.yaml", &yaml), ("f.json", &json), ("f.ini", &ini)] {
        // about 1 MB of context from a file of a few KB: over a 100 KB budget, inside the default
        assert_eq!(limit_kind(extract(name, body.as_bytes(), &opts().with_limits(tight))), LimitKind::Context, "{name}");
        let doc = extract(name, body.as_bytes(), &opts()).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(doc.units.len(), 500, "{name}");
    }
}

fn translate_first(doc: &mut Document, text: &str) {
    let id = doc.units.iter().flat_map(|u| u.segments.iter()).next().expect("a segment").id.clone();
    doc.set_target(&id, vec![Inline::text(text)]).unwrap();
}

/// A target language holding a line break, a quote or a bracket is not a language code: the merged file is the
/// same as with no target language at all, in the three formats that write the code into a line of the file.
#[test]
fn a_hostile_target_language_is_never_stamped() {
    let desktop = "[Desktop Entry]\nType=Application\nName=Editor\nExec=/usr/bin/editor %f\nComment=Edit text\n";
    let po = "msgid \"\"\nmsgstr \"\"\n\"Language: \\n\"\n\"Content-Type: text/plain; charset=UTF-8\\n\"\n\nmsgid \"Hello\"\nmsgstr \"\"\n\nmsgid \"World\"\nmsgstr \"\"\n";
    let stf = "Language code: en_US\nType: Source\n\n# KEY\tLABEL\n\nCustomLabel.a\tHello\nCustomLabel.b\tWorld\n";
    let hostile = [
        "de_@x]=1\nExec=/bin/true\nComment[zz",
        "de\\n\"\n\"X-Injected: 1\\n\"\n\nmsgid \"Injected\"\nmsgstr \"x\"\n\nmsgid \"y\"\nmsgstr \"",
        "de\ncustomlabel.injected\tinjected label\tinjected translation",
        "de\tx",
        "de DE",
    ];
    for (name, body) in [("a.desktop", desktop), ("a.po", po), ("a.stf", stf)] {
        let mut plain = extract(name, body.as_bytes(), &opts()).unwrap_or_else(|e| panic!("{name}: {e}"));
        plain.target_lang = None;
        translate_first(&mut plain, "Uebersetzt");
        let want = merge(&plain).unwrap_or_else(|e| panic!("{name}: {e}"));
        for lang in hostile {
            let mut doc = plain.clone();
            doc.target_lang = Some(lang.to_string());
            let got = merge(&doc).unwrap_or_else(|e| panic!("{name} {lang:?}: {e}"));
            assert_eq!(String::from_utf8_lossy(&got), String::from_utf8_lossy(&want), "{name} with target language {lang:?}");
        }
        // a real language code is still stamped
        let mut de = plain.clone();
        de.target_lang = Some("de-DE".into());
        let stamped = merge(&de).unwrap();
        if name != "a.stf" {
            assert_ne!(stamped, want, "{name}: de-DE is stamped");
        }
    }
}

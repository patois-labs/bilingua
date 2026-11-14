use std::path::{Path, PathBuf};
use std::process::Command;

fn run(args: &[&str]) -> Result<String, String> {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    let mut out: Vec<u8> = Vec::new();
    bilingua_cli::run(&args, &mut out)?;
    Ok(String::from_utf8(out).unwrap())
}

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("bilingua-cli-test-{}-{}", std::process::id(), name));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn p(d: &Path, f: &str) -> String {
    d.join(f).to_string_lossy().to_string()
}

const TXT: &str = "Welcome to Larchmoor. The tide is out.\n\nSecond paragraph.\r\n";
const JSON: &str = "{\"a\": \"Click <b>Save</b> for {name}. Then %d done.\", \"url\": \"https://x.test\", \"ja\": \"\u{4eca}\u{65e5}\u{306f}\u{6674}\u{308c}\u{3002}\u{660e}\u{65e5}\u{306f}\u{96e8}\u{3002}\"}";
const MD: &str = "---\ntitle: Lamp guide\n---\n\n# Harbor lamp\n\nFirst sentence. **Second** sentence & more. See [docs](https://x.test \"Docs\").\n\n| A | B |\n|---|---|\n| one | two |\n";

#[test]
fn extract_then_merge_is_identity_for_every_format_and_mode() {
    let d = tmp("identity");
    for (file, src) in [("a.txt", TXT), ("a.json", JSON), ("a.md", MD)] {
        std::fs::write(d.join(file), src).unwrap();
        for mode in ["sentence", "paragraph", "document"] {
            let xlf = p(&d, &format!("{file}.{mode}.xlf"));
            let msg = run(&["extract", &p(&d, file), "--target", "es-ES", "--mode", mode, "-o", &xlf]).unwrap();
            assert!(msg.contains("segments"), "{msg}");
            assert!(Path::new(&format!("{xlf}.skl.json")).exists());
            let outp = p(&d, &format!("out-{mode}-{file}"));
            run(&["merge", &xlf, "-o", &outp]).unwrap();
            assert_eq!(std::fs::read(&outp).unwrap(), src.as_bytes(), "{file} {mode}");
        }
    }
}

#[test]
fn xliff_is_valid_xliff_21_with_segments_and_codes() {
    let d = tmp("xliff");
    std::fs::write(d.join("a.json"), JSON).unwrap();
    let xlf = p(&d, "a.xlf");
    run(&["extract", &p(&d, "a.json"), "--source", "en", "--target", "fr-FR", "-o", &xlf]).unwrap();
    let x = std::fs::read_to_string(&xlf).unwrap();
    assert!(x.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    assert!(x.contains("version=\"2.1\" srcLang=\"en\" trgLang=\"fr-FR\""));
    assert!(x.contains("<unit id=\"u1\" name=\"/a\">"));
    assert!(x.contains("<segment id=\"u1#1\" state=\"initial\">"));
    assert!(x.contains("<pc id=\"1\" dispStart=\"&lt;b&gt;\" dispEnd=\"&lt;/b&gt;\">Save</pc>"), "{x}");
    assert!(x.contains("<ph id=\"2\" disp=\"{name}\"/>"));
    assert!(x.contains("<ph id=\"3\" disp=\"%d\"/>"));
    assert!(x.contains("<ignorable>"));
    assert!(!x.contains("x.test"), "the skipped URL stays out of the XLIFF");
    // it is well-formed XML
    let mut r = quick_xml::Reader::from_str(&x);
    while r.read_event().unwrap() != quick_xml::events::Event::Eof {}
}

#[test]
fn translate_with_codes_and_merge() {
    let d = tmp("translate");
    std::fs::write(d.join("a.json"), JSON).unwrap();
    let xlf = p(&d, "a.xlf");
    run(&["extract", &p(&d, "a.json"), "--target", "es-ES", "-o", &xlf]).unwrap();
    let x = std::fs::read_to_string(&xlf).unwrap();
    let src1 = "<source xml:space=\"preserve\">Click <pc id=\"1\" dispStart=\"&lt;b&gt;\" dispEnd=\"&lt;/b&gt;\">Save</pc> for <ph id=\"2\" disp=\"{name}\"/>.</source>\n";
    let src2 = "<source xml:space=\"preserve\">Then <ph id=\"3\" disp=\"%d\"/> done.</source>\n";
    let ja = "<source xml:space=\"preserve\">\u{4eca}\u{65e5}\u{306f}\u{6674}\u{308c}\u{3002}\u{660e}\u{65e5}\u{306f}\u{96e8}\u{3002}</source>\n";
    for s in [src1, src2, ja] {
        assert!(x.contains(s), "missing {s} in\n{x}");
    }
    let t = x
        .replace(src1, &format!("{src1}        <target xml:space=\"preserve\">Pulse <pc id=\"1\">Guardar</pc> para <ph id=\"2\"/> &amp; &lt;listo&gt;.</target>\n"))
        .replace(src2, &format!("{src2}        <target>Luego <ph id=\"3\"/> hecho.</target>\n"))
        .replace(ja, &format!("{ja}        <target>\u{4eca}\u{65e5}\u{306f}\u{6674}\u{308c}\u{3067}\u{3059}\u{3002}\u{660e}\u{65e5}\u{306f}\u{96e8}\u{3067}\u{3059}\u{3002}</target>\n"));
    std::fs::write(&xlf, t).unwrap();
    let outp = p(&d, "out.json");
    let msg = run(&["merge", &xlf, "-o", &outp]).unwrap();
    assert!(msg.starts_with("3 targets merged"), "{msg}");
    let out = std::fs::read_to_string(&outp).unwrap();
    assert_eq!(
        out,
        "{\"a\": \"Pulse <b>Guardar</b> para {name} & <listo>. Luego %d hecho.\", \"url\": \"https://x.test\", \"ja\": \"\u{4eca}\u{65e5}\u{306f}\u{6674}\u{308c}\u{3067}\u{3059}\u{3002}\u{660e}\u{65e5}\u{306f}\u{96e8}\u{3067}\u{3059}\u{3002}\"}"
    );
}

#[test]
fn merge_rejects_bad_targets() {
    let d = tmp("badtarget");
    std::fs::write(d.join("a.json"), JSON).unwrap();
    let xlf = p(&d, "a.xlf");
    run(&["extract", &p(&d, "a.json"), "-o", &xlf]).unwrap();
    let x = std::fs::read_to_string(&xlf).unwrap();
    // a code the source does not have
    let bad = x.replacen("</source>\n      </segment>", "</source>\n<target>x <ph id=\"99\"/></target>\n      </segment>", 1);
    std::fs::write(&xlf, bad).unwrap();
    let e = run(&["merge", &xlf, "-o", &p(&d, "o.json")]).unwrap_err();
    assert!(e.contains("u1#1") && e.contains("99"), "{e}");
    // not XML
    std::fs::write(&xlf, "<xliff><unit></xliff>").unwrap();
    assert!(run(&["merge", &xlf, "-o", &p(&d, "o.json")]).unwrap_err().contains("invalid XLIFF"));
    // a segment the skeleton does not know is simply not merged, a missing skeleton is an error
    let lone = p(&d, "lone.xlf");
    std::fs::write(&lone, "<xliff/>").unwrap();
    assert!(run(&["merge", &lone, "-o", &p(&d, "o.json")]).unwrap_err().contains("skeleton"));
}

#[test]
fn segment_prints_ids_and_tags() {
    let d = tmp("segment");
    std::fs::write(d.join("a.md"), MD).unwrap();
    let out = run(&["segment", &p(&d, "a.md")]).unwrap();
    let lines: Vec<&str> = out.lines().collect();
    assert!(lines.contains(&"u1\tLamp guide"), "{out}");
    assert!(lines.contains(&"u2\tHarbor lamp"));
    assert!(lines.contains(&"u3#1\tFirst sentence."));
    assert!(lines.contains(&"u3#2\t{<b>}Second{</b>} sentence & more."));
    assert!(lines.iter().any(|l| l.starts_with("u3#3\tSee {<a>}docs{</a>}.")));
    let para = run(&["segment", &p(&d, "a.md"), "--mode", "paragraph"]).unwrap();
    assert!(para.contains("u3\tFirst sentence."));
    assert!(!para.contains("u3#1"));
    let json = run(&["segment", &p(&d, "a.md"), "--json"]).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["format"], "markdown");
}

#[test]
fn wordcount_counts_cjk_by_character() {
    let d = tmp("wc");
    std::fs::write(d.join("a.txt"), "Welcome to Larchmoor.\n\n\u{6f6e}\u{304c}\u{5f15}\u{3044}\u{3066}\u{3044}\u{307e}\u{3059}\u{3002}\n\n\u{645}\u{631}\u{62d}\u{628}\u{627} \u{628}\u{643}\u{645}\n").unwrap();
    let out = run(&["wordcount", &p(&d, "a.txt")]).unwrap();
    assert!(out.contains("p1\t3\tparagraph"), "{out}");
    assert!(out.contains("p2\t8\tparagraph"));
    assert!(out.contains("p3\t2\tparagraph"));
    assert!(out.contains("total\t13\t3 units, 3 segments"));
    // locked units are shown but not totalled
    std::fs::write(d.join("a.json"), "{\"u\": \"https://x.test\", \"t\": \"Hello big world\"}").unwrap();
    let out = run(&["wordcount", &p(&d, "a.json")]).unwrap();
    assert!(out.contains("total\t3\t1 units"), "{out}");
}

#[test]
fn pseudo_translates_every_format_and_keeps_it_valid() {
    let d = tmp("pseudo");
    for (file, src) in [("a.txt", TXT), ("a.json", JSON), ("a.md", MD)] {
        std::fs::write(d.join(file), src).unwrap();
        let o = p(&d, &format!("ps-{file}"));
        let msg = run(&["pseudo", &p(&d, file), "-o", &o]).unwrap();
        assert!(msg.contains("pseudo-translated"), "{msg}");
        let out = std::fs::read_to_string(&o).unwrap();
        assert_ne!(out, src);
        assert!(out.contains('['), "{out}");
        assert!(out.len() > src.len() / 2);
    }
    let json = std::fs::read_to_string(d.join("ps-a.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    let a = v["a"].as_str().unwrap();
    assert!(a.contains("<b>") && a.contains("</b>") && a.contains("{name}") && a.contains("%d"), "{a}");
    assert_eq!(v["url"], "https://x.test");
    // 30% longer, in brackets
    assert!(a.starts_with("[\u{108}") || a.starts_with('['), "{a}");
    assert!(a.contains(']'));
    let md = std::fs::read_to_string(d.join("ps-a.md")).unwrap();
    assert!(md.contains("**") && md.contains("](https://x.test"), "{md}");
    assert!(md.contains("title: \"[") || md.contains("title: [") || md.contains("title:"), "{md}");
    // default output name
    run(&["pseudo", &p(&d, "a.txt")]).unwrap();
    assert!(d.join("a.pseudo.txt").exists());
}

#[test]
fn eleven_languages_through_the_whole_pipeline() {
    let d = tmp("langs");
    let langs: &[(&str, &str)] = &[
        ("en", "Welcome. The tide is out."),
        ("es", "\u{a1}Bienvenido! \u{bf}C\u{f3}mo est\u{e1}s?"),
        ("fr", "Bienvenue. \u{c0} bient\u{f4}t !"),
        ("it", "Benvenuti. La marea \u{e8} bassa."),
        ("de", "Willkommen. Die Flut ist \u{fc}ber."),
        ("nl", "Welkom. Het is eb."),
        ("pt", "Bem-vindo. A mar\u{e9} est\u{e1} baixa."),
        ("ja", "\u{3088}\u{3046}\u{3053}\u{305d}\u{3002}\u{6f6e}\u{304c}\u{5f15}\u{3044}\u{3066}\u{3044}\u{307e}\u{3059}\u{3002}"),
        ("zh", "\u{6b22}\u{8fce}\u{3002}\u{6f6e}\u{6c34}\u{9000}\u{4e86}\u{3002}"),
        ("ko", "\u{d658}\u{c601}\u{d569}\u{b2c8}\u{b2e4}. \u{ac8c}\u{c774}\u{bc8c}\u{c774} \u{c379}\u{c2b5}\u{b2c8}\u{b2e4}."),
        ("ar", "\u{645}\u{631}\u{62d}\u{628}\u{627}. \u{647}\u{644} \u{623}\u{646}\u{62a} \u{628}\u{62e}\u{64a}\u{631}\u{61f}"),
    ];
    for (lang, text) in langs {
        for ext in ["txt", "json", "md"] {
            let body = if ext == "json" { format!("{{\"k\": \"{text}\"}}") } else { format!("{text}\n") };
            let f = p(&d, &format!("{lang}.{ext}"));
            std::fs::write(&f, &body).unwrap();
            let seg = run(&["segment", &f, "--source", lang]).unwrap();
            assert_eq!(seg.lines().count(), 2, "{lang} {ext}: {seg}");
            let xlf = format!("{f}.xlf");
            run(&["extract", &f, "--source", lang, "--target", "en", "-o", &xlf]).unwrap();
            let o = format!("{f}.out");
            run(&["merge", &xlf, "-o", &o]).unwrap();
            assert_eq!(std::fs::read_to_string(&o).unwrap(), body, "{lang} {ext}");
            run(&["pseudo", &f, "--source", lang, "-o", &format!("{f}.ps")]).unwrap();
        }
    }
}

#[test]
fn usage_and_errors() {
    assert!(run(&[]).unwrap().contains("Usage:"));
    assert!(run(&["help"]).unwrap().contains("bilingua extract"));
    assert!(run(&["frobnicate"]).unwrap_err().contains("unknown command"));
    assert!(run(&["extract"]).unwrap_err().contains("needs a file"));
    assert!(run(&["extract", "a", "b"]).unwrap_err().contains("one file"));
    assert!(run(&["extract", "nofile.txt", "-o", "x.xlf"]).unwrap_err().contains("cannot read"));
    assert!(run(&["segment", "x", "--mode", "weird"]).unwrap_err().contains("unknown mode"));
    assert!(run(&["segment", "x", "--bogus"]).unwrap_err().contains("unknown option"));
    assert!(run(&["segment", "x", "--source"]).unwrap_err().contains("needs a value"));
    let d = tmp("errs");
    std::fs::write(d.join("a.txt"), "hi there").unwrap();
    assert!(run(&["extract", &p(&d, "a.txt")]).unwrap_err().contains("-o"));
    std::fs::write(d.join("bad.json"), "{oops").unwrap();
    assert!(run(&["segment", &p(&d, "bad.json")]).unwrap_err().contains("bad.json"));
    std::fs::write(d.join("a.xyz"), [0u8, 159, 146, 150]).unwrap();
    assert!(run(&["segment", &p(&d, "a.xyz")]).unwrap_err().contains("no parser"));
}

#[test]
fn the_binary_runs_and_reports_errors_with_exit_code_1() {
    let exe = env!("CARGO_BIN_EXE_bilingua");
    let d = tmp("bin");
    std::fs::write(d.join("a.txt"), "One. Two.\n").unwrap();
    let ok = Command::new(exe).args(["segment", &p(&d, "a.txt")]).output().unwrap();
    assert!(ok.status.success());
    assert_eq!(String::from_utf8_lossy(&ok.stdout), "p1#1\tOne.\np1#2\tTwo.\n");
    let bad = Command::new(exe).args(["nope"]).output().unwrap();
    assert_eq!(bad.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&bad.stderr).starts_with("bilingua: unknown command"));
}

#[test]
fn format_options_are_listed_and_checked() {
    let list = run(&["options"]).unwrap();
    assert!(list.contains("csv.columns\ttext") && list.contains("odf.translate_comments\tbool"), "{list}");
    let d = tmp("opts");
    std::fs::write(d.join("a.txt"), TXT).unwrap();
    let f = p(&d, "a.txt");
    assert!(run(&["segment", &f, "--option", "docx.translate_comments=false"]).is_ok());
    let e = run(&["segment", &f, "--option", "docx.translate_coments=false"]).unwrap_err();
    assert!(e.contains("docx.translate_coments"), "{e}");
    assert!(run(&["segment", &f, "--option", "novalue"]).is_err());
}

const SDL_SEG_LOCK: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>
<xliff version=\"1.2\" xmlns=\"urn:oasis:names:tc:xliff:document:1.2\" xmlns:sdl=\"http://sdl.com/FileTypes/SdlXliff/1.0\" sdl:version=\"1.0\">
<file original=\"a.txt\" source-language=\"en-US\" target-language=\"de-DE\" datatype=\"x-sdlfilterframework2\"><body>
<trans-unit id=\"t1\"><source>Open it. Keep this. Close it.</source><seg-source><mrk mtype=\"seg\" mid=\"1\">Open it.</mrk> <mrk mtype=\"seg\" mid=\"2\">Keep this.</mrk> <mrk mtype=\"seg\" mid=\"3\">Close it.</mrk></seg-source><target><mrk mtype=\"seg\" mid=\"1\"></mrk> <mrk mtype=\"seg\" mid=\"2\">Behalten.</mrk> <mrk mtype=\"seg\" mid=\"3\"></mrk></target><sdl:seg-defs><sdl:seg id=\"2\" conf=\"Translated\" locked=\"true\"/></sdl:seg-defs></trans-unit>
</body></file></xliff>
";

#[test]
fn pseudo_and_merge_skip_a_locked_segment_in_an_open_unit() {
    let d = tmp("seglock");
    std::fs::write(d.join("a.sdlxliff"), SDL_SEG_LOCK).unwrap();
    let o = p(&d, "ps.sdlxliff");
    let msg = run(&["pseudo", &p(&d, "a.sdlxliff"), "-o", &o]).unwrap();
    assert!(msg.contains("pseudo-translated"), "{msg}");
    let out = std::fs::read_to_string(&o).unwrap();
    assert!(out.contains("<mrk mtype=\"seg\" mid=\"2\">Behalten.</mrk>"), "{out}");
    assert!(out.contains("<mrk mtype=\"seg\" mid=\"1\">[") && out.contains("<mrk mtype=\"seg\" mid=\"3\">["), "{out}");
    // extract to XLIFF 2 carries the locked target; merging it back leaves the locked segment alone
    let xlf = p(&d, "a.xlf");
    run(&["extract", &p(&d, "a.sdlxliff"), "--target", "de-DE", "-o", &xlf]).unwrap();
    let x = std::fs::read_to_string(&xlf).unwrap();
    assert!(x.contains("Behalten."), "{x}");
    let x = x.replace("<source xml:space=\"preserve\">Open it.</source>\n", "<source xml:space=\"preserve\">Open it.</source>\n        <target>Offnen.</target>\n");
    std::fs::write(&xlf, x).unwrap();
    let outp = p(&d, "out.sdlxliff");
    let msg = run(&["merge", &xlf, "-o", &outp]).unwrap();
    assert!(msg.starts_with("1 targets merged"), "{msg}");
    let out = std::fs::read_to_string(&outp).unwrap();
    assert!(out.contains(">Offnen.</mrk>") && out.contains("<mrk mtype=\"seg\" mid=\"2\">Behalten.</mrk>"), "{out}");
}

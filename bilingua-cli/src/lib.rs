//! The `bilingua` command line. `run` is the whole program, so tests can call it without a process.

mod pseudo;
mod words;
mod xliff;

use bilingua::*;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const USAGE: &str = "\
bilingua: open source files for translation

Usage:
  bilingua extract <file> [--source en] [--target es-ES] [--mode sentence|paragraph|document] -o out.xlf
      Write XLIFF 2.1 (out.xlf) and the skeleton (out.xlf.skl.json).
  bilingua merge <out.xlf> -o translated.ext [--skeleton out.xlf.skl.json]
      Read the targets from the XLIFF and write the file in its original format.
  bilingua segment <file> [--source en] [--mode sentence|paragraph|document] [--json]
      Print the segments, one per line (id, tab, text). --json prints the whole document.
  bilingua wordcount <file> [--source en] [--mode sentence|paragraph|document]
      Words per unit and in total. Han, Hiragana and Katakana count one word per character.
  bilingua pseudo <file> [--source en] [--target xx] [--mode ...] [-o out]
      Pseudo-translate: accented letters, about 30 percent longer, in brackets, tags kept.
  bilingua options
      List the format options (--option key=value on extract, segment, wordcount and pseudo).

Formats: .txt, .json, .md (more come with the Bilingua format crates).
";

struct Args {
    cmd: String,
    positional: Vec<String>,
    source: Option<String>,
    target: Option<String>,
    mode: Granularity,
    output: Option<PathBuf>,
    skeleton: Option<PathBuf>,
    json: bool,
    /// `--option key=value`, repeatable: `ExtractOptions::format_options`.
    options: Vec<(String, String)>,
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut it = args.iter();
    let cmd = it.next().ok_or("no command given")?.clone();
    let mut a = Args { cmd, positional: vec![], source: None, target: None, mode: Granularity::Sentence, output: None, skeleton: None, json: false, options: vec![] };
    while let Some(x) = it.next() {
        let mut value = |name: &str| it.next().cloned().ok_or(format!("{name} needs a value"));
        match x.as_str() {
            "--source" | "-s" => a.source = Some(value("--source")?),
            "--target" | "-t" => a.target = Some(value("--target")?),
            "--mode" | "-m" => {
                a.mode = match value("--mode")?.as_str() {
                    "sentence" => Granularity::Sentence,
                    "paragraph" => Granularity::Paragraph,
                    "document" => Granularity::Document,
                    other => return Err(format!("unknown mode '{other}' (use sentence, paragraph or document)")),
                }
            }
            "-o" | "--output" => a.output = Some(PathBuf::from(value("-o")?)),
            "--skeleton" => a.skeleton = Some(PathBuf::from(value("--skeleton")?)),
            "--json" => a.json = true,
            "--option" => {
                let kv = value("--option")?;
                let (k, v) = kv.split_once('=').ok_or(format!("--option needs key=value, not {kv}"))?;
                a.options.push((k.trim().to_string(), v.trim().to_string()));
            }
            s if s.starts_with('-') && s.len() > 1 => return Err(format!("unknown option {s}")),
            _ => a.positional.push(x.clone()),
        }
    }
    Ok(a)
}

fn one_file(a: &Args) -> Result<&str, String> {
    match a.positional.as_slice() {
        [f] => Ok(f),
        [] => Err(format!("{} needs a file", a.cmd)),
        _ => Err(format!("{} takes one file", a.cmd)),
    }
}

fn open(a: &Args, path: &str) -> Result<(Document, String), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let name = Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string());
    let src = a.source.clone().unwrap_or_else(|| "en".to_string());
    let mut opts = ExtractOptions::new().with_source_lang(src.clone()).with_target_lang(a.target.clone()).with_granularity(a.mode);
    for (k, v) in &a.options {
        opts = opts.with_format_option(k.as_str(), v);
    }
    let mut doc = extract(&name, &bytes, &opts).map_err(|e| format!("{path}: {e}"))?;
    doc.source_lang = Some(src);
    Ok((doc, name))
}

/// Runs the program. `args` excludes the program name. Output for the user goes to `out`.
pub fn run(args: &[String], out: &mut dyn Write) -> Result<(), String> {
    if args.is_empty() || matches!(args[0].as_str(), "help" | "--help" | "-h") {
        let _ = write!(out, "{USAGE}");
        return Ok(());
    }
    let a = parse_args(args)?;
    match a.cmd.as_str() {
        "extract" => cmd_extract(&a, out),
        "merge" => cmd_merge(&a, out),
        "segment" => cmd_segment(&a, out),
        "wordcount" => cmd_wordcount(&a, out),
        "pseudo" => cmd_pseudo(&a, out),
        "options" => {
            for k in format_option_keys() {
                let kind = match k.kind {
                    OptionKind::Bool => "bool",
                    OptionKind::Int => "int",
                    _ => "text",
                };
                let _ = writeln!(out, "{}\t{kind}\t{}", k.key, k.help);
            }
            Ok(())
        }
        other => Err(format!("unknown command '{other}'. Run 'bilingua help'.")),
    }
}

fn skeleton_path(xlf: &Path) -> PathBuf {
    let mut s = xlf.as_os_str().to_owned();
    s.push(".skl.json");
    PathBuf::from(s)
}

fn cmd_extract(a: &Args, out: &mut dyn Write) -> Result<(), String> {
    let path = one_file(a)?;
    let o = a.output.as_ref().ok_or("extract needs -o out.xlf")?;
    let (doc, name) = open(a, path)?;
    let src = doc.source_lang.clone().unwrap_or_else(|| "en".into());
    let xml = xliff::write(&doc, &name, &src, a.target.as_deref());
    std::fs::write(o, xml).map_err(|e| format!("cannot write {}: {e}", o.display()))?;
    let skl = skeleton_path(o);
    let json = serde_json::json!({ "bilingua_cli": 1, "document": doc });
    std::fs::write(&skl, serde_json::to_vec(&json).map_err(|e| e.to_string())?).map_err(|e| format!("cannot write {}: {e}", skl.display()))?;
    let _ = writeln!(out, "{}: {} units, {} segments -> {} and {}", name, doc.units.len(), doc.segment_count(), o.display(), skl.display());
    Ok(())
}

fn cmd_merge(a: &Args, out: &mut dyn Write) -> Result<(), String> {
    let path = one_file(a)?;
    let o = a.output.as_ref().ok_or("merge needs -o translated-file")?;
    let skl = a.skeleton.clone().unwrap_or_else(|| skeleton_path(Path::new(path)));
    let raw = std::fs::read(&skl).map_err(|e| format!("cannot read the skeleton {}: {e}", skl.display()))?;
    let v: serde_json::Value = serde_json::from_slice(&raw).map_err(|e| format!("{} is not a Bilingua skeleton: {e}", skl.display()))?;
    let mut doc: Document = serde_json::from_value(v.get("document").cloned().ok_or("the skeleton file has no document")?)
        .map_err(|e| format!("{} is not a Bilingua skeleton: {e}", skl.display()))?;
    let xml = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let read = xliff::read(&xml, &doc)?;
    if doc.target_lang.is_none() {
        doc.target_lang = read.target_lang.clone();
    }
    let mut n = 0usize;
    for (id, target, state) in read.targets {
        doc.set_target(&id, target).map_err(|e| format!("segment {id}: {e}"))?;
        doc.set_state(&id, state).map_err(|e| format!("segment {id}: {e}"))?;
        n += 1;
    }
    let bytes = bilingua::merge(&doc).map_err(|e| e.to_string())?;
    std::fs::write(o, bytes).map_err(|e| format!("cannot write {}: {e}", o.display()))?;
    let _ = writeln!(out, "{} targets merged -> {}", n, o.display());
    Ok(())
}

fn show(inl: &[Inline]) -> String {
    inl.iter()
        .map(|i| match i {
            Inline::Text(t) => t.replace('\n', "\\n").replace('\r', "\\r").replace('\t', "\\t"),
            Inline::Tag { display, .. } => format!("{{{display}}}"),
        })
        .collect()
}

fn cmd_segment(a: &Args, out: &mut dyn Write) -> Result<(), String> {
    let (doc, _) = open(a, one_file(a)?)?;
    if a.json {
        let _ = writeln!(out, "{}", serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())?);
        return Ok(());
    }
    for (_, s) in doc.segments() {
        let _ = writeln!(out, "{}\t{}", s.id, show(&s.source));
    }
    Ok(())
}

fn cmd_wordcount(a: &Args, out: &mut dyn Write) -> Result<(), String> {
    let (doc, _) = open(a, one_file(a)?)?;
    let mut total = 0usize;
    for u in &doc.units {
        let n: usize = u.segments.iter().map(|s| words::count_words(&plain_text(&s.source))).sum();
        if u.translate {
            total += n;
        }
        let what = u.context.key.clone().or_else(|| u.context.kind.clone()).unwrap_or_default();
        let _ = writeln!(out, "{}\t{}\t{}{}", u.id, n, what, if u.translate { "" } else { "\t(locked)" });
    }
    let _ = writeln!(out, "total\t{total}\t{} units, {} segments", doc.units.len(), doc.segment_count());
    Ok(())
}

fn cmd_pseudo(a: &Args, out: &mut dyn Write) -> Result<(), String> {
    let path = one_file(a)?;
    let (mut doc, name) = open(a, path)?;
    let ids: Vec<(String, Vec<Inline>)> = doc.units.iter().filter(|u| u.translate).flat_map(|u| u.segments.iter().map(|s| (s.id.clone(), s.source.clone()))).collect();
    let mut n = 0usize;
    for (id, source) in ids {
        let chars = plain_text(&source).chars().count();
        if !source.iter().any(|i| matches!(i, Inline::Text(t) if t.chars().any(|c| c.is_alphanumeric()))) {
            continue;
        }
        let mut target: Vec<Inline> = Vec::with_capacity(source.len() + 2);
        target.push(Inline::text("["));
        for i in &source {
            match i {
                Inline::Text(t) => target.push(Inline::Text(pseudo::accent_text(t))),
                tag => target.push(tag.clone()),
            }
        }
        target.push(Inline::Text(format!("{}]", "\u{b7}".repeat(pseudo::padding(chars)))));
        doc.set_target(&id, merge_text(target)).map_err(|e| format!("segment {id}: {e}"))?;
        doc.set_state(&id, SegState::Translated).map_err(|e| e.to_string())?;
        n += 1;
    }
    let dest = match &a.output {
        Some(o) => o.clone(),
        None => {
            let p = Path::new(path);
            let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| name.clone());
            let ext = p.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
            p.with_file_name(format!("{stem}.pseudo{ext}"))
        }
    };
    let bytes = bilingua::merge(&doc).map_err(|e| e.to_string())?;
    std::fs::write(&dest, bytes).map_err(|e| format!("cannot write {}: {e}", dest.display()))?;
    let _ = writeln!(out, "{} segments pseudo-translated -> {}", n, dest.display());
    Ok(())
}

//! The LocOS profile's SRT / WebVTT filter (Track D): a port of Localization OS's `srt_filter.py`, so units and
//! built files match LocOS exactly.
//!
//! One unit per cue (its text lines joined by `\n`, stripped). In a WebVTT file (first line `WEBVTT`) every cue tag
//! is a code: a span closed in the same cue is a pair (`g<n>`), anything else a standalone code (`x<n>`); in SRT
//! every `{\...}` override block is a standalone code and `<i>` / `<font>` stay text. Tags and blocks at the edges
//! of a cue stay out of the unit (a toggle whose partner is inside stays in). Tag ids are LocOS's token names, so
//! the adapter's tokens are LocOS's. Build writes each code back as its source tag (pairs balanced, a repeated or
//! unknown code dropped), escapes what a WebVTT translation brought in, removes blank lines from a translation,
//! and stamps the WebVTT `Language:` header on a localized build. The skeleton is the decoded file
//! (`Skeleton::Binary`, UTF-8), which marks a profile document.

use bilingua_model::*;
use regex::Regex;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::OnceLock;

fn re(cell: &'static OnceLock<Regex>, pat: &str) -> &'static Regex {
    cell.get_or_init(|| Regex::new(pat).expect("subtitle pattern"))
}

fn vtt_tag() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"<[^<>\n]*>")
}
fn vtt_open() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"^<(c|i|b|u|v|lang|ruby|rt)((?:\.[^\s.<>]*)*)(?:[ \t\x0C][^<>]*)?>$")
}
fn vtt_close() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"^</(c|i|b|u|v|lang|ruby|rt)[ \t\x0C]*>$")
}
fn vtt_known() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"</?(?:c|i|b|u|v|lang|ruby|rt)(?:\.[^\s.<>]+)*(?:[ \t\x0C][^<>\n]*)?>|<(?:[0-9]{2,}:)?[0-5][0-9]:[0-5][0-9]\.[0-9]{3}>")
}
fn srt_override() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, r"\{\\[^{}\n]*\}")
}
fn any_marker() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    re(&R, "\u{27E6}/?[a-z]?[0-9]*\u{27E7}")
}

/// `\\(?:[ibus]\d|r(?![a-z]))` somewhere in an override block: a style toggle.
fn srt_toggle(s: &str) -> bool {
    let b: Vec<char> = s.chars().collect();
    (0..b.len()).any(|i| {
        b[i] == '\\'
            && match b.get(i + 1) {
                Some('i' | 'b' | 'u' | 's') => b.get(i + 2).is_some_and(|c| py_isdecimal_char(*c)),
                Some('r') => !b.get(i + 2).is_some_and(|c| c.is_ascii_lowercase()),
                _ => false,
            }
    })
}

/// `&` that does not start a character reference: `&(?!(?:#[0-9]+|#[xX][0-9a-fA-F]+|[A-Za-z][A-Za-z0-9]*);)`.
fn escape_bare_amp(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    for (i, c) in s.char_indices() {
        if c != '&' {
            out.push(c);
            continue;
        }
        let rest = &b[i + 1..];
        let refers = if let Some(r) = rest.strip_prefix(b"#x").or_else(|| rest.strip_prefix(b"#X")) {
            let n = r.iter().take_while(|c| c.is_ascii_hexdigit()).count();
            n > 0 && r.get(n) == Some(&b';')
        } else {
            false
        } || if let Some(r) = rest.strip_prefix(b"#") {
            let n = r.iter().take_while(|c| c.is_ascii_digit()).count();
            n > 0 && r.get(n) == Some(&b';')
        } else {
            let n = rest.iter().take_while(|c| c.is_ascii_alphanumeric()).count();
            n > 0 && rest[0].is_ascii_alphabetic() && rest.get(n) == Some(&b';')
        };
        out.push_str(if refers { "&" } else { "&amp;" });
    }
    out
}

/// One cue as LocOS's `_parse_cues` yields it.
struct Cue {
    header: Vec<String>,
    text: Vec<String>,
    vtt: bool,
    signature: bool,
    pre: String,
}

fn is_signature(line: &str) -> bool {
    py_rstrip(line.trim_end_matches('\n')) == "WEBVTT" || line.starts_with("WEBVTT ") || line.starts_with("WEBVTT\t")
}

fn is_structural(first: &str) -> bool {
    ["NOTE", "STYLE", "REGION"].iter().any(|k| first == *k || first.starts_with(&format!("{k} ")) || first.starts_with(&format!("{k}\t")))
}

/// `re.split(r"(\n[ \t]*\n)", core)`: blocks and the separators between them.
fn split_blocks(core: &str) -> (Vec<&str>, Vec<&str>) {
    let b = core.as_bytes();
    let (mut blocks, mut seps) = (Vec::new(), Vec::new());
    let (mut start, mut i) = (0usize, 0usize);
    while i < b.len() {
        if b[i] == b'\n' {
            let mut j = i + 1;
            while j < b.len() && matches!(b[j], b' ' | b'\t') {
                j += 1;
            }
            if j < b.len() && b[j] == b'\n' {
                blocks.push(&core[start..i]);
                seps.push(&core[i..j + 1]);
                start = j + 1;
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    blocks.push(&core[start..]);
    (blocks, seps)
}

/// LocOS `_parse_cues` over text whose line breaks are `\n`; returns the cues and the tail after the last one.
fn parse_cues(s: &str) -> (Vec<Cue>, String) {
    let core = s.trim_matches('\n');
    let lead = &s[..s.len() - s.trim_start_matches('\n').len()];
    let (blocks, seps) = split_blocks(core);
    let mut pending = lead.to_string();
    let mut vtt = false;
    let mut cues = Vec::new();
    for (bi, block) in blocks.iter().enumerate() {
        let sep = seps.get(bi).copied().unwrap_or("");
        let lines: Vec<String> = block.split('\n').map(str::to_string).collect();
        if !lines.iter().any(|l| !py_strip(l).is_empty()) {
            pending.push_str(block);
            pending.push_str(sep);
            continue;
        }
        let pre = std::mem::replace(&mut pending, sep.to_string());
        let first = py_strip(&lines[0]).to_string();
        if bi == 0 && is_signature(&lines[0]) {
            vtt = true;
            cues.push(Cue { header: lines, text: vec![], vtt, signature: true, pre });
            continue;
        }
        if is_structural(&first) {
            cues.push(Cue { header: lines, text: vec![], vtt, signature: false, pre });
            continue;
        }
        let mut ti = 0;
        if ti < lines.len() && !lines[ti].contains("-->") && ti + 1 < lines.len() && lines[ti + 1].contains("-->") {
            ti += 1;
        }
        if ti < lines.len() && lines[ti].contains("-->") {
            ti += 1;
        }
        let text = lines[ti..].to_vec();
        let header = lines[..ti].to_vec();
        cues.push(Cue { header, text, vtt, signature: false, pre });
    }
    let tail = format!("{pending}{}", &s[lead.len() + core.len()..]);
    (cues, tail)
}

/// A piece of a cue's unit text.
#[derive(Debug, Clone, PartialEq)]
enum Piece {
    Text(String),
    Open(String, String),
    Close(String, String),
    Point(String, String),
}

/// (prefix, unit pieces, suffix, token name -> source tag) of one cue text.
type Tokenized = (String, Vec<Piece>, String, HashMap<String, String>);

fn pieces_of(text: &str, rx: &Regex) -> Vec<(bool, String)> {
    let mut v = Vec::new();
    let mut pos = 0;
    for m in rx.find_iter(text) {
        if m.start() > pos {
            v.push((false, text[pos..m.start()].to_string()));
        }
        v.push((true, m.as_str().to_string()));
        pos = m.end();
    }
    if pos < text.len() {
        v.push((false, text[pos..].to_string()));
    }
    v
}

fn bounds(p: &[(bool, String)], pinned: &HashSet<usize>) -> (usize, usize) {
    let (mut lo, mut hi) = (0, p.len());
    while lo < hi {
        let (tok, s) = &p[lo];
        if (*tok && !pinned.contains(&lo)) || (!tok && py_strip(s).is_empty()) {
            lo += 1;
            continue;
        }
        break;
    }
    while hi > lo {
        let (tok, s) = &p[hi - 1];
        if (*tok && !pinned.contains(&(hi - 1))) || (!tok && py_strip(s).is_empty()) {
            hi -= 1;
            continue;
        }
        break;
    }
    (lo, hi)
}

fn vtt_pairs(p: &[(bool, String)]) -> BTreeMap<usize, usize> {
    let mut stack: Vec<(usize, String)> = Vec::new();
    let mut pairs = BTreeMap::new();
    for (k, (tok, s)) in p.iter().enumerate() {
        if !tok {
            continue;
        }
        if let Some(m) = vtt_open().captures(s) {
            stack.push((k, m[1].to_string()));
            continue;
        }
        let Some(m) = vtt_close().captures(s) else { continue };
        let name = &m[1];
        if name == "ruby" && stack.last().is_some_and(|t| t.1 == "rt") {
            stack.pop();
        }
        if stack.last().is_some_and(|t| t.1 == name) {
            let (o, _) = stack.pop().expect("checked");
            pairs.insert(o, k);
        }
    }
    pairs
}

/// Moves the white space at the unit's edges into the prefix and suffix.
fn edges(p: &[(bool, String)], lo: usize, hi: usize) -> (String, Vec<(bool, String)>, String) {
    let mut prefix: String = p[..lo].iter().map(|x| x.1.as_str()).collect();
    let mut suffix: String = p[hi..].iter().map(|x| x.1.as_str()).collect();
    let mut core = p[lo..hi].to_vec();
    if let Some((false, s)) = core.first().cloned() {
        let t = py_lstrip(&s).to_string();
        prefix.push_str(&s[..s.len() - t.len()]);
        core[0] = (false, t);
    }
    if let Some((false, s)) = core.last().cloned() {
        let t = py_rstrip(&s).to_string();
        suffix = format!("{}{suffix}", &s[t.len()..]);
        let n = core.len() - 1;
        core[n] = (false, t);
    }
    (prefix, core, suffix)
}

fn vtt_tokenize(text: &str) -> Tokenized {
    let p = pieces_of(text, vtt_tag());
    if !p.iter().any(|x| x.0) {
        return (String::new(), vec![Piece::Text(text.to_string())], String::new(), HashMap::new());
    }
    let pairs = vtt_pairs(&p);
    let (lo, hi) = if p.iter().any(|(t, s)| !t && !py_strip(s).is_empty()) {
        let mut pinned: HashSet<usize> = HashSet::new();
        loop {
            let (lo, hi) = bounds(&p, &pinned);
            let mut more = HashSet::new();
            for (&o, &c) in &pairs {
                if (lo <= o && o < hi) != (lo <= c && c < hi) {
                    more.insert(o);
                    more.insert(c);
                }
            }
            if more.is_subset(&pinned) {
                break (lo, hi);
            }
            pinned.extend(more);
        }
    } else {
        (0, p.len())
    };
    let (prefix, core, suffix) = edges(&p, lo, hi);
    let closes: HashMap<usize, usize> = pairs.iter().filter(|(o, _)| lo <= **o && **o < hi).map(|(o, c)| (*c, *o)).collect();
    let mut ids: HashMap<usize, usize> = HashMap::new();
    let mut tokens = HashMap::new();
    let mut out = Vec::new();
    let mut n = 0;
    for (j, (tok, s)) in core.into_iter().enumerate() {
        let k = lo + j;
        if !tok {
            out.push(Piece::Text(s));
        } else if let Some(o) = closes.get(&k) {
            let name = format!("g{}", ids[o]);
            tokens.insert(format!("/{name}"), s.clone());
            out.push(Piece::Close(name, s));
        } else {
            n += 1;
            if pairs.get(&k).is_some_and(|c| *c < hi) {
                ids.insert(k, n);
                let name = format!("g{n}");
                tokens.insert(name.clone(), s.clone());
                out.push(Piece::Open(name, s));
            } else {
                let name = format!("x{n}");
                tokens.insert(name.clone(), s.clone());
                out.push(Piece::Point(name, s));
            }
        }
    }
    (prefix, out, suffix, tokens)
}

fn srt_tokenize(text: &str) -> Tokenized {
    let p = pieces_of(text, srt_override());
    if !p.iter().any(|x| x.0) {
        return (String::new(), vec![Piece::Text(text.to_string())], String::new(), HashMap::new());
    }
    let (lo, hi) = if p.iter().any(|(t, s)| !t && !py_strip(s).is_empty()) {
        let (mut lo, mut hi) = bounds(&p, &HashSet::new());
        if p[lo..hi].iter().any(|x| x.0) {
            let pinned: HashSet<usize> = (0..lo).chain(hi..p.len()).filter(|&k| p[k].0 && srt_toggle(&p[k].1)).collect();
            if !pinned.is_empty() {
                (lo, hi) = bounds(&p, &pinned);
            }
        }
        (lo, hi)
    } else {
        (0, p.len())
    };
    let (prefix, core, suffix) = edges(&p, lo, hi);
    let mut tokens = HashMap::new();
    let mut out = Vec::new();
    for (tok, s) in core {
        if tok {
            let name = format!("x{}", tokens.len() + 1);
            tokens.insert(name.clone(), s.clone());
            out.push(Piece::Point(name, s));
        } else {
            out.push(Piece::Text(s));
        }
    }
    (prefix, out, suffix, tokens)
}

fn core_text(p: &[Piece]) -> String {
    let mut s = String::new();
    for x in p {
        match x {
            Piece::Text(t) => s.push_str(t),
            Piece::Open(n, _) | Piece::Point(n, _) => s.push_str(&format!("\u{27E6}{n}\u{27E7}")),
            Piece::Close(n, _) => s.push_str(&format!("\u{27E6}/{n}\u{27E7}")),
        }
    }
    s
}

/// LocOS `_cue_text`: blank lines removed from a translation; one with no text left keeps the source.
fn cue_text(new: &str, source: &str) -> String {
    let n = new.replace("\r\n", "\n").replace('\r', "\n");
    let kept: Vec<&str> = n.split('\n').filter(|l| !py_strip(l).is_empty()).collect();
    if kept.is_empty() {
        source.to_string()
    } else {
        kept.join("\n")
    }
}

fn vtt_escape(text: &str) -> String {
    let plain = |s: &str| escape_bare_amp(s).replace('<', "&lt;").replace("-->", "--&gt;");
    let mut out = String::new();
    let mut pos = 0;
    for m in vtt_known().find_iter(text) {
        if m.as_str().contains("-->") {
            continue;
        }
        out.push_str(&plain(&text[pos..m.start()]));
        out.push_str(m.as_str());
        pos = m.end();
    }
    out.push_str(&plain(&text[pos..]));
    out
}

fn vtt_detokenize(translated: &str, tokens: &HashMap<String, String>) -> String {
    let mut out = String::new();
    let mut stack: Vec<String> = Vec::new();
    let mut used: HashSet<String> = HashSet::new();
    let mut pos = 0;
    for m in any_marker().find_iter(translated) {
        out.push_str(&vtt_escape(&translated[pos..m.start()]));
        pos = m.end();
        let s = m.as_str();
        let name = &s['\u{27E6}'.len_utf8()..s.len() - '\u{27E7}'.len_utf8()];
        if let Some(closed) = name.strip_prefix('/') {
            if stack.last().is_some_and(|t| t == closed) {
                stack.pop();
                if let Some(t) = tokens.get(name) {
                    out.push_str(t);
                }
            }
            continue;
        }
        if let (Some(t), false) = (tokens.get(name), used.contains(name)) {
            used.insert(name.to_string());
            out.push_str(t);
            if name.starts_with('g') {
                stack.push(name.to_string());
            }
        }
    }
    out.push_str(&vtt_escape(&translated[pos..]));
    while let Some(n) = stack.pop() {
        if let Some(t) = tokens.get(&format!("/{n}")) {
            out.push_str(t);
        }
    }
    out
}

fn vtt_cue(text: &str, translated: &str) -> String {
    let (prefix, _, suffix, tokens) = vtt_tokenize(text);
    let tagged = !tokens.is_empty() || !py_strip(&prefix).is_empty() || !py_strip(&suffix).is_empty();
    if tagged && !any_marker().is_match(translated) && vtt_known().is_match(translated) {
        return cue_text(&vtt_escape(translated), text);
    }
    cue_text(&format!("{prefix}{}{suffix}", vtt_detokenize(translated, &tokens)), text)
}

fn srt_cue(text: &str, translated: &str) -> String {
    let (prefix, _, suffix, tokens) = srt_tokenize(text);
    if tokens.is_empty() && prefix.is_empty() && suffix.is_empty() {
        return cue_text(translated, text);
    }
    if !any_marker().is_match(translated) && srt_override().is_match(translated) {
        return cue_text(translated, text);
    }
    let mut out = String::new();
    let mut used = HashSet::new();
    let mut pos = 0;
    for m in any_marker().find_iter(translated) {
        out.push_str(&translated[pos..m.start()]);
        pos = m.end();
        let s = m.as_str();
        let name = &s['\u{27E6}'.len_utf8()..s.len() - '\u{27E7}'.len_utf8()];
        if let (Some(t), true) = (tokens.get(name), used.insert(name.to_string())) {
            out.push_str(t);
        }
    }
    out.push_str(&translated[pos..]);
    cue_text(&format!("{prefix}{out}{suffix}"), text)
}

/// A canonical BCP 47 tag (LocOS `a11y.target_tag`), or `None` when unusable.
fn canonical_tag(lang: &str) -> Option<String> {
    let t = lang.trim().replace('_', "-");
    if t.is_empty() || t.len() > 64 {
        return None;
    }
    let mut parts = t.split('-');
    let first = parts.next()?;
    if !(2..=8).contains(&first.len()) || !first.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let mut out = vec![first.to_ascii_lowercase()];
    let mut extension = false;
    for sub in parts {
        if sub.is_empty() || sub.len() > 8 || !sub.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return None;
        }
        let alpha = sub.bytes().all(|b| b.is_ascii_alphabetic());
        if sub.len() == 1 {
            extension = true;
            out.push(sub.to_ascii_lowercase());
        } else if extension {
            out.push(sub.to_ascii_lowercase());
        } else if sub.len() == 4 && alpha {
            let mut c = sub.to_ascii_lowercase();
            c[..1].make_ascii_uppercase();
            out.push(c);
        } else if (sub.len() == 2 && alpha) || (sub.len() == 3 && sub.bytes().all(|b| b.is_ascii_digit())) {
            out.push(sub.to_ascii_uppercase());
        } else {
            out.push(sub.to_ascii_lowercase());
        }
    }
    Some(out.join("-"))
}

/// LocOS `_stamp_language`: the header's `Language:` line names the target.
fn stamp_language(lines: &[String], tag: &str) -> Vec<String> {
    let mut out = lines.to_vec();
    for (i, line) in out.iter_mut().enumerate() {
        if i == 0 || line.contains("-->") || !line.get(..8).is_some_and(|h| h.eq_ignore_ascii_case("language")) {
            continue;
        }
        let b = line.as_bytes();
        let mut j = 8;
        while j < b.len() && matches!(b[j], b' ' | b'\t') {
            j += 1;
        }
        if b.get(j) != Some(&b':') {
            continue;
        }
        j += 1;
        while j < b.len() && matches!(b[j], b' ' | b'\t') {
            j += 1;
        }
        let rest = &line[j..];
        let trail = &rest[rest.trim_end_matches([' ', '\t']).len()..];
        *line = format!("{}{tag}{trail}", &line[..j]);
    }
    out
}

/// LocOS `_rebuild`.
fn rebuild(s: &str, texts: &BTreeMap<usize, String>, locale: Option<&str>) -> String {
    let (cues, tail) = parse_cues(s);
    let tag = locale.and_then(canonical_tag);
    let mut out = String::new();
    let mut k = 0;
    for cue in &cues {
        let text = py_strip(&cue.text.join("\n")).to_string();
        let mut lines = cue.header.clone();
        if let (true, Some(t)) = (cue.signature, tag.as_deref()) {
            lines = stamp_language(&lines, t);
        }
        let mut changed = lines != cue.header;
        if !text.is_empty() {
            let new = texts.get(&k);
            k += 1;
            match new {
                None => lines.push(text.clone()),
                Some(n) => {
                    let core = if cue.vtt { core_text(&vtt_tokenize(&text).1) } else { core_text(&srt_tokenize(&text).1) };
                    if *n == core {
                        lines.push(text.clone());
                    } else {
                        lines.push(if cue.vtt { vtt_cue(&text, n) } else { srt_cue(&text, n) });
                        changed = true;
                    }
                }
            }
        } else {
            lines.extend(cue.text.iter().cloned());
        }
        out.push_str(&cue.pre);
        if changed {
            out.push_str(&lines.join("\n"));
        } else {
            out.push_str(&cue.header.iter().chain(&cue.text).cloned().collect::<Vec<_>>().join("\n"));
        }
    }
    out.push_str(&tail);
    out
}

/// The decoded text with `\n` breaks and no BOM (the file's BOM, line breaks and encoding are in its skeleton tag,
/// and `enc::encode_text` writes them back as LocOS's `encode_like` does).
fn normalized(text: &str) -> String {
    let body = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    body.replace("\r\n", "\n").replace('\r', "\n")
}

/// Extracts a subtitle file the LocOS way (see the module notes).
pub fn extract(format: &'static str, text: &str, encoding: &str, opts: &ExtractOptions, warnings: Vec<Warning>) -> Result<Document, Error> {
    let (cues, _) = parse_cues(&normalized(text));
    let mut units = Vec::new();
    for cue in &cues {
        let text = py_strip(&cue.text.join("\n")).to_string();
        if text.is_empty() {
            continue;
        }
        let pieces = if cue.vtt { vtt_tokenize(&text).1 } else { srt_tokenize(&text).1 };
        let mut inl = Vec::new();
        let mut codes = BTreeMap::new();
        for p in pieces {
            match p {
                Piece::Text(t) if !t.is_empty() => inl.push(Inline::Text(t)),
                Piece::Text(_) => {}
                Piece::Open(n, raw) => {
                    codes.entry(n.clone()).or_insert_with(CodeData::new).open = Some(raw.clone());
                    inl.push(Inline::tag(n, TagKind::Open, raw));
                }
                Piece::Close(n, raw) => {
                    codes.entry(n.clone()).or_insert_with(CodeData::new).close = Some(raw.clone());
                    inl.push(Inline::tag(n, TagKind::Close, raw));
                }
                Piece::Point(n, raw) => {
                    codes.insert(n.clone(), CodeData::new().with_standalone(raw.clone()));
                    inl.push(Inline::tag(n, TagKind::Standalone, raw));
                }
            }
        }
        let mut u = Unit::new(format!("u{}", units.len() + 1), inl);
        u.codes = codes;
        u.context.kind = Some("subtitle".into());
        units.push(u);
    }
    let data = enc::locos_skeleton(encoding, text);
    let doc = Document::new(format, units, Skeleton::binary(data, bilingua_model::parser_info!())).with_options(opts);
    bilingua_model::guard::finish(doc, warnings, &opts.limits())
}

/// A unit's translation as LocOS text: its codes as their token names.
fn locos_text(v: &[Inline]) -> String {
    let mut s = String::new();
    for i in v {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { id, kind: TagKind::Close, .. } => s.push_str(&format!("\u{27E6}/{id}\u{27E7}")),
            Inline::Tag { id, .. } => s.push_str(&format!("\u{27E6}{id}\u{27E7}")),
        }
    }
    s
}

/// Writes a profile document: LocOS's `build`, or `build_localized` when the document has a target language and
/// the merge stamps it.
pub fn merge(doc: &Document, data: &[u8], opts: &MergeOptions) -> Result<Vec<u8>, Error> {
    let (tag, source) = enc::locos_skeleton_parts(data).ok_or_else(|| Error::Write("subtitle: not a LocOS profile skeleton".into()))?;
    let mut texts = BTreeMap::new();
    for (k, u) in doc.units.iter().enumerate() {
        if u.segments.iter().any(Segment::has_target) {
            texts.insert(k, locos_text(&u.output()));
        }
    }
    let out = rebuild(&normalized(source), &texts, opts.stamp_lang(doc, !texts.is_empty()));
    // LocOS `encode_like`: the BOM, the line breaks when the file used one kind, the encoding when the text fits
    enc::encode_text(&out, tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_tags_are_hoisted() {
        let (p, core, s, _) = vtt_tokenize("<v Roger>Welcome to the trail.</v>");
        assert_eq!((p.as_str(), core_text(&core).as_str(), s.as_str()), ("<v Roger>", "Welcome to the trail.", "</v>"));
        let (_, core, _, _) = srt_tokenize("{\\an8}{\\i1}Watch{\\i0} out{\\i1}");
        // the toggles at the edges stay in the unit, their partner is inside; `{\an8}` stays out
        assert_eq!(core_text(&core), "\u{27E6}x1\u{27E7}Watch\u{27E6}x2\u{27E7} out\u{27E6}x3\u{27E7}");
    }

    #[test]
    fn a_cue_with_many_tags_stays_linear() {
        // 50 000 nested spans around the text and 50 000 more inside it
        let n = 50_000;
        let cue = format!("{}{}word{}{}", "<i>".repeat(n), "<b>x</b>".repeat(n), "</i>".repeat(n), "<c>".repeat(n));
        let (_, core, _, tokens) = vtt_tokenize(&cue);
        assert!(!core.is_empty() && !tokens.is_empty());
        let out = vtt_cue(&cue, &core_text(&core));
        assert!(out.contains("word"));
        let srt = format!("{}x{}", "{\\i1}".repeat(n), "{\\i0}".repeat(n));
        assert!(!srt_tokenize(&srt).1.is_empty());
    }

    #[test]
    fn bare_ampersands_only() {
        assert_eq!(escape_bare_amp("a & b &amp; &#38; &#x26; &x"), "a &amp; b &amp; &#38; &#x26; &amp;x");
    }

    #[test]
    fn blocks_split_like_python() {
        assert_eq!(split_blocks("a\n\n\nb"), (vec!["a", "\nb"], vec!["\n\n"]));
        assert_eq!(split_blocks("a\n \t\nb"), (vec!["a", "b"], vec!["\n \t\n"]));
    }
}

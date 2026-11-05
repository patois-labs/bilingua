//! Bilingua filter for Scenarist Closed Captions (SCC, CEA-608 line 21 captions as hex byte pairs). See the README.
//!
//! One unit per caption line (pop-on, roll-up and paint-on alike), rows joined with a line break, mid-row codes as
//! standalone inline tags. Every code and every timecode is skeleton. A line whose text is untouched is written
//! back byte for byte; a changed line is re-encoded (CEA-608 characters only, 32 cells per row, rows laid out
//! again) or the whole merge is refused with the list of captions that do not fit.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod layout;
mod line;
mod tables;

use bilingua_model::*;
use layout::{encode_line, ROW_CELLS};
use line::*;

const HEADER: &str = "Scenarist_SCC V1.0";
/// A pre-parse budget: words per line. Real lines carry well under 100 words.
pub const MAX_WORDS_PER_LINE: usize = 4096;
const RAW: &str = "raw";
const STATE: &str = "state";

/// The SCC filter.
pub struct SccFilter;

fn perr(msg: impl Into<String>) -> Error {
    Error::Parse { format: "scc".into(), msg: msg.into() }
}

/// (content start, content end, end including the line ending) of every line.
fn split_lines(text: &str) -> Vec<(usize, usize, usize)> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\n' => {
                out.push((start, i, i + 1));
                i += 1;
                start = i;
            }
            b'\r' => {
                let end = if b.get(i + 1) == Some(&b'\n') { i + 2 } else { i + 1 };
                out.push((start, i, end));
                i = end;
                start = i;
            }
            _ => i += 1,
        }
    }
    out.push((start, b.len(), b.len()));
    out
}

struct Cap<'a> {
    lead: &'a str,
    tc: &'a str,
    sep: &'a str,
    rest: &'a str,
}

/// `^(\d{2}:\d{2}:\d{2}[:;.,]\d{2})([ \t]+)(.*)$` on the stripped line (Python `\d`: any decimal digit).
fn split_caption(content: &str) -> Option<Cap<'_>> {
    let s = py_strip(content);
    let lead = &content[..content.len() - py_lstrip(content).len()];
    let mut it = s.char_indices();
    let mut end = 0;
    let shape = [b'd', b'd', b':', b'd', b'd', b':', b'd', b'd', b'p', b'd', b'd'];
    for k in shape {
        let (i, c) = it.next()?;
        let ok = match k {
            b'd' => py_isdecimal_char(c),
            b':' => c == ':',
            _ => matches!(c, ':' | ';' | '.' | ','),
        };
        if !ok {
            return None;
        }
        end = i + c.len_utf8();
    }
    let tc = &s[..end];
    let after = &s[end..];
    let rest = after.trim_start_matches([' ', '\t']);
    if rest.len() == after.len() {
        return None;
    }
    Some(Cap { lead, tc, sep: &after[..after.len() - rest.len()], rest })
}

fn parse_words(rest: &str, lineno: usize) -> Result<Vec<Word>, Error> {
    let parts: Vec<&str> = rest.split(py_isspace_char).filter(|p| !p.is_empty()).collect();
    if parts.len() > MAX_WORDS_PER_LINE {
        return Err(LimitError::new(
            LimitKind::UnitChars,
            format!("SCC line {lineno} has more than {MAX_WORDS_PER_LINE} code words; refused."),
        )
        .into());
    }
    let mut out = Vec::with_capacity(parts.len());
    for p in parts {
        if !is_word(p.as_bytes()) {
            return Err(perr(format!("SCC line {lineno} has a word that is not 4 hex digits; refused.")));
        }
        let b = p.as_bytes();
        out.push([b[0], b[1], b[2], b[3]]);
    }
    Ok(out)
}

fn atoms_to_inlines(atoms: &[Atom], line: &Line) -> (Vec<Inline>, std::collections::BTreeMap<String, CodeData>) {
    let toks = line.tokens();
    let mut inl: Vec<Inline> = Vec::new();
    let mut codes = std::collections::BTreeMap::new();
    let mut text = String::new();
    for a in atoms {
        match a {
            Atom::C(c) => text.push(*c),
            Atom::T(n) => {
                if !text.is_empty() {
                    inl.push(Inline::Text(std::mem::take(&mut text)));
                }
                let Some(ev) = toks.get(*n as usize - 1) else { continue };
                let id = n.to_string();
                codes.insert(
                    id.clone(),
                    CodeData { standalone: Some(words_text(&ev.words)), label: Some(ev.name.label().to_string()), ..Default::default() },
                );
                inl.push(Inline::tag(id, TagKind::Standalone, ev.display()));
            }
        }
    }
    if !text.is_empty() {
        inl.push(Inline::Text(text));
    }
    (inl, codes)
}

fn inlines_to_atoms(inl: &[Inline]) -> Vec<Atom> {
    let mut out = Vec::new();
    for i in inl {
        match i {
            Inline::Text(t) => out.extend(t.chars().map(Atom::C)),
            Inline::Tag { id, kind: TagKind::Standalone, .. } => out.push(Atom::T(id.parse::<u32>().unwrap_or(0))),
            Inline::Tag { .. } => {}
        }
    }
    out
}

impl Filter for SccFilter {
    fn id(&self) -> &'static str {
        "scc"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["scc"]
    }
    fn sniff(&self, bytes: &[u8]) -> bool {
        // the header on the first non-blank line, in UTF-8 or UTF-16 (BOM)
        let head: Vec<u8> = bytes.iter().take(256).copied().filter(|b| *b != 0).collect();
        let mut h: &[u8] = &head;
        for bom in [&[0xEF, 0xBB, 0xBF][..], &[0xFF, 0xFE][..], &[0xFE, 0xFF][..]] {
            if let Some(r) = h.strip_prefix(bom) {
                h = r;
            }
        }
        let h = h.trim_ascii_start();
        h.starts_with(b"Scenarist_SCC V")
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        let limits = opts.limits();
        let mut dec = enc::decode_text(bytes, &enc::DecodeOptions::new(opts))?;
        let text_owned = std::mem::take(&mut dec.text);
        let text = text_owned.as_str();
        let lines = split_lines(text);
        let first = lines
            .iter()
            .position(|(s, e, _)| !py_strip(&text[*s..*e]).is_empty())
            .filter(|i| py_strip(&text[lines[*i].0..lines[*i].1]) == HEADER)
            .ok_or_else(|| perr(format!("Not a Scenarist SCC file: the first line must be '{HEADER}'.")))?;

        let mut state = State::default();
        let mut units: Vec<Unit> = Vec::new();
        let mut spans: Vec<(usize, usize)> = Vec::new();
        let mut skipped = 0usize;
        for (i, (s, e, _)) in lines.iter().enumerate() {
            if i <= first {
                continue;
            }
            let content = &text[*s..*e];
            if py_strip(content).is_empty() {
                continue;
            }
            let lineno = i + 1;
            let cap = split_caption(content)
                .ok_or_else(|| perr(format!("SCC line {lineno} is not 'timecode<TAB>hex words'; refused.")))?;
            let words = parse_words(cap.rest, lineno)?;
            let before = state;
            let line = Line::new(&words, &mut state);
            if line.rows.is_empty() {
                continue;
            }
            if !line.unit {
                skipped += 1;
                continue;
            }
            let atoms = line.text();
            if atoms.is_empty() {
                continue;
            }
            let n = units.len() + 1;
            guard::check_unit_count(n, &limits)?;
            let (inl, codes) = atoms_to_inlines(&atoms, &line);
            let mut u = Unit::new(format!("c{n}"), inl);
            u.codes = codes;
            u.set_hidden(RAW, content);
            u.set_hidden(STATE, before.to_text());
            u.context.kind = Some("subtitle".into());
            u.context.key = Some(cap.tc.to_string());
            u.context.location = Some(format!("line {lineno}"));
            u.context.extra.insert("start".into(), cap.tc.to_string());
            u.context.extra.insert("mode".into(), line.mode.word().into());
            u.context.extra.insert("rows".into(), line.rows.len().to_string());
            u.context.extra.insert("max_cpl".into(), ROW_CELLS.to_string());
            units.push(u);
            spans.push((*s, *e));
        }

        let mut parts = Vec::new();
        let mut cursor = 0;
        for (i, (s, e)) in spans.iter().enumerate() {
            if *s > cursor {
                parts.push(SkelPart::Raw(text[cursor..*s].to_string()));
            }
            parts.push(SkelPart::Unit(i));
            cursor = *e;
        }
        if cursor < text.len() {
            parts.push(SkelPart::Raw(text[cursor..].to_string()));
        }
        let mut warnings = std::mem::take(&mut dec.warnings);
        if skipped > 0 {
            warnings.push(Warning::new(
                "scc_lines_kept_verbatim",
                "",
                format!("{skipped} caption line(s) on data channel 2, with backspaces, or with several captions were kept as they are."),
            ));
        }
        guard::finish(
            Document::new("scc", units, Skeleton::text(parts, dec.encoding.as_str(), parser_info!())).with_options(opts),
            warnings,
            &limits,
        )
    }

    fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
        merge(doc, opts)
    }
}

/// The new text of a changed caption line, or the reason it was refused.
fn rebuild(u: &Unit) -> Result<Option<String>, String> {
    let raw = u.hidden(RAW).ok_or("the unit lost its original line")?;
    let st = u.hidden(STATE)
        .and_then(State::from_text)
        .ok_or("the unit lost its caption state")?;
    let cap = split_caption(raw).ok_or("the original line is not a caption line")?;
    let words = parse_words(cap.rest, 0).map_err(|e| e.to_string())?;
    let mut state = st;
    let line = Line::new(&words, &mut state);
    let src = line.text();
    let new = inlines_to_atoms(&u.output());
    if new == src {
        return Ok(None);
    }
    let words = encode_line(&line, &new)?;
    Ok(Some(format!("{}{}{}{}", cap.lead, cap.tc, cap.sep, words_text(&words))))
}

/// The units (by index) whose rewritten file would not read them back as a channel-1 caption.
///
/// The data channel carries from one line to the next, so a caption that starts with a character before any
/// channel-1 code is read on whatever channel the line before it ended on. A translation that moves or drops an
/// inline code that switches to channel 2 can therefore put its own caption, or the next one, on channel 2, and
/// that caption is no longer a unit (LocOS writes such a file and the caption vanishes). `lines` holds the line
/// number and unit index of every unit line in `out`.
fn lost_on_readback(out: &str, lines: &[(usize, usize)]) -> Vec<(usize, bool)> {
    let spans = split_lines(out);
    let Some(first) = spans.iter().position(|(s, e, _)| !py_strip(&out[*s..*e]).is_empty()) else {
        return lines.iter().map(|(_, u)| (*u, false)).collect();
    };
    let mut state = State::default();
    let mut lost = Vec::new();
    let mut want = lines.iter().peekable();
    for (i, (s, e, _)) in spans.iter().enumerate() {
        if want.peek().is_none() {
            break;
        }
        let content = &out[*s..*e];
        if i <= first || py_strip(content).is_empty() {
            continue;
        }
        let Some(cap) = split_caption(content) else { break };
        let Ok(words) = parse_words(cap.rest, i + 1) else { break };
        let line = Line::new(&words, &mut state);
        if let Some(&&(li, u)) = want.peek() {
            if li == i {
                want.next();
                if !line.unit || line.rows.is_empty() || line.text().is_empty() {
                    let ch2 = line.rows.iter().flat_map(|r| r.cells.iter()).any(|c| c.is_char && c.chan != 1);
                    lost.push((u, ch2));
                }
            }
        }
    }
    // a unit line the scan never reached is lost too
    lost.extend(want.map(|(_, u)| (*u, false)));
    lost
}

fn merge(doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
    check_parser(doc, &parser_info!())?;
    let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
        return Err(Error::Write("scc needs a text skeleton".into()));
    };
    let mut out = String::new();
    let mut problems: Vec<String> = Vec::new();
    // (line number in `out`, unit index) of every unit line, and whether any caption was re-encoded
    let mut unit_lines: Vec<(usize, usize)> = Vec::new();
    let mut lineno = 0usize;
    let mut rewritten = false;
    for p in parts {
        match p {
            SkelPart::Raw(s) => {
                lineno += split_lines(s).len() - 1;
                out.push_str(s);
            }
            SkelPart::Unit(i) => {
                let u = doc.units.get(*i).ok_or_else(|| Error::Write(format!("skeleton points at missing unit {i}")))?;
                let raw = u.hidden(RAW)
                    .ok_or_else(|| Error::Write(format!("unit {} lost its original line", u.id)))?;
                if raw.contains(['\r', '\n']) {
                    return Err(Error::Write(format!("unit {}: its original line holds a line break", u.id)));
                }
                unit_lines.push((lineno, *i));
                let translated = u.translate && u.segments.iter().any(|s| s.has_target());
                if !translated {
                    out.push_str(raw);
                    continue;
                }
                match rebuild(u) {
                    Ok(Some(l)) => {
                        rewritten = true;
                        out.push_str(&l);
                    }
                    Ok(None) => out.push_str(raw),
                    Err(why) => {
                        let tc = split_caption(raw).map(|c| c.tc).unwrap_or_default();
                        problems.push(format!("caption {} at {tc}: {why}", i + 1));
                        out.push_str(raw);
                    }
                }
            }
        }
    }
    if !problems.is_empty() {
        let more = if problems.len() > 10 { format!(" (and {} more)", problems.len() - 10) } else { String::new() };
        return Err(Error::Unwritable {
            unit: None,
            reason: format!(
                "The SCC file was not exported: CEA-608 captions allow only Latin characters from the 608 set and {ROW_CELLS} characters per row. {}{more}. Shorten or transliterate these captions and export again.",
                problems[..problems.len().min(10)].join("; ")
            ),
        });
    }
    if rewritten {
        // lead decision (11-05, the rc precedent): refuse rather than write a caption that would vanish
        let lost = lost_on_readback(&out, &unit_lines);
        if !lost.is_empty() {
            let shown: Vec<String> = lost
                .iter()
                .take(10)
                .map(|(i, ch2)| {
                    let raw = doc.units[*i].hidden(RAW).unwrap_or("");
                    let tc = split_caption(raw).map(|c| c.tc).unwrap_or_default();
                    let why = if *ch2 {
                        "would be read back on data channel 2"
                    } else {
                        "would no longer read back as a caption"
                    };
                    format!("caption {} at {tc}: {why}", i + 1)
                })
                .collect();
            let more = if lost.len() > 10 { format!(" (and {} more)", lost.len() - 10) } else { String::new() };
            return Err(Error::Unwritable {
                unit: None,
                reason: format!(
                    "The SCC file was not exported: after translation these captions would not read back as channel 1 captions and would be lost. {}{more}. An inline code can switch the data channel: keep the inline codes where they were and export again.",
                    shown.join("; ")
                ),
            });
        }
    }
    enc::encode_text_with(&out, encoding, opts)
}

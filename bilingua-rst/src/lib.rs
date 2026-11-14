//! Bilingua reStructuredText filter (`.rst`, `.rest`). See the README.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod blocks;
mod inline;
mod pyu;
mod rx;
mod table;
mod tables;

use bilingua_model::*;
use blocks::{Kind, Slot, Walk};
use pyu::*;

/// The reStructuredText filter.
pub struct Rst;

// Reserved code ids (start with `~`, never a numeric tag id).
const RAW: &str = "raw"; // the source text of the unit's region (a paragraph, a title with its adornments, a table)
const KIND: &str = "kind"; // para, span, title, grid, simple, or cell (a later cell of the table the head unit holds)
const TITLE: &str = "title"; // width, title span and adornment lines of a title, relative to the region
const CELL: &str = "cell"; // the cell's index in its table
const LINE: &str = "line"; // the first line of a table
const COUNT: &str = "n"; // on a table's head unit: how many units belong to the table

fn get<'a>(u: &'a Unit, key: &str) -> Option<&'a str> {
    u.hidden(key)
}

fn trimmed(v: Vec<Inline>) -> Vec<Inline> {
    let mut v = merge_text(v);
    if let Some(Inline::Text(t)) = v.first_mut() {
        *t = lstrip(t).to_string();
    }
    if let Some(Inline::Text(t)) = v.last_mut() {
        *t = rstrip(t).to_string();
    }
    v.retain(|i| !matches!(i, Inline::Text(t) if t.is_empty()));
    v
}

/// The unit's translation when it differs from the source (LocOS: `new.strip() == unit` leaves the original).
fn changed(u: &Unit) -> Option<Vec<Inline>> {
    if !u.translate || u.segments.iter().all(|s| !s.has_target()) {
        return None;
    }
    let new = trimmed(u.output());
    (new != trimmed(u.source())).then_some(new)
}

fn starts_with_token(v: &[Inline]) -> bool {
    for i in v {
        match i {
            Inline::Tag { .. } => return true,
            Inline::Text(t) => {
                let l = lstrip(t);
                if !l.is_empty() {
                    return l.starts_with('\u{27E6}');
                }
            }
        }
    }
    false
}

fn write_title(raw: &str, info: &str, rendered: &str) -> Result<String, Error> {
    let bad = || Error::Write("rst: broken title record".into());
    let nums: Vec<usize> = info.split(',').map(|x| x.parse().map_err(|_| bad())).collect::<Result<_, _>>()?;
    if nums.len() < 3 || !(nums.len() - 3).is_multiple_of(3) {
        return Err(bad());
    }
    let (width, ts, te) = (nums[0], nums[1], nums[2]);
    let new_w = column_width(rendered);
    let delta = new_w as isize - width as isize;
    let mut edits: Vec<(usize, usize, String)> = vec![(ts, te, rendered.to_string())];
    for c in nums[3..].chunks(3) {
        let (a, b) = (c[0], c[1]);
        if b < a || b > raw.len() {
            return Err(bad());
        }
        let ch = char::from_u32(c[2] as u32).ok_or_else(bad)?;
        let len = (new_w as isize).max((b - a) as isize + delta) as usize;
        edits.push((a, b, ch.to_string().repeat(len)));
    }
    edits.sort();
    let mut out = String::with_capacity(raw.len() + 16);
    let mut pos = 0usize;
    for (a, b, rep) in edits {
        if a < pos || b > raw.len() || !raw.is_char_boundary(a) || !raw.is_char_boundary(b) {
            return Err(bad());
        }
        out.push_str(&raw[pos..a]);
        out.push_str(&rep);
        pos = b;
    }
    out.push_str(&raw[pos..]);
    Ok(out)
}

fn write_table(doc: &Document, i: usize, head: &Unit, widen: bool) -> Result<String, Error> {
    let bad = |m: &str| Error::Write(format!("rst: {m}"));
    let raw = get(head, RAW).ok_or_else(|| bad("table without its source"))?;
    let kind = get(head, KIND).unwrap_or("");
    let n: usize = get(head, COUNT).and_then(|x| x.parse().ok()).ok_or_else(|| bad("table without a cell count"))?;
    let line: usize = get(head, LINE).and_then(|x| x.parse().ok()).unwrap_or(1);
    let mut cells: Vec<(usize, String)> = Vec::new();
    for k in 0..n {
        let u = doc.units.get(i + k).ok_or_else(|| bad("table cell unit missing"))?;
        let ci: usize = get(u, CELL).and_then(|x| x.parse().ok()).ok_or_else(|| bad("cell without its index"))?;
        if let Some(new) = changed(u) {
            cells.push((ci, inline::render(&new, &u.codes)));
        }
    }
    if cells.is_empty() {
        return Ok(raw.to_string());
    }
    let ls = blocks::lines(raw);
    // The slot starts at the table's first character, so an indented table has its indent on every line but the
    // first one: read the table without that indent and put it back on the lines written.
    let pad: &str = ls
        .iter()
        .skip(1)
        .filter(|l| !l.s.trim().is_empty())
        .map(|l| &l.s[..l.s.len() - l.s.trim_start_matches([' ', '\t']).len()])
        .min_by_key(|p| p.len())
        .unwrap_or("");
    let strs: Vec<&str> = ls.iter().enumerate().map(|(k, l)| if k > 0 { l.s.strip_prefix(pad).unwrap_or(l.s) } else { l.s }).collect();
    let rendered = if kind == "grid" {
        let mut g = table::Grid::new(&strs);
        if !g.parse().unwrap_or(false) {
            return Err(bad("the table no longer reads as a grid table"));
        }
        let mut nc = Vec::with_capacity(cells.len());
        for (ci, t) in cells {
            nc.push((*g.cells.get(ci).ok_or_else(|| bad("cell index out of range"))?, t));
        }
        g.render(&nc, line)
    } else {
        let mut t = table::Simple::new(&strs);
        if !t.parse() {
            return Err(bad("the table no longer reads as a simple table"));
        }
        if cells.iter().any(|(ci, _)| *ci >= t.cells.len()) {
            return Err(bad("cell index out of range"));
        }
        if widen {
            t.render_widening(&cells, line)
        } else {
            t.render(&cells, line)
        }
    };
    // a translation that does not fit its cell is the translator's to change (`unwritable`, LocOS TableFitError, B-SIDE
    // W6 rst-1); anything else is a broken table record
    let mut lines = rendered.map_err(|m| if m.contains("does not fit its cell") { Error::Unwritable { unit: Some(head.id.clone()), reason: m } } else { Error::Write(m) })?;
    if !pad.is_empty() {
        for l in lines.iter_mut().skip(1).filter(|l| !l.trim().is_empty()) {
            l.insert_str(0, pad);
        }
    }
    let nl = match raw.find(['\r', '\n']) {
        Some(p) if raw[p..].starts_with("\r\n") => "\r\n",
        Some(p) if raw[p..].starts_with('\r') => "\r",
        _ => "\n",
    };
    Ok(lines.join(nl))
}

/// The indentation of the second line of a paragraph `raw` whose first line follows an explicit markup start on
/// the current output line (`.. [x] `, `.. name:: `); None when the paragraph is one line or starts its own line.
fn explicit_body_pad<'a>(out: &str, raw: &'a str) -> Option<&'a str> {
    let line = &out[out.rfind(['\n', '\r']).map_or(0, |p| p + 1)..];
    let lead = line.trim_start_matches([' ', '\t']);
    if !lead.starts_with(".. ") || lead.trim_end_matches([' ', '\t']).len() <= 2 {
        return None;
    }
    let second = &raw[raw.find(['\n', '\r'])?..];
    let second = second.trim_start_matches(['\r', '\n']);
    let pad = &second[..second.len() - second.trim_start_matches([' ', '\t']).len()];
    (!pad.is_empty()).then_some(pad)
}

fn write_unit(doc: &Document, i: usize, out: &mut String, widen: bool) -> Result<(), Error> {
    let u = doc.units.get(i).ok_or_else(|| Error::Write(format!("skeleton points at missing unit {i}")))?;
    let kind = get(u, KIND).unwrap_or("");
    match kind {
        "cell" => Ok(()),
        "grid" | "simple" => {
            out.push_str(&write_table(doc, i, u, widen)?);
            Ok(())
        }
        _ => {
            let raw = get(u, RAW).unwrap_or("");
            let Some(new) = changed(u) else {
                out.push_str(raw);
                return Ok(());
            };
            let mut rendered = inline::render(&new, &u.codes);
            if kind == "title" {
                let info = get(u, TITLE).unwrap_or("");
                out.push_str(&write_title(raw, info, &rendered)?);
                return Ok(());
            }
            if kind == "para" && !starts_with_token(&new) && inline::para_start(&rendered) {
                rendered.insert(0, '\\');
            }
            // native profile: a paragraph that starts on an explicit markup line (`.. [1] text`, `.. note:: text`)
            // and went on over more lines set the body's indentation with its second line. Written as one line on
            // the marker line, a more indented block after it (a literal block) would become the body's own level,
            // so the text moves to its own line at that indentation instead.
            if widen && kind == "para" {
                if let Some(pad) = explicit_body_pad(out, raw) {
                    let keep = out.trim_end_matches([' ', '\t']).len();
                    out.truncate(keep);
                    out.push_str(if raw.contains("\r\n") { "\r\n" } else if raw.contains('\n') { "\n" } else { "\r" });
                    out.push_str(pad);
                }
            }
            out.push_str(&rendered);
            Ok(())
        }
    }
}

/// `enc::encode_text`, except that text which only looks like a charset declaration (`charset=UTF-8` in a meta
/// directive or an example) cannot make a legacy file come out as UTF-8: reStructuredText declares no charset.
///
/// A legacy file that carries C0 controls passes the binary check only while it is long enough (the allowance
/// grows with the length). A merge that shortens it (a paragraph's line breaks and runs of spaces collapse) could
/// write bytes that the same extract refuses as binary, so such output is written as UTF-8 instead, which is what
/// LocOS writes for every translated rst file and which is always read back.
fn write_bytes(text: &str, tag: &str) -> Result<Vec<u8>, Error> {
    let base = enc::base(tag);
    // the model's check (also inside `encode_text`): a shortened legacy merge that would read back as binary is UTF-8
    enc::legacy_output_or_utf8(write_legacy(text, tag, base)?, text, tag)
}

fn write_legacy(text: &str, tag: &str, base: &str) -> Result<Vec<u8>, Error> {
    if matches!(base, "utf-8" | "utf-8-bom" | "utf-16le" | "utf-16be") || base == "iso-8859-1" {
        return enc::encode_text(text, tag);
    }
    let Some(e) = encoding_rs::Encoding::for_label(base.as_bytes()) else {
        return Err(Error::Write(format!("unknown encoding {base}")));
    };
    let nl = if tag.contains(";nl=crlf") {
        "\r\n"
    } else if tag.contains(";nl=cr") {
        "\r"
    } else {
        ""
    };
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let fixed;
    let text = if nl.is_empty() {
        text
    } else {
        let mut o = String::with_capacity(text.len() + text.len() / 16);
        let mut it = text.chars().peekable();
        while let Some(c) = it.next() {
            match c {
                '\r' => {
                    if it.peek() == Some(&'\n') {
                        it.next();
                    }
                    o.push_str(nl);
                }
                '\n' => o.push_str(nl),
                c => o.push(c),
            }
        }
        fixed = o;
        &fixed
    };
    let writer = if e == encoding_rs::GBK { encoding_rs::GB18030 } else { e };
    let (bytes, _, unmappable) = writer.encode(text);
    if unmappable {
        return Ok(text.as_bytes().to_vec());
    }
    Ok(bytes.into_owned())
}

struct Region {
    start: usize,
    end: usize,
    units: Vec<usize>,
}

impl Rst {
    fn unit(
        units: &mut Vec<Unit>,
        raw: &str,
        tok: &inline::Tok,
        kind: &str,
        slot_kind: &str,
        style: Option<&str>,
        line: usize,
    ) -> usize {
        let (inl, codes) = tok.inlines(inline::label_for);
        let mut u = Unit::new(format!("u{}", units.len() + 1), inl);
        u.codes = codes;
        u.set_hidden(KIND, kind);
        if !raw.is_empty() {
            u.set_hidden(RAW, raw);
        }
        u.context.kind = Some(slot_kind.to_string());
        u.context.style = style.map(str::to_string);
        u.context.location = Some(format!("line {line}"));
        units.push(u);
        units.len() - 1
    }
}

impl Filter for Rst {
    fn id(&self) -> &'static str {
        "rst"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["rst", "rest"]
    }

    /// Loose: reStructuredText has no signature. It needs a directive line (`.. name::`), a role (`:ref:`...``) or
    /// a title with an underline in the first 8 KB. Plain text that happens to have an underlined line also passes,
    /// so this belongs late in the sniff order.
    fn sniff(&self, bytes: &[u8]) -> bool {
        let head = &bytes[..bytes.len().min(8192)];
        if head.contains(&0) {
            return false;
        }
        let text = String::from_utf8_lossy(head);
        let ls: Vec<&str> = text.lines().collect();
        for (k, l) in ls.iter().enumerate() {
            if let Some(rest) = l.strip_prefix(".. ") {
                if rx::directive(rest).is_some() {
                    return true;
                }
            }
            if l.contains(":ref:`") || l.contains(":doc:`") || l.contains(":class:`") {
                return true;
            }
            if let Some(next) = ls.get(k + 1) {
                let r = rstrip(next);
                let mut it = r.chars();
                if let Some(c) = it.next() {
                    if "=-~^\"#*+".contains(c) && r.len() >= 3 && it.all(|x| x == c) && !blank(l) && indent(l) == 0 {
                        let r0 = rstrip(l);
                        if r.len() >= column_width(r0) && !r0.starts_with(c) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        let limits = opts.limits();
        let d = enc::decode_text(bytes, &enc::DecodeOptions::new(opts))?;
        let text = d.text.as_str();
        // the LocOS profile keeps LocOS's reading of a backslash that ends a unit (text, golden inline-40)
        let native = !opts.is_profile(profile::LOCOS);
        let ls = blocks::lines(text);
        let mut w = Walk::new(text, limits);
        w.native = native;
        w.walk(&ls, 0)?;

        let mut units: Vec<Unit> = Vec::new();
        let mut regions: Vec<Region> = Vec::new();
        let all: Vec<&Slot> = w.slots.iter().chain(w.meta.iter()).collect();
        for slot in all {
            match slot.kind {
                Kind::Grid | Kind::Simple => {
                    let region = &text[slot.start..slot.end];
                    let kind = if slot.kind == Kind::Grid { "grid" } else { "simple" };
                    let mut idxs: Vec<usize> = Vec::new();
                    for (ci, raw) in slot.cells.iter().enumerate() {
                        let Some(raw) = raw else { continue };
                        let tok = if native { inline::tokenize_native(raw) } else { inline::tokenize(raw) };
                        if !tok.translatable() {
                            continue;
                        }
                        let head = idxs.is_empty();
                        let k = Rst::unit(&mut units, if head { region } else { "" }, &tok, if head { kind } else { "cell" }, "cell", None, w.line_no(slot.start) + 1);
                        units[k].set_hidden(CELL, ci.to_string());
                        idxs.push(k);
                    }
                    if let Some(&h) = idxs.first() {
                        units[h].set_hidden(LINE, slot.line.to_string());
                        units[h].set_hidden(COUNT, idxs.len().to_string());
                        regions.push(Region { start: slot.start, end: slot.end, units: idxs });
                    }
                }
                _ => {
                    let raw = &text[slot.start..slot.end];
                    let tok = if native { inline::tokenize_native(raw) } else { inline::tokenize(raw) };
                    if !tok.translatable() {
                        continue;
                    }
                    let line = w.line_no(slot.start);
                    let (rs, re) = if slot.kind == Kind::Title {
                        let s = slot.adorn.iter().map(|a| a.0).chain([slot.start]).min().unwrap_or(slot.start);
                        let e = slot.adorn.iter().map(|a| a.1).chain([slot.end]).max().unwrap_or(slot.end);
                        (s, e)
                    } else {
                        (slot.start, slot.end)
                    };
                    let (kind, sk) = match slot.kind {
                        Kind::Title => ("title", "title"),
                        Kind::Para => ("para", "paragraph"),
                        _ => ("span", "text"),
                    };
                    let k = Rst::unit(&mut units, &text[rs..re], &tok, kind, sk, slot.style, line);
                    if slot.kind == Kind::Title {
                        let mut info = format!("{},{},{}", slot.width, slot.start - rs, slot.end - rs);
                        for &(a, b, ch) in &slot.adorn {
                            info.push_str(&format!(",{},{},{}", a - rs, b - rs, ch as u32));
                        }
                        units[k].set_hidden(TITLE, info);
                    }
                    regions.push(Region { start: rs, end: re, units: vec![k] });
                }
            }
        }
        regions.sort_by_key(|r| r.start);
        let mut parts: Vec<SkelPart> = Vec::new();
        let mut pos = 0usize;
        for r in &regions {
            if r.start < pos {
                continue;
            }
            if r.start > pos {
                parts.push(SkelPart::Raw(text[pos..r.start].to_string()));
            }
            parts.extend(r.units.iter().map(|&k| SkelPart::Unit(k)));
            pos = r.end;
        }
        if pos < text.len() {
            parts.push(SkelPart::Raw(text[pos..].to_string()));
        }
        let mut warnings = d.warnings;
        for (line, reason) in &w.skipped {
            warnings.push(Warning::new(
                "rst_skipped_table",
                format!("line {line}"),
                format!("The table at line {line} ({reason}) is kept as written and not translated."),
            ));
        }
        let doc = Document::new("rst", units, Skeleton::text(parts, d.encoding, bilingua_model::parser_info!())).with_options(opts);
        guard::finish(doc, warnings, &limits)
    }

    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("rst needs a text skeleton".into()));
        };
        // the native profile widens a simple table column that a translation outgrows; LocOS refuses the merge
        let widen = !opts.is_profile(profile::LOCOS);
        let mut out = String::new();
        for p in parts {
            match p {
                SkelPart::Raw(s) => out.push_str(s),
                SkelPart::Unit(i) => write_unit(doc, *i, &mut out, widen)?,
            }
        }
        match enc::utf8_output(&out, encoding, opts) {
            Some(b) => Ok(b),
            None => write_bytes(&out, encoding),
        }
    }
}

/// Display columns as docutils counts them (East Asian wide and full-width = 2, combining marks = 0).
pub fn display_width(s: &str) -> usize {
    column_width(s)
}


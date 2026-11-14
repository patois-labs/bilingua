//! Comment units under the Localization OS profile (`comments=True`; LocOS's `office_comments.py`, Excel part).
//!
//! The walk: for each worksheet in workbook order its threaded comments, then its classic notes (a note on the same
//! cell as a threaded comment is Excel's placeholder and is skipped). A slot's number is its place in that walk;
//! its unit is numbered `COMMENT_BASE + place`, far above every cell and text-box number, so a workbook read
//! without comments keeps its numbers. Only a slot whose text (trimmed) has a letter or digit is a unit. A classic
//! note's text is the note without Excel's bold `Author:` label run; authors, dates and persons are never units.
//!
//! The write patches the comment parts in place: a threaded comment keeps its leading and trailing white space, a
//! note's translation replaces the first occurrence of the source text after the label (across its runs).

use super::*;

/// LocOS's `COMMENT_BASE`: the first number of a comment unit.
pub const COMMENT_BASE: usize = 1_000_000;
/// LocOS's style for a comment unit.
pub const COMMENT: &str = "comment";
/// `Unit::hidden` key: the unit's place in the comment walk.
pub const SLOT: &str = "xm";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Thread,
    Note,
}

/// One comment of the workbook (`_xlsx_walk`).
pub(super) struct Slot {
    sheet: String,
    cell_ref: String,
    kind: Kind,
    id: String,
    el: usize,
    part: String,
    text: String,
}

/// The text nodes of a classic note (phonetic runs left out) and how many leading ones are Excel's bold
/// `Author:` label (`_note_nodes`).
fn note_nodes(d: &Dom, comment: usize) -> (Vec<usize>, usize) {
    let Some(text_el) = d.element_children(comment).into_iter().find(|&c| d.local(c) == "text") else { return (Vec::new(), 0) };
    let nodes: Vec<usize> = d
        .iter(text_el)
        .into_iter()
        .filter(|&el| d.local(el) == "t" && !d.nodes[el].parent.is_some_and(|p| d.is_el(p) && d.local(p) == "rPh"))
        .collect();
    let mut skip = 0;
    if nodes.len() >= 2 {
        let first = nodes[0];
        let run = d.nodes[first].parent.filter(|&r| d.is_el(r) && d.local(r) == "r");
        let bold = run.is_some_and(|r| d.element_children(r).into_iter().filter(|&x| d.local(x) == "rPr").any(|rpr| d.element_children(rpr).into_iter().any(|x| d.local(x) == "b")));
        if bold && bilingua_model::py_strip(&d.text(first).unwrap_or_default()).ends_with(':') {
            skip = 1;
        }
    }
    (nodes, skip)
}

impl Package {
    /// (sheet name, worksheet part) in workbook order (chartsheets have no cells) (`_xlsx_sheets`).
    fn comment_sheets(&mut self) -> Result<Vec<(String, String)>, Error> {
        let wb = self.workbook.clone();
        let wrels = self.rels(&wb)?;
        if !self.load(&wb)? {
            return Ok(Vec::new());
        }
        let d = &self.roots[&wb];
        let mut out = Vec::new();
        for el in d.iter(d.root) {
            if d.local(el) != "sheet" {
                continue;
            }
            // the first attribute in a namespace named `id` (`r:id`)
            let rid = d.nodes[el].attrs.iter().find(|(k, _)| !k.starts_with("xmlns") && k.split_once(':').is_some_and(|(_, l)| l == "id")).map(|(_, v)| v.as_str());
            let Some((_, kind, path)) = rid.and_then(|r| rel_by_id(&wrels, r)) else { continue };
            if kind == "worksheet" && self.names.contains(path) {
                out.push((d.get(el, "name").unwrap_or("").to_string(), path.clone()));
            }
        }
        Ok(out)
    }

    /// Every comment slot of the workbook, in the order the units are numbered (`_xlsx_walk`).
    pub(super) fn comment_walk(&mut self) -> Result<Vec<Slot>, Error> {
        let mut slots = Vec::new();
        for (name, sheet_part) in self.comment_sheets()? {
            let rels = self.rels(&sheet_part)?;
            let mut threaded: HashSet<String> = HashSet::new();
            for (_, kind, path) in &rels {
                if kind != "threadedComment" || !self.load(path)? {
                    continue;
                }
                let d = &self.roots[path];
                for tc in d.element_children(d.root) {
                    if d.local(tc) != "threadedComment" {
                        continue;
                    }
                    let r = d.get(tc, "ref").unwrap_or("").to_string();
                    threaded.insert(r.to_uppercase());
                    let Some(t) = d.element_children(tc).into_iter().find(|&c| d.local(c) == "text") else { continue };
                    slots.push(Slot {
                        sheet: name.clone(),
                        cell_ref: r,
                        kind: Kind::Thread,
                        id: d.get(tc, "id").unwrap_or("").to_string(),
                        el: t,
                        part: path.clone(),
                        text: d.text(t).unwrap_or_default(),
                    });
                }
            }
            for (_, kind, path) in &rels {
                if kind != "comments" || !self.load(path)? {
                    continue;
                }
                let d = &self.roots[path];
                for cm in d.iter(d.root) {
                    if d.local(cm) != "comment" {
                        continue;
                    }
                    let r = d.get(cm, "ref").unwrap_or("").to_string();
                    if threaded.contains(&r.to_uppercase()) {
                        continue;
                    }
                    let (nodes, skip) = note_nodes(d, cm);
                    let body: String = nodes[skip..].iter().map(|&n| d.text(n).unwrap_or_default()).collect();
                    slots.push(Slot { sheet: name.clone(), cell_ref: r, kind: Kind::Note, id: String::new(), el: cm, part: path.clone(), text: body });
                }
            }
        }
        Ok(slots)
    }
}

/// The comment units of a workbook (`xlsx_comment_slots`).
pub(super) fn units(pkg: &mut Package) -> Result<Vec<Unit>, Error> {
    let mut out = Vec::new();
    for (j, slot) in pkg.comment_walk()?.into_iter().enumerate() {
        let core = bilingua_model::py_strip(&slot.text);
        if !bilingua_model::py_has_alnum(core) {
            continue;
        }
        let key = COMMENT_BASE + j;
        let mut u = Unit::new(key.to_string(), vec![Inline::text(core)]);
        u.context = UnitContext::new().with_kind(COMMENT).with_key(slot.cell_ref.clone()).with_location(slot.sheet.clone());
        u.context.extra.insert(profile::P_INDEX.to_string(), key.to_string());
        u.context.extra.insert(profile::STYLE.to_string(), COMMENT.to_string());
        u.hidden.insert(KEY.to_string(), key.to_string());
        u.hidden.insert(SLOT.to_string(), j.to_string());
        out.push(u);
    }
    Ok(out)
}

/// Replaces characters `[start, end)` of the concatenated text of `nodes` with `new`: the first node the span
/// touches takes the new text, later ones lose their part of the span (`_replace_span`).
fn replace_span(d: &mut Dom, nodes: &[usize], start: usize, end: usize, new: &str) {
    let mut pos = 0;
    let mut placed = false;
    for &node in nodes {
        let text: Vec<char> = d.text(node).unwrap_or_default().chars().collect();
        let (a, b) = (pos, pos + text.len());
        pos = b;
        if !(a < end && b > start) {
            continue;
        }
        let lo = start.max(a) - a;
        let hi = end.min(b) - a;
        let head: String = text[..lo].iter().collect();
        let tail: String = text[hi..].iter().collect();
        let joined = if placed { format!("{head}{tail}") } else { format!("{head}{new}{tail}") };
        placed = true;
        d.set_text(node, &joined);
        d.set(node, "xml:space", "preserve");
    }
}

/// Writes `new` over `core` in a classic note (`_write_note`); false when the note does not hold it.
fn write_note(d: &mut Dom, comment: usize, core: &str, new: &str) -> bool {
    let (nodes, skip) = note_nodes(d, comment);
    if nodes.is_empty() {
        return false;
    }
    let texts: Vec<Vec<char>> = nodes.iter().map(|&n| d.text(n).unwrap_or_default().chars().collect()).collect();
    let full: Vec<char> = texts.iter().flatten().copied().collect();
    let after: usize = texts[..skip].iter().map(Vec::len).sum();
    let needle: Vec<char> = core.chars().collect();
    let Some(i) = (after..=full.len().saturating_sub(needle.len())).find(|&i| i + needle.len() <= full.len() && full[i..i + needle.len()] == needle[..]) else { return false };
    replace_span(d, &nodes, i, i + needle.len(), new);
    true
}

/// Writes the comment translations (keys from `COMMENT_BASE` up) into the package (`xlsx_apply`): a threaded
/// comment is found by its id, a note by its sheet and cell; one the package does not have stays as it is.
pub(super) fn apply(pkg: &mut Package, texts: &BTreeMap<usize, String>) -> Result<(), Error> {
    let mut wanted: Vec<(usize, String)> = texts.iter().filter(|(k, _)| **k >= COMMENT_BASE).map(|(k, v)| (*k - COMMENT_BASE, xml_target_text(v))).collect();
    if wanted.is_empty() {
        return Ok(());
    }
    let slots = pkg.comment_walk()?;
    wanted.retain(|(j, _)| *j < slots.len());
    if wanted.is_empty() {
        return Ok(());
    }
    // the last slot with a key wins, as in a Python dict
    let mut notes: HashMap<(String, String), usize> = HashMap::new();
    let mut threads: HashMap<String, usize> = HashMap::new();
    for (n, s) in slots.iter().enumerate() {
        match s.kind {
            Kind::Note => {
                notes.insert((s.sheet.clone(), s.cell_ref.to_uppercase()), n);
            }
            Kind::Thread if !s.id.is_empty() => {
                threads.insert(s.id.clone(), n);
            }
            Kind::Thread => {}
        }
    }
    let mut touched: BTreeSet<String> = BTreeSet::new();
    for (j, new) in wanted {
        let s = &slots[j];
        let core = bilingua_model::py_strip(&s.text);
        if core.is_empty() || new == core {
            continue;
        }
        match s.kind {
            Kind::Thread => {
                let Some(&n) = threads.get(&s.id) else { continue };
                let t = &slots[n];
                let d = pkg.dom(&t.part);
                let text = d.text(t.el).unwrap_or_default();
                let lead = &text[..text.len() - bilingua_model::py_lstrip(&text).len()];
                let trail = &text[bilingua_model::py_rstrip(&text).len()..];
                let joined = format!("{lead}{new}{trail}");
                d.set_text(t.el, &joined);
                touched.insert(t.part.clone());
            }
            Kind::Note => {
                let Some(&n) = notes.get(&(s.sheet.clone(), s.cell_ref.to_uppercase())) else { continue };
                let t = &slots[n];
                if write_note(pkg.dom(&t.part), t.el, core, &new) {
                    touched.insert(t.part.clone());
                }
            }
        }
    }
    pkg.dirty.extend(touched);
    Ok(())
}

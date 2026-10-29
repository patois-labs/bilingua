//! XLIFF 2.1 writer and target reader for the CLI.
//!
//! The writer emits one `<unit>` per Bilingua unit, one `<segment>` per segment, and `<ignorable>` for the text
//! between segments. Paired codes are `<pc>` when they nest inside the segment, `<sc>`/`<ec>` when they overlap
//! or are unpaired in it; standalone codes are `<ph>`. The reader only needs the targets back.

use bilingua::*;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            c if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') => o.push('\u{fffd}'),
            c => o.push(c),
        }
    }
    o
}

fn nests(inl: &[Inline]) -> bool {
    let mut stack: Vec<&str> = Vec::new();
    for i in inl {
        if let Inline::Tag { id, kind, .. } = i {
            match kind {
                TagKind::Open => stack.push(id),
                TagKind::Close => {
                    if stack.pop() != Some(id.as_str()) {
                        return false;
                    }
                }
                TagKind::Standalone => {}
            }
        }
    }
    stack.is_empty()
}

/// Inline content as XLIFF markup.
pub fn inline_xml(inl: &[Inline]) -> String {
    let pc_mode = nests(inl);
    let mut out = String::new();
    for i in inl {
        match i {
            Inline::Text(t) => out.push_str(&esc(t)),
            Inline::Tag { id, kind, display, .. } => match (kind, pc_mode) {
                (TagKind::Standalone, _) => out.push_str(&format!("<ph id=\"{}\" disp=\"{}\"/>", esc(id), esc(display))),
                (TagKind::Open, true) => {
                    out.push_str(&format!("<pc id=\"{}\" dispStart=\"{}\"", esc(id), esc(display)));
                    // dispEnd is known only at the close tag: write a placeholder and patch it there
                    out.push_str(" dispEnd=\"\u{0}\">");
                }
                (TagKind::Close, true) => {
                    out.push_str("</pc>");
                    // nested pairs close innermost first, so the last unpatched placeholder is this pair's
                    if let Some(pos) = out.rfind("dispEnd=\"\u{0}\"") {
                        out.replace_range(pos..pos + "dispEnd=\"\u{0}\"".len(), &format!("dispEnd=\"{}\"", esc(display)));
                    }
                }
                (TagKind::Open, false) => out.push_str(&format!("<sc id=\"{}\" disp=\"{}\"/>", esc(id), esc(display))),
                (TagKind::Close, false) => out.push_str(&format!("<ec startRef=\"{}\" disp=\"{}\"/>", esc(id), esc(display))),
            },
        }
    }
    out
}

fn state_name(s: SegState) -> &'static str {
    match s {
        SegState::New | SegState::Draft | SegState::Rejected => "initial",
        SegState::Translated => "translated",
        SegState::Reviewed => "reviewed",
        SegState::Confirmed | SegState::SignedOff | SegState::Locked => "final",
    }
}

pub fn write(doc: &Document, original: &str, src: &str, trg: Option<&str>) -> String {
    let mut x = String::new();
    x.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    x.push_str(&format!(
        "<xliff xmlns=\"urn:oasis:names:tc:xliff:document:2.0\" version=\"2.1\" srcLang=\"{}\"{}>\n",
        esc(src),
        trg.map(|t| format!(" trgLang=\"{}\"", esc(t))).unwrap_or_default()
    ));
    x.push_str(&format!("  <file id=\"f1\" original=\"{}\">\n", esc(original)));
    for u in &doc.units {
        let mut attrs = format!("id=\"{}\"", esc(&u.id));
        if let Some(k) = &u.context.key {
            attrs.push_str(&format!(" name=\"{}\"", esc(k)));
        }
        if !u.translate {
            attrs.push_str(" translate=\"no\"");
        }
        x.push_str(&format!("    <unit {attrs}>\n"));
        for (n, s) in u.segments.iter().enumerate() {
            if n > 0 {
                let sep = u.separators.get(n - 1).cloned().unwrap_or_default();
                x.push_str(&format!("      <ignorable><source xml:space=\"preserve\">{}</source></ignorable>\n", esc(&sep)));
            }
            x.push_str(&format!("      <segment id=\"{}\" state=\"{}\">\n", esc(&s.id), state_name(s.state)));
            x.push_str(&format!("        <source xml:space=\"preserve\">{}</source>\n", inline_xml(&s.source)));
            if !s.target.is_empty() {
                x.push_str(&format!("        <target xml:space=\"preserve\">{}</target>\n", inline_xml(&s.target)));
            }
            x.push_str("      </segment>\n");
        }
        x.push_str("    </unit>\n");
    }
    x.push_str("  </file>\n</xliff>\n");
    x
}

pub struct Read {
    pub target_lang: Option<String>,
    /// segment id -> (target, state)
    pub targets: Vec<(String, Vec<Inline>, SegState)>,
}

fn attr(e: &BytesStart<'_>, name: &str) -> Option<String> {
    e.attributes().flatten().find(|a| a.key.as_ref() == name.as_bytes()).and_then(|a| a.unescape_value().ok().map(|v| v.into_owned()))
}

/// Reads the targets from an XLIFF 2 file. `doc` supplies tag displays by looking the segment's source up.
pub fn read(xml: &str, doc: &Document) -> Result<Read, String> {
    let mut reader = Reader::from_str(xml);
    let mut res = Read { target_lang: None, targets: Vec::new() };
    let mut seg: Option<(String, SegState)> = None;
    let mut in_target = false;
    let mut in_source = false;
    let mut cur: Vec<Inline> = Vec::new();
    let mut text = String::new();
    let mut pc_stack: Vec<String> = Vec::new();
    let mut has_target = false;
    let lookup = |seg_id: &str, id: &str, kind: TagKind| -> String {
        doc.find(seg_id)
            .and_then(|(_, s)| {
                s.source.iter().find_map(|i| match i {
                    Inline::Tag { id: tid, kind: k, display, .. } if tid == id && *k == kind => Some(display.clone()),
                    _ => None,
                })
            })
            .unwrap_or_default()
    };
    let is_free = |seg_id: &str, id: &str| -> bool {
        doc.find(seg_id).is_some_and(|(_, s)| s.source.iter().any(|i| matches!(i, Inline::Tag { id: tid, free: true, .. } if tid == id)))
    };
    loop {
        let ev = reader.read_event().map_err(|e| format!("invalid XLIFF: {e}"))?;
        match ev {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(ev, Event::Empty(_));
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                match name.as_str() {
                    "xliff" => res.target_lang = attr(e, "trgLang"),
                    "segment" => {
                        let id = attr(e, "id").ok_or("a <segment> has no id")?;
                        let st = match attr(e, "state").as_deref() {
                            Some("translated") => SegState::Translated,
                            Some("reviewed") => SegState::Reviewed,
                            Some("final") => SegState::Confirmed,
                            _ => SegState::Draft,
                        };
                        seg = Some((id, st));
                        has_target = false;
                    }
                    "source" => in_source = !empty,
                    "target" if seg.is_some() => {
                        in_target = !empty;
                        has_target = true;
                        cur.clear();
                        text.clear();
                        pc_stack.clear();
                    }
                    "pc" | "sc" | "ph" | "ec" if in_target => {
                        if !text.is_empty() {
                            cur.push(Inline::Text(std::mem::take(&mut text)));
                        }
                        let sid = seg.as_ref().map(|s| s.0.clone()).unwrap_or_default();
                        match name.as_str() {
                            "pc" | "sc" => {
                                let id = attr(e, "id").ok_or("a code has no id")?;
                                let display = lookup(&sid, &id, TagKind::Open);
                                cur.push(Inline::tag(id.clone(), TagKind::Open, display));
                                if name == "pc" && !empty {
                                    pc_stack.push(id);
                                } else if name == "pc" {
                                    // <pc/> with no content: close right away
                                    let display = lookup(&sid, &id, TagKind::Close);
                                    cur.push(Inline::tag(id, TagKind::Close, display));
                                }
                            }
                            "ec" => {
                                let id = attr(e, "startRef").or_else(|| attr(e, "id")).ok_or("an <ec> has no startRef")?;
                                let display = lookup(&sid, &id, TagKind::Close);
                                cur.push(Inline::tag(id, TagKind::Close, display));
                            }
                            _ => {
                                let id = attr(e, "id").ok_or("a code has no id")?;
                                let display = lookup(&sid, &id, TagKind::Standalone);
                                let free = is_free(&sid, &id);
                                cur.push(Inline::Tag { id, kind: TagKind::Standalone, display, free });
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::End(ref e) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).to_string();
                match name.as_str() {
                    "pc" if in_target => {
                        if !text.is_empty() {
                            cur.push(Inline::Text(std::mem::take(&mut text)));
                        }
                        let id = pc_stack.pop().ok_or("unbalanced <pc>")?;
                        let sid = seg.as_ref().map(|s| s.0.clone()).unwrap_or_default();
                        let display = lookup(&sid, &id, TagKind::Close);
                        cur.push(Inline::tag(id, TagKind::Close, display));
                    }
                    "source" => in_source = false,
                    "target" if in_target => {
                        in_target = false;
                        if !text.is_empty() {
                            cur.push(Inline::Text(std::mem::take(&mut text)));
                        }
                    }
                    "segment" => {
                        if let Some((id, st)) = seg.take() {
                            if has_target && !cur.is_empty() {
                                res.targets.push((id, merge_text(std::mem::take(&mut cur)), st));
                            }
                        }
                        cur.clear();
                    }
                    _ => {}
                }
            }
            Event::Text(ref t) if in_target && !in_source => {
                text.push_str(&t.decode().map_err(|e| format!("invalid XLIFF: {e}"))?);
            }
            Event::CData(ref c) if in_target => text.push_str(&c.decode().map_err(|e| format!("invalid XLIFF: {e}"))?),
            Event::GeneralRef(ref r) if in_target => {
                if let Some(c) = r.resolve_char_ref().map_err(|e| format!("invalid XLIFF: {e}"))? {
                    text.push(c);
                } else {
                    match r.decode().map_err(|e| format!("invalid XLIFF: {e}"))?.as_ref() {
                        "lt" => text.push('<'),
                        "gt" => text.push('>'),
                        "amp" => text.push('&'),
                        "quot" => text.push('"'),
                        "apos" => text.push('\''),
                        other => return Err(format!("invalid XLIFF: unknown entity &{other};")),
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(res)
}

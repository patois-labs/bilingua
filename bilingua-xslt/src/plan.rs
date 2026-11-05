//! Decides which text of a stylesheet is translatable: literal result text, `xsl:text`, the text-bearing
//! instructions and a few attributes. Text units come first in document order, then attribute units.

use crate::lex::*;
use crate::Limits;
use bilingua_model::{guard, Error};

#[derive(Debug, Clone, Copy)]
pub struct Ctx {
    pub text_ok: bool,
    pub tvt: bool,
    pub preserve: bool,
}

/// xsl:* elements whose direct text children are written to the output as text.
fn text_context(local: &str) -> bool {
    matches!(
        local,
        "break"
            | "catch"
            | "copy"
            | "document"
            | "element"
            | "fallback"
            | "for-each"
            | "for-each-group"
            | "if"
            | "iterate"
            | "matching-substring"
            | "merge-action"
            | "non-matching-substring"
            | "on-completion"
            | "on-empty"
            | "on-non-empty"
            | "otherwise"
            | "perform-sort"
            | "result-document"
            | "template"
            | "text"
            | "try"
            | "when"
            | "where-populated"
    )
}

/// The context for `el`'s content, or `None` when nothing under `el` is translated.
pub fn child_ctx(el: &El, ctx: Ctx) -> Option<Ctx> {
    if el.skip {
        return None;
    }
    let tvt = el.expand_text.unwrap_or(ctx.tvt);
    let preserve = el.preserve.unwrap_or(ctx.preserve);
    if el.is_xsl {
        return Some(match el.local {
            "text" => Ctx { text_ok: ctx.text_ok, tvt, preserve: true },
            "attribute" => Ctx { text_ok: el.attr_name_ok, tvt, preserve },
            l => Ctx { text_ok: text_context(l), tvt, preserve },
        });
    }
    Some(Ctx { text_ok: true, tvt, preserve })
}

#[derive(Debug, Clone, Copy)]
pub enum Piece {
    Lit { s: usize, e: usize, tvt: bool, preserve: bool },
    Point { s: usize, e: usize },
    Open { s: usize, e: usize, tvt: bool, key: usize },
    Close { s: usize, e: usize, key: usize },
}

impl Piece {
    pub fn span(&self) -> (usize, usize) {
        match *self {
            Piece::Lit { s, e, .. } | Piece::Point { s, e } | Piece::Open { s, e, .. } | Piece::Close { s, e, .. } => (s, e),
        }
    }
}

#[derive(Debug)]
pub struct PUnit {
    /// The attribute's local name for an attribute unit.
    pub attr: Option<String>,
    pub s: usize,
    pub e: usize,
    pub pieces: Vec<Piece>,
    /// Whether `{` and `}` are doubled in the unit's own literal text.
    pub tvt: bool,
    pub crlf: bool,
    pub quote: u8,
    /// A text unit that sits next to a whitespace-only text node the stylesheet strips: (before, after, the
    /// prefix of the `xsl:text` element, e.g. `xsl:`). See `Edge`.
    pub edge: Option<(bool, bool, String)>,
}

fn is_xml_ws(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\r' | '\n')
}

/// The text a translator sees for a literal span: whitespace collapsed (text only), references decoded.
pub fn lit_display(raw: &str, tvt: bool, preserve: bool, attr: bool) -> String {
    let mut s = raw.replace("\r\n", "\n").replace('\r', "\n");
    if attr {
        s = s.replace(['\t', '\n'], " ");
    } else if !preserve {
        s = collapse_ws(&s);
    }
    let mut s = unescape(&s);
    if tvt {
        s = s.replace("{{", "{").replace("}}", "}");
    }
    s
}

/// Every run of space, tab and newline characters that holds a newline becomes one space.
fn collapse_ws(s: &str) -> String {
    if !s.contains('\n') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut run = String::new();
    let mut has_nl = false;
    for c in s.chars() {
        if is_xml_ws(c) {
            has_nl |= c == '\n';
            run.push(c);
        } else {
            if !run.is_empty() {
                if has_nl {
                    out.push(' ');
                } else {
                    out.push_str(&run);
                }
                run.clear();
                has_nl = false;
            }
            out.push(c);
        }
    }
    if !run.is_empty() {
        if has_nl {
            out.push(' ');
        } else {
            out.push_str(&run);
        }
    }
    out
}

/// Index just past the `}` that closes the expression opened at `raw[i] == '{'`.
fn avt_end(raw: &str, i: usize) -> Option<usize> {
    let b = raw.as_bytes();
    let (mut depth, mut j) = (0usize, i);
    while j < b.len() {
        let c = b[j];
        if c == b'"' || c == b'\'' {
            let k = b[j + 1..].iter().position(|x| *x == c)?;
            j += k + 2;
            continue;
        }
        if c == b'{' {
            depth += 1;
        } else if c == b'}' {
            depth -= 1;
            if depth == 0 {
                return Some(j + 1);
            }
        }
        j += 1;
    }
    None
}

/// Splits character data into literal (`false`) and expression (`true`) spans, absolute offsets. With `tvt` off
/// everything is literal. `None` on a malformed value template (the caller then leaves the value whole).
fn split_template(raw: &str, base: usize, tvt: bool) -> Option<Vec<(bool, usize, usize)>> {
    if !tvt {
        return Some(vec![(false, base, base + raw.len())]);
    }
    let b = raw.as_bytes();
    let n = b.len();
    let (mut parts, mut i, mut start) = (Vec::new(), 0usize, 0usize);
    while i < n {
        let c = b[i];
        if (c == b'{' && b.get(i + 1) == Some(&b'{')) || (c == b'}' && b.get(i + 1) == Some(&b'}')) {
            i += 2;
        } else if c == b'{' {
            let end = avt_end(raw, i)?;
            if i > start {
                parts.push((false, base + start, base + i));
            }
            parts.push((true, base + i, base + end));
            i = end;
            start = end;
        } else if c == b'}' {
            return None;
        } else {
            i += 1;
        }
    }
    if n > start {
        parts.push((false, base + start, base + n));
    }
    Some(parts)
}

pub struct Planner<'a> {
    text: &'a str,
    d: &'a Lexed<'a>,
    limits: &'a Limits,
    pub units: Vec<PUnit>,
}

impl<'a> Planner<'a> {
    fn push(&mut self, u: PUnit) -> Result<(), Error> {
        self.units.push(u);
        guard::check_unit_count(self.units.len(), self.limits)
    }

    fn blank(&self, c: &Child) -> bool {
        match c {
            Child::Leaf(l) => match l.kind {
                LeafKind::Comment | LeafKind::Pi => true,
                LeafKind::Text => self.text[l.s..l.e].trim().is_empty(),
                _ => false,
            },
            Child::El(_) => false,
        }
    }

    fn ik(&self, c: &Child) -> Ik {
        match c {
            Child::Leaf(l) => {
                if l.kind == LeafKind::Text {
                    Ik::Text
                } else {
                    Ik::Point
                }
            }
            Child::El(i) => self.d.els[*i].ik,
        }
    }

    fn has_text(&self, nodes: &[Child], ctx: Ctx) -> bool {
        for c in nodes {
            match c {
                Child::Leaf(l) => {
                    if l.kind == LeafKind::Text && !self.text[l.s..l.e].trim().is_empty() {
                        return true;
                    }
                }
                Child::El(i) => {
                    let el = &self.d.els[*i];
                    if el.ik == Ik::Paired {
                        if let Some(cc) = child_ctx(el, ctx) {
                            if cc.text_ok && self.has_text(&el.children, cc) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    pub fn visit(&mut self, el: usize, ctx: Ctx) -> Result<(), Error> {
        let d = self.d;
        let e = &d.els[el];
        if !ctx.text_ok {
            for c in &e.children {
                if let Child::El(ci) = c {
                    if let Some(cc) = child_ctx(&d.els[*ci], ctx) {
                        self.visit(*ci, cc)?;
                    }
                }
            }
            return Ok(());
        }
        let mut run: Vec<Child> = Vec::new();
        // Directly inside an xsl:* instruction a whitespace-only text node is stripped from the stylesheet, so it
        // separates outputs: end the run there.
        let ws_breaks = e.is_xsl && e.local != "text" && !ctx.preserve;
        // The prefix of this element's name, to write an `xsl:text` with when a unit's edge text must stay apart
        // from a stripped whitespace node.
        let name_end = self.text[e.s + 1..e.oe].find(|c: char| c.is_whitespace() || c == '>' || c == '/').map_or(e.oe, |i| e.s + 1 + i);
        let prefix = self.text[e.s + 1..name_end].find(':').map(|i| self.text[e.s + 1..e.s + 2 + i].to_string()).unwrap_or_default();
        let mut before = false; // the node before the run is a stripped whitespace node
        for c in &e.children {
            if ws_breaks && matches!(c, Child::Leaf(l) if l.kind == LeafKind::Text) && self.blank(c) {
                self.run(&run, ctx, ws_breaks.then_some((before, true, prefix.as_str())))?;
                run.clear();
                before = true;
            } else if self.ik(c) == Ik::Break {
                self.run(&run, ctx, ws_breaks.then_some((before, false, prefix.as_str())))?;
                run.clear();
                before = false;
                if let Child::El(ci) = c {
                    if let Some(cc) = child_ctx(&d.els[*ci], ctx) {
                        self.visit(*ci, cc)?;
                    }
                }
            } else {
                run.push(*c);
            }
        }
        self.run(&run, ctx, ws_breaks.then_some((before, false, prefix.as_str())))
    }

    fn trim<'n>(&self, nodes: &'n [Child]) -> &'n [Child] {
        let (mut i, mut j) = (0, nodes.len());
        while i < j && self.blank(&nodes[i]) {
            i += 1;
        }
        while j > i && self.blank(&nodes[j - 1]) {
            j -= 1;
        }
        &nodes[i..j]
    }

    fn run(&mut self, nodes: &[Child], ctx: Ctx, edge: Option<(bool, bool, &str)>) -> Result<(), Error> {
        let d: &'a Lexed<'a> = self.d;
        let mut ctx = ctx;
        let mut edge = edge;
        let mut nodes: Vec<Child> = nodes.to_vec();
        loop {
            nodes = self.trim(&nodes).to_vec();
            if nodes.is_empty() || !self.has_text(&nodes, ctx) {
                return Ok(());
            }
            if let [Child::El(i)] = nodes[..] {
                let el = &d.els[i];
                if el.ik == Ik::Paired {
                    match child_ctx(el, ctx) {
                        Some(inner) if inner.text_ok => {
                            nodes = el.children.clone();
                            ctx = inner;
                            edge = None;
                            continue;
                        }
                        _ => return Ok(()),
                    }
                }
            }
            break;
        }
        let mut pieces: Vec<Piece> = Vec::new();
        self.pieces(&nodes, ctx, &mut pieces);
        let text = self.text;
        // trim edge whitespace from the first and last literal pieces
        while let Some(Piece::Lit { s, e, tvt, preserve }) = pieces.first().copied() {
            let raw = &text[s..e];
            let s2 = s + (raw.len() - raw.trim_start_matches(is_xml_ws).len());
            if s2 >= e {
                pieces.remove(0);
                continue;
            }
            pieces[0] = Piece::Lit { s: s2, e, tvt, preserve };
            break;
        }
        while let Some(Piece::Lit { s, e, tvt, preserve }) = pieces.last().copied() {
            let raw = &text[s..e];
            let e2 = s + raw.trim_end_matches(is_xml_ws).len();
            if e2 <= s {
                pieces.pop();
                continue;
            }
            let k = pieces.len() - 1;
            pieces[k] = Piece::Lit { s, e: e2, tvt, preserve };
            break;
        }
        let (Some(first), Some(last)) = (pieces.first(), pieces.last()) else { return Ok(()) };
        let (s, e) = (first.span().0, last.span().1);
        let crlf = text[s..e].contains("\r\n");
        self.push(PUnit { attr: None, s, e, pieces, tvt: ctx.tvt, crlf, quote: 0, edge: edge.filter(|(b, a, _)| *b || *a).map(|(b, a, p)| (b, a, p.to_string())) })
    }

    fn pieces(&self, nodes: &[Child], ctx: Ctx, out: &mut Vec<Piece>) {
        let d = self.d;
        for c in nodes {
            match c {
                Child::Leaf(l) => {
                    if l.kind != LeafKind::Text {
                        out.push(Piece::Point { s: l.s, e: l.e });
                        continue;
                    }
                    match split_template(&self.text[l.s..l.e], l.s, ctx.tvt) {
                        Some(parts) => {
                            for (expr, s, e) in parts {
                                out.push(if expr {
                                    Piece::Point { s, e }
                                } else {
                                    Piece::Lit { s, e, tvt: ctx.tvt, preserve: ctx.preserve }
                                });
                            }
                        }
                        None => out.push(Piece::Point { s: l.s, e: l.e }), // malformed template: keep it whole
                    }
                }
                Child::El(i) => {
                    let el = &d.els[*i];
                    if el.ik == Ik::Point {
                        out.push(Piece::Point { s: el.s, e: el.e });
                        continue;
                    }
                    let Some(cc) = child_ctx(el, ctx) else {
                        out.push(Piece::Point { s: el.s, e: el.e });
                        continue;
                    };
                    out.push(Piece::Open { s: el.s, e: el.oe, tvt: cc.tvt, key: *i });
                    self.pieces(&el.children, cc, out);
                    out.push(Piece::Close { s: el.cs, e: el.e, key: *i });
                }
            }
        }
    }

    // ---- attribute units
    fn attrs(&mut self, el: usize, ctx: Option<Ctx>) -> Result<(), Error> {
        let Some(ctx) = ctx else { return Ok(()) };
        let d = self.d;
        let e = &d.els[el];
        if !e.is_xsl {
            for a in &e.attrs {
                if a.prefixed || !TRANSLATABLE_ATTRIBUTES.contains(&a.local) {
                    continue;
                }
                self.attr_unit(a)?;
            }
        }
        for c in &e.children {
            if let Child::El(ci) = c {
                self.attrs(*ci, child_ctx(&d.els[*ci], ctx))?;
            }
        }
        Ok(())
    }

    fn attr_unit(&mut self, a: &Attr) -> Result<(), Error> {
        let text = self.text;
        // entity references (declared ones) split the value
        let mut parts: Vec<(bool, usize, usize)> = Vec::new(); // (is entity, s, e)
        let mut pos = a.vs;
        let mut ents: Vec<(usize, usize)> = Vec::new();
        if text[a.vs..a.ve].contains('&') {
            let _ = scan_refs(&text[a.vs..a.ve], a.vs, &self.d.ents, |s, e| ents.push((s, e)));
        }
        for (s, e) in ents {
            parts.push((false, pos, s));
            parts.push((true, s, e));
            pos = e;
        }
        parts.push((false, pos, a.ve));
        let mut pieces: Vec<Piece> = Vec::new();
        for (ent, s, e) in parts {
            if ent {
                pieces.push(Piece::Point { s, e });
                continue;
            }
            let Some(sp) = split_template(&text[s..e], s, true) else { return Ok(()) };
            for (expr, s2, e2) in sp {
                pieces.push(if expr { Piece::Point { s: s2, e: e2 } } else { Piece::Lit { s: s2, e: e2, tvt: true, preserve: false } });
            }
        }
        pieces.retain(|p| {
            let (s, e) = p.span();
            s < e
        });
        let has_letter = pieces.iter().any(|p| matches!(p, Piece::Lit { s, e, .. } if bilingua_model::py_has_alnum(&lit_display(&text[*s..*e], true, false, true))));
        if !has_letter {
            return Ok(());
        }
        self.push(PUnit { attr: Some(a.local.to_string()), s: a.vs, e: a.ve, pieces, tvt: true, crlf: false, quote: a.quote, edge: None })
    }
}

/// Text units (document order), then attribute units (document order). A simplified stylesheet (the root is a
/// literal result element) walks the same way.
pub fn plan(text: &str, d: &Lexed, limits: &Limits) -> Result<Vec<PUnit>, Error> {
    let mut p = Planner { text, d, limits, units: Vec::new() };
    let root = &d.els[d.root];
    let rctx = child_ctx(root, Ctx { text_ok: false, tvt: false, preserve: false });
    if let Some(c) = rctx {
        p.visit(d.root, c)?;
    }
    let mut texts = std::mem::take(&mut p.units);
    p.attrs(d.root, rctx)?;
    texts.append(&mut p.units);
    Ok(texts)
}

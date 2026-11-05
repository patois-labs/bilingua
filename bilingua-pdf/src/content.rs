//! The content-stream interpreter, reduced to what the text layer needs: the graphics and text state, text
//! objects (one per show operator, as PDFium's `CPDF_StreamContentParser::AddTextObject` makes them), forms, and
//! a count of every page object (PDFium's `FPDFPage_CountObjects`, which LocOS caps).

use crate::doc::Doc;
use crate::font::{Font, FontRef, Loader};
use crate::obj::{self, Dict, Lexer, Obj, Parsed, Tok};
use std::collections::HashMap;
use std::rc::Rc;

/// PDFium's `kMaxFormLevel`.
const MAX_FORM_LEVEL: usize = 40;
/// The content parser keeps strings to this length (PDFium's `kMaxStringLength`).
const MAX_STRING: usize = 32767;
/// PDFium keeps the last 16 operands of an operator.
const PARAM_BUF: usize = 16;
/// Saved graphics states kept (`q` deeper than this is counted, not copied: 100 MB of `q` would otherwise be
/// tens of GB of states). PDFium has no cap; no real page nests this deep.
const MAX_SAVED_STATES: usize = 1 << 16;
/// Marked-content levels kept (each level copies the list of the open ones, so an unbounded nesting is
/// quadratic). Deeper `BMC`/`BDC` are counted and their `EMC` matched.
const MAX_MARKS: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matrix {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Default for Matrix {
    fn default() -> Self {
        Matrix { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 }
    }
}

impl Matrix {
    pub fn new(a: f32, b: f32, c: f32, d: f32, e: f32, f: f32) -> Matrix {
        Matrix { a, b, c, d, e, f }
    }
    /// `self` then `o` (PDFium's `CFX_Matrix::operator*` / `Concat`).
    pub fn then(&self, o: &Matrix) -> Matrix {
        Matrix {
            a: self.a * o.a + self.b * o.c,
            b: self.a * o.b + self.b * o.d,
            c: self.c * o.a + self.d * o.c,
            d: self.c * o.b + self.d * o.d,
            e: self.e * o.a + self.f * o.c + o.e,
            f: self.e * o.b + self.f * o.d + o.f,
        }
    }
    pub fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (self.a * x + self.c * y + self.e, self.b * x + self.d * y + self.f)
    }
    /// PDFium's `GetInverse`, with its operation order (the text layer compares positions at float precision).
    pub fn inverse(&self) -> Matrix {
        let i = self.a * self.d - self.b * self.c;
        if i == 0.0 {
            return Matrix::default();
        }
        let j = -i;
        Matrix {
            a: self.d / i,
            b: self.b / j,
            c: self.c / j,
            d: self.a / i,
            e: (self.c * self.f - self.d * self.e) / i,
            f: (self.a * self.f - self.b * self.e) / j,
        }
    }
    /// PDFium's `TransformDistance(float)`.
    pub fn distance(&self, dist: f32) -> f32 {
        let (x, y) = (self.a + self.c, self.b + self.d);
        dist * (x * x + y * y).sqrt() / 2.0f32.sqrt()
    }
    /// PDFium's `TransformXDistance`.
    pub fn x_distance(&self, dx: f32) -> f32 {
        let (x, y) = (self.a * dx, self.b * dx);
        (x * x + y * y).sqrt()
    }
    pub fn rect(&self, r: &Rect) -> Rect {
        let pts = [self.apply(r.left, r.bottom), self.apply(r.left, r.top), self.apply(r.right, r.bottom), self.apply(r.right, r.top)];
        let mut out = Rect { left: pts[0].0, right: pts[0].0, bottom: pts[0].1, top: pts[0].1 };
        for (x, y) in pts {
            out.left = out.left.min(x);
            out.right = out.right.max(x);
            out.bottom = out.bottom.min(y);
            out.top = out.top.max(y);
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
    pub top: f32,
}

impl Rect {
    pub fn width(&self) -> f32 {
        self.right - self.left
    }
    pub fn height(&self) -> f32 {
        self.top - self.bottom
    }
    pub fn is_empty(&self) -> bool {
        self.left >= self.right || self.bottom >= self.top
    }
    pub fn normalized(&self) -> Rect {
        Rect { left: self.left.min(self.right), right: self.left.max(self.right), bottom: self.bottom.min(self.top), top: self.bottom.max(self.top) }
    }
    pub fn intersect(&self, o: &Rect) -> Rect {
        let n = self.normalized();
        let m = o.normalized();
        let r = Rect { left: n.left.max(m.left), right: n.right.min(m.right), bottom: n.bottom.max(m.bottom), top: n.top.min(m.top) };
        if r.left > r.right || r.bottom > r.top {
            Rect::default()
        } else {
            r
        }
    }
    pub fn union(&self, o: &Rect) -> Rect {
        let n = self.normalized();
        let m = o.normalized();
        Rect { left: n.left.min(m.left), right: n.right.max(m.right), bottom: n.bottom.min(m.bottom), top: n.top.max(m.top) }
    }
    pub fn contains(&self, x: f32, y: f32) -> bool {
        let n = self.normalized();
        x <= n.right && x >= n.left && y <= n.top && y >= n.bottom
    }
}

/// One marked-content level: the identity of its property dictionary and its `/ActualText`.
#[derive(Debug, Clone)]
pub struct Mark {
    pub param: Option<usize>,
    pub actual_text: Option<Vec<u16>>,
}

#[derive(Debug)]
pub struct TextObj {
    pub font: FontRef,
    pub size: f32,
    /// The text matrix without translation (PDFium's text state matrix: horizontal scale, Tm, CTM).
    pub m: Matrix,
    pub pos: (f32, f32),
    pub codes: Vec<u32>,
    pub kernings: Vec<f32>,
    pub positions: Vec<f32>,
    pub rect: Rect,
    pub char_space: f32,
    pub marks: Rc<Vec<Mark>>,
}

impl TextObj {
    /// The text matrix with the object's position as translation (`GetTextMatrix`).
    pub fn text_matrix(&self) -> Matrix {
        Matrix { e: self.pos.0, f: self.pos.1, ..self.m }
    }
    /// PDFium's `GetFontSizeH`.
    pub fn font_size_h(&self) -> f32 {
        ((self.m.a * self.m.a + self.m.c * self.m.c).sqrt() * self.size).abs()
    }
    /// The origin of item `i` in the object's own text space (`GetItemInfo`).
    pub fn item_origin(&self, i: usize) -> (f32, f32) {
        let x = self.positions[i];
        if self.font.is_vertical() {
            let (_w, vx, vy) = self.font.vert(self.codes[i]);
            return (-self.size * vx as f32 / 1000.0, x - self.size * vy as f32 / 1000.0);
        }
        (x, 0.0)
    }
    /// `CPDF_TextObject::GetCharWidth`.
    pub fn char_width(&self, code: u32) -> f32 {
        if self.font.is_vertical() {
            return self.font.vert(code).0 as f32 * self.size / 1000.0;
        }
        self.font.width(code) as f32 * self.size / 1000.0
    }
}

pub enum PageObj {
    Text(Rc<TextObj>),
    Form(Matrix, Vec<PageObj>),
    Other,
}

#[derive(Clone)]
struct State {
    ctm: Matrix,
    tm: Matrix,
    line_pos: (f32, f32),
    text_pos: (f32, f32),
    font: Option<FontRef>,
    size: f32,
    char_space: f32,
    word_space: f32,
    horz: f32,
    leading: f32,
    rise: f32,
    render: i64,
    line_width: f32,
    text_m: Matrix,
}

impl Default for State {
    fn default() -> Self {
        State {
            ctm: Matrix::default(),
            tm: Matrix::default(),
            line_pos: (0.0, 0.0),
            text_pos: (0.0, 0.0),
            font: None,
            size: 0.0,
            char_space: 0.0,
            word_space: 0.0,
            horz: 1.0,
            leading: 0.0,
            rise: 0.0,
            render: 0,
            line_width: 1.0,
            text_m: Matrix::default(),
        }
    }
}

impl State {
    fn update_text_matrix(&mut self) {
        let h = Matrix::new(self.horz, 0.0, 0.0, 1.0, 0.0, 0.0);
        self.text_m = h.then(&self.tm).then(&self.ctm);
    }
    fn text_position(&self) -> (f32, f32) {
        let p = self.tm.apply(self.text_pos.0, self.text_pos.1 + self.rise);
        self.ctm.apply(p.0, p.1)
    }
}

/// Fonts loaded once per dictionary, shared by the pages of one document.
#[derive(Default)]
pub struct FontCache {
    by_key: HashMap<usize, Option<FontRef>>,
    stock: Option<FontRef>,
    next_id: usize,
}

pub struct Interp<'d, 'a> {
    pub doc: &'d Doc<'a>,
    pub fonts: &'d mut FontCache,
    /// Page objects counted so far at the top level (`FPDFPage_CountObjects`).
    pub top_level_count: usize,
    next_mark: usize,
    form_stack: Vec<*const u8>,
    /// Text objects made so far (all levels), to stop a hostile page early.
    pub text_objects: usize,
    pub max_text_objects: usize,
    type3_depth: usize,
}

impl<'d, 'a> Interp<'d, 'a> {
    pub fn new(doc: &'d Doc<'a>, fonts: &'d mut FontCache, max_text_objects: usize) -> Self {
        Interp { doc, fonts, top_level_count: 0, next_mark: 1, form_stack: Vec::new(), text_objects: 0, max_text_objects, type3_depth: 0 }
    }

    pub fn run_page(&mut self, content: &[u8], resources: Option<Rc<Dict>>) -> Vec<PageObj> {
        let mut out = Vec::new();
        let st = State::default();
        self.run(content, resources.clone(), resources, st, &mut out, 0);
        out
    }

    fn font_for(&mut self, res: &Option<Rc<Dict>>, page_res: &Option<Rc<Dict>>, name: &[u8]) -> Option<FontRef> {
        let entry = find_resource(self.doc, res, page_res, b"Font", name);
        let (key, dict) = match &entry {
            Some((key, Obj::Dict(d))) => (*key, d.clone()),
            Some((key, Obj::Stream(s))) => (*key, Rc::new(s.dict.clone())),
            _ => {
                if self.fonts.stock.is_none() {
                    let mut d = Dict::default();
                    d.insert(b"Type", obj::name(b"Font"));
                    d.insert(b"Subtype", obj::name(b"Type1"));
                    d.insert(b"BaseFont", obj::name(b"Helvetica"));
                    let f = self.load_font(&d);
                    self.fonts.stock = f;
                }
                return self.fonts.stock.clone();
            }
        };
        if let Some(f) = self.fonts.by_key.get(&key) {
            return f.clone();
        }
        let f = self.load_font(&dict);
        self.fonts.by_key.insert(key, f.clone());
        f
    }

    fn load_font(&mut self, d: &Dict) -> Option<FontRef> {
        let doc = self.doc;
        if doc.work.get() == 0 {
            return None;
        }
        let resolve = |o: &Obj| doc.resolve(o);
        let stream = |o: &Obj| match o {
            Obj::Stream(s) => doc.stream_data(s),
            _ => None,
        };
        let depth = self.type3_depth;
        let cap = self.max_text_objects;
        // A Type 3 glyph drawn with text (colour emoji layers): its box is the union of that text's rectangles,
        // read with a fresh interpreter, two levels deep at most.
        let form_box = |data: &[u8], res: Option<Rc<Dict>>| -> Option<[f64; 4]> {
            if depth >= 2 {
                return None;
            }
            let mut fc = FontCache::default();
            let mut it = Interp::new(doc, &mut fc, cap.min(10_000));
            it.type3_depth = depth + 1;
            let objs = it.run_page(data, res);
            let mut b: Option<Rect> = None;
            for o in &objs {
                if let PageObj::Text(t) = o {
                    b = Some(match b {
                        None => t.rect,
                        Some(r) => r.union(&t.rect),
                    });
                }
            }
            b.map(|r| [r.left as f64, r.bottom as f64, r.right as f64, r.top as f64])
        };
        self.fonts.next_id += 1;
        let l = Loader { resolve: &resolve, stream: &stream, form_box: &form_box, work: &doc.work };
        l.load(d, self.fonts.next_id).map(Rc::new)
    }

    #[allow(clippy::too_many_arguments)]
    fn run(&mut self, content: &[u8], res: Option<Rc<Dict>>, page_res: Option<Rc<Dict>>, mut st: State, out: &mut Vec<PageObj>, level: usize) {
        let mut lx = Lexer::new(content);
        lx.max_string = MAX_STRING;
        let mut ops: Vec<Obj> = Vec::new();
        let mut stack: Vec<State> = Vec::new();
        let mut marks: Vec<Mark> = Vec::new();
        let mut marks_rc: Rc<Vec<Mark>> = Rc::new(Vec::new());
        let mut path_pending = false;
        let (mut states_over, mut marks_over) = (0usize, 0usize);
        let top = level == 0;
        loop {
            // A page over its object cap, or a document over its stream budget, stops here: the caller reports
            // the limit. Without the budget check a fan-out of forms runs on with empty data.
            if self.text_objects > self.max_text_objects || self.doc.budget.borrow().exceeded || self.doc.work.get() == 0 {
                return;
            }
            let before = lx.pos;
            let tok = lx.next();
            let w: &[u8] = match tok {
                Tok::Eof => break,
                Tok::Word(w, true) => {
                    push_op(&mut ops, obj::parse_number(w));
                    continue;
                }
                Tok::Name(n) => {
                    push_op(&mut ops, Obj::Name(Rc::new(obj::decode_name(n))));
                    continue;
                }
                Tok::Word(b"true", false) => {
                    push_op(&mut ops, Obj::Bool(true));
                    continue;
                }
                Tok::Word(b"false", false) => {
                    push_op(&mut ops, Obj::Bool(false));
                    continue;
                }
                Tok::Word(b"null", false) => {
                    push_op(&mut ops, Obj::Null);
                    continue;
                }
                Tok::Word(w, false) => w,
                Tok::StrStart => {
                    push_op(&mut ops, Obj::Str(Rc::new(lx.read_string())));
                    continue;
                }
                Tok::HexStart => {
                    push_op(&mut ops, Obj::Str(Rc::new(lx.read_hex())));
                    continue;
                }
                Tok::Open(b'[') | Tok::DictOpen => {
                    lx.pos = before;
                    match obj::parse_obj(&mut lx, 0, false) {
                        Some(Parsed::Obj(o)) => push_op(&mut ops, o),
                        _ => {
                            if lx.pos == before {
                                lx.pos += 1;
                            }
                        }
                    }
                    continue;
                }
                _ => continue,
            };
            let w = w.to_vec();
            let num = |i: usize| -> f32 { ops.len().checked_sub(i + 1).and_then(|k| ops[k].numf()).unwrap_or(0.0) };
            match w.as_slice() {
                b"q" => {
                    if stack.len() < MAX_SAVED_STATES {
                        stack.push(st.clone());
                    } else {
                        states_over += 1;
                    }
                }
                b"Q" => {
                    if states_over > 0 {
                        states_over -= 1;
                    } else if let Some(s) = stack.pop() {
                        st = s;
                    }
                }
                b"cm" => {
                    let m = Matrix::new(num(5), num(4), num(3), num(2), num(1), num(0));
                    st.ctm = m.then(&st.ctm);
                    st.update_text_matrix();
                }
                b"BT" => {
                    st.tm = Matrix::default();
                    st.update_text_matrix();
                    st.line_pos = (0.0, 0.0);
                    st.text_pos = (0.0, 0.0);
                }
                b"ET" => {}
                b"Tc" => st.char_space = num(0),
                b"Tw" => st.word_space = num(0),
                b"Tz" => {
                    if ops.len() == 1 {
                        st.horz = num(0) / 100.0;
                        st.update_text_matrix();
                    }
                }
                b"TL" => st.leading = num(0),
                b"Ts" => st.rise = num(0),
                b"Tr" => {
                    let m = ops.last().and_then(|o| o.int()).unwrap_or(0);
                    if (0..=7).contains(&m) {
                        st.render = m;
                    }
                }
                b"w" => st.line_width = num(0),
                b"Tf" => {
                    st.size = num(0);
                    let name = ops.len().checked_sub(2).and_then(|k| ops[k].as_name().map(|n| n.to_vec())).unwrap_or_default();
                    if let Some(f) = self.font_for(&res, &page_res, &name) {
                        st.font = Some(f);
                    }
                }
                b"gs" => {
                    let name = ops.last().and_then(|o| o.as_name().map(|n| n.to_vec())).unwrap_or_default();
                    if let Some((_, Obj::Dict(gs))) = find_resource(self.doc, &res, &page_res, b"ExtGState", &name) {
                        if let Some(Obj::Array(fa)) = gs.get(b"Font").map(|o| self.doc.resolve(o)) {
                            st.size = fa.get(1).map(|o| self.doc.resolve(o)).and_then(|o| o.numf()).unwrap_or(0.0);
                            let fname = fa.first().map(|o| self.doc.resolve(o));
                            // PDFium looks the font up by name only when the array names one.
                            if let Some(Obj::Name(n)) = fname {
                                if let Some(f) = self.font_for(&res, &page_res, &n) {
                                    st.font = Some(f);
                                }
                            }
                        }
                        if let Some(lw) = gs.get(b"LW").map(|o| self.doc.resolve(o)).and_then(|o| o.numf()) {
                            st.line_width = lw;
                        }
                    }
                }
                b"Td" => {
                    st.line_pos.0 += num(1);
                    st.line_pos.1 += num(0);
                    st.text_pos = st.line_pos;
                }
                b"TD" => {
                    st.line_pos.0 += num(1);
                    st.line_pos.1 += num(0);
                    st.text_pos = st.line_pos;
                    st.leading = -num(0);
                }
                b"Tm" => {
                    st.tm = Matrix::new(num(5), num(4), num(3), num(2), num(1), num(0));
                    st.update_text_matrix();
                    st.line_pos = (0.0, 0.0);
                    st.text_pos = (0.0, 0.0);
                }
                b"T*" => {
                    st.line_pos.1 -= st.leading;
                    st.text_pos = st.line_pos;
                }
                b"Tj" => {
                    if let Some(Obj::Str(s)) = ops.last() {
                        if !s.is_empty() {
                            let s = s.to_vec();
                            self.add_text(&mut st, &[s], &[0.0], 0.0, &marks_rc, out, top);
                        }
                    }
                }
                b"'" => {
                    st.line_pos.1 -= st.leading;
                    st.text_pos = st.line_pos;
                    if let Some(Obj::Str(s)) = ops.last() {
                        if !s.is_empty() {
                            let s = s.to_vec();
                            self.add_text(&mut st, &[s], &[0.0], 0.0, &marks_rc, out, top);
                        }
                    }
                }
                b"\"" => {
                    st.word_space = num(2);
                    st.char_space = num(1);
                    st.line_pos.1 -= st.leading;
                    st.text_pos = st.line_pos;
                    if let Some(Obj::Str(s)) = ops.last() {
                        if !s.is_empty() {
                            let s = s.to_vec();
                            self.add_text(&mut st, &[s], &[0.0], 0.0, &marks_rc, out, top);
                        }
                    }
                }
                b"TJ" => {
                    if let Some(Obj::Array(a)) = ops.last() {
                        let a = a.clone();
                        let mut strs: Vec<Vec<u8>> = Vec::new();
                        let mut kerns: Vec<f32> = Vec::new();
                        let mut init = 0.0;
                        let any_str = a.iter().any(|o| matches!(self.doc.resolve(o), Obj::Str(_)));
                        if !any_str {
                            for o in a.iter() {
                                let k = self.doc.resolve(o).numf().unwrap_or(0.0);
                                if k != 0.0 {
                                    st.text_pos.0 -= k * st.size / 1000.0 * st.horz;
                                }
                            }
                        } else {
                            for o in a.iter() {
                                match self.doc.resolve(o) {
                                    Obj::Str(s) => {
                                        if s.is_empty() {
                                            continue;
                                        }
                                        strs.push(s.to_vec());
                                        kerns.push(0.0);
                                    }
                                    other => {
                                        let n = other.numf().unwrap_or(0.0);
                                        if strs.is_empty() {
                                            init += n;
                                        } else if let Some(k) = kerns.last_mut() {
                                            *k += n;
                                        }
                                    }
                                }
                            }
                            self.add_text(&mut st, &strs, &kerns, init, &marks_rc, out, top);
                        }
                    }
                }
                b"Do" => {
                    let name = ops.last().and_then(|o| o.as_name().map(|n| n.to_vec())).unwrap_or_default();
                    if let Some((_, Obj::Stream(x))) = find_resource(self.doc, &res, &page_res, b"XObject", &name) {
                        let sub = x.dict.get(b"Subtype").map(|o| self.doc.resolve(o));
                        match sub.as_ref().and_then(|o| o.as_name()) {
                            Some(b"Form") => {
                                let key = Rc::as_ptr(&x) as *const u8;
                                if self.form_stack.len() <= MAX_FORM_LEVEL && !self.form_stack.contains(&key) {
                                    let data = self.doc.stream_data(&x).unwrap_or_default();
                                    let form_res = match x.dict.get(b"Resources").map(|o| self.doc.resolve(o)) {
                                        Some(Obj::Dict(d)) => Some(d),
                                        _ => res.clone().or(page_res.clone()),
                                    };
                                    let fm = match x.dict.get(b"Matrix").map(|o| self.doc.resolve(o)) {
                                        Some(Obj::Array(a)) if a.len() >= 6 => {
                                            let v: Vec<f32> = a.iter().take(6).map(|o| self.doc.resolve(o).numf().unwrap_or(0.0)).collect();
                                            Matrix::new(v[0], v[1], v[2], v[3], v[4], v[5])
                                        }
                                        _ => Matrix::default(),
                                    };
                                    let mut fst = State { font: st.font.clone(), size: st.size, char_space: st.char_space, word_space: st.word_space, horz: st.horz, leading: st.leading, rise: st.rise, render: st.render, line_width: st.line_width, ..State::default() };
                                    fst.ctm = fm;
                                    fst.text_m = st.text_m;
                                    let mut inner = Vec::new();
                                    // Every form run counts against the page's object cap (forms that draw
                                    // each other several times fan out exponentially).
                                    self.text_objects += 1;
                                    self.form_stack.push(key);
                                    self.run(&data, form_res, page_res.clone(), fst, &mut inner, level + 1);
                                    self.form_stack.pop();
                                    out.push(PageObj::Form(st.ctm, inner));
                                } else {
                                    out.push(PageObj::Form(st.ctm, Vec::new()));
                                }
                                if top {
                                    self.top_level_count += 1;
                                }
                            }
                            Some(b"Image") => {
                                out.push(PageObj::Other);
                                if top {
                                    self.top_level_count += 1;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                b"BI" => {
                    skip_inline_image(&mut lx, self.doc, &res, &page_res);
                    out.push(PageObj::Other);
                    if top {
                        self.top_level_count += 1;
                    }
                }
                b"BMC" | b"BDC" if marks.len() >= MAX_MARKS => marks_over += 1,
                b"BMC" => {
                    marks.push(Mark { param: None, actual_text: None });
                    marks_rc = Rc::new(marks.clone());
                }
                b"BDC" => {
                    let p = ops.last().cloned().unwrap_or(Obj::Null);
                    let (param, at) = match &p {
                        Obj::Dict(d) => {
                            let id = self.next_mark;
                            self.next_mark += 1;
                            (Some(id << 1), actual_text(self.doc, d))
                        }
                        Obj::Name(n) => match find_resource(self.doc, &res, &page_res, b"Properties", n) {
                            Some((key, Obj::Dict(d))) => (Some((key << 1) | 1), actual_text(self.doc, &d)),
                            _ => (None, None),
                        },
                        _ => (None, None),
                    };
                    marks.push(Mark { param, actual_text: at });
                    marks_rc = Rc::new(marks.clone());
                }
                b"EMC" if marks_over > 0 => marks_over -= 1,
                b"EMC" => {
                    if marks.pop().is_some() {
                        marks_rc = Rc::new(marks.clone());
                    }
                }
                b"m" | b"l" | b"c" | b"v" | b"y" | b"re" | b"h" => path_pending = true,
                b"S" | b"s" | b"f" | b"F" | b"f*" | b"B" | b"B*" | b"b" | b"b*" => {
                    if path_pending {
                        out.push(PageObj::Other);
                        if top {
                            self.top_level_count += 1;
                        }
                    }
                    path_pending = false;
                }
                b"n" => path_pending = false,
                b"sh" => {
                    out.push(PageObj::Other);
                    if top {
                        self.top_level_count += 1;
                    }
                }
                _ => {}
            }
            ops.clear();
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn add_text(&mut self, st: &mut State, strs: &[Vec<u8>], kerns: &[f32], init: f32, marks: &Rc<Vec<Mark>>, out: &mut Vec<PageObj>, top: bool) {
        let Some(font) = st.font.clone() else { return };
        if init != 0.0 {
            if font.is_vertical() {
                st.text_pos.1 -= init * st.size / 1000.0;
            } else {
                st.text_pos.0 -= init * st.size / 1000.0 * st.horz;
            }
        }
        if strs.is_empty() {
            return;
        }
        let mut codes = Vec::new();
        let mut kernings = Vec::new();
        for (i, s) in strs.iter().enumerate() {
            let c = font.codes(s);
            let n = c.len();
            codes.extend(c);
            kernings.extend(std::iter::repeat_n(0.0, n));
            if i + 1 != strs.len() && n > 0 {
                let last = kernings.len() - 1;
                kernings[last] = kerns[i];
            }
        }
        if codes.is_empty() {
            return;
        }
        let pos = st.text_position();
        let (positions, rect, advance) = calc_positions(&font, st.size, st.char_space, st.word_space, &codes, &kernings, &st.text_m, pos, st.render, st.line_width);
        if font.is_vertical() {
            st.text_pos.1 += advance;
        } else {
            st.text_pos.0 += advance * st.horz;
        }
        self.text_objects += 1;
        let t = TextObj {
            font: font.clone(),
            size: st.size,
            m: st.text_m,
            pos,
            codes,
            kernings,
            positions,
            rect,
            char_space: st.char_space,
            marks: marks.clone(),
        };
        out.push(PageObj::Text(Rc::new(t)));
        if top {
            self.top_level_count += 1;
        }
        let last = *kerns.last().unwrap_or(&0.0);
        if last != 0.0 {
            if font.is_vertical() {
                st.text_pos.1 -= last * st.size / 1000.0;
            } else {
                st.text_pos.0 -= last * st.size / 1000.0 * st.horz;
            }
        }
    }
}

fn push_op(ops: &mut Vec<Obj>, o: Obj) {
    if ops.len() >= PARAM_BUF {
        ops.remove(0);
    }
    ops.push(o);
}

/// PDFium's `CalcPositionDataInternal`: the char positions, the object rectangle, and the advance.
#[allow(clippy::too_many_arguments)]
fn calc_positions(font: &Font, size: f32, char_space: f32, word_space: f32, codes: &[u32], kernings: &[f32], m: &Matrix, pos: (f32, f32), render: i64, line_width: f32) -> (Vec<f32>, Rect, f32) {
    let mut cur = 0.0;
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (10000.0f32, -10000.0f32, 10000.0f32, -10000.0f32);
    let vertical = font.is_vertical();
    let mut positions = Vec::with_capacity(codes.len());
    let word_space_applies = |code: u32| code == 32 && (!font.is_cid() || font.codes(b" ") == [32]);
    for (i, &code) in codes.iter().enumerate() {
        positions.push(cur);
        let r = font.char_bbox(code);
        let w;
        if vertical {
            let (w1, vx, vy) = font.vert(code);
            let (l, rr) = ((r[0] - vx) as f32, (r[2] - vx) as f32);
            let (b, t) = ((r[1] - vy) as f32, (r[3] - vy) as f32);
            min_x = min_x.min(l).min(rr);
            max_x = max_x.max(l).max(rr);
            let ct = cur + t * size / 1000.0;
            let cb = cur + b * size / 1000.0;
            min_y = min_y.min(ct).min(cb);
            max_y = max_y.max(ct).max(cb);
            w = w1 as f32 * size / 1000.0;
        } else {
            min_y = min_y.min(r[3] as f32).min(r[1] as f32);
            max_y = max_y.max(r[3] as f32).max(r[1] as f32);
            let cl = cur + r[0] as f32 * size / 1000.0;
            let cr = cur + r[2] as f32 * size / 1000.0;
            min_x = min_x.min(cl).min(cr);
            max_x = max_x.max(cl).max(cr);
            w = font.width(code) as f32 * size / 1000.0;
        }
        cur += w;
        if word_space_applies(code) {
            cur += word_space;
        }
        cur += char_space;
        cur -= kernings[i] * size / 1000.0;
    }
    if vertical {
        min_x = min_x * size / 1000.0;
        max_x = max_x * size / 1000.0;
    } else {
        min_y = min_y * size / 1000.0;
        max_y = max_y * size / 1000.0;
    }
    let orig = Rect { left: min_x, bottom: min_y, right: max_x, top: max_y };
    let tm = Matrix { e: pos.0, f: pos.1, ..*m };
    let mut rect = tm.rect(&orig);
    if matches!(render, 1 | 2 | 5 | 6) {
        let h = line_width / 2.0;
        rect.left -= h;
        rect.right += h;
        rect.bottom -= h;
        rect.top += h;
    }
    (positions, rect, cur)
}

fn actual_text(doc: &Doc, d: &Dict) -> Option<Vec<u16>> {
    match d.get(b"ActualText").map(|o| doc.resolve(o)) {
        Some(Obj::Str(s)) => Some(crate::text::pdf_text_string(&s)),
        _ => None,
    }
}

/// A resource by category and name: the current resources, then the page's. The key identifies the object
/// (its number for an indirect object, the address of the dictionary otherwise).
fn find_resource(doc: &Doc, res: &Option<Rc<Dict>>, page_res: &Option<Rc<Dict>>, cat: &[u8], name: &[u8]) -> Option<(usize, Obj)> {
    for r in [res, page_res].into_iter().flatten() {
        if let Some(Obj::Dict(c)) = r.get(cat).map(|o| doc.resolve(o)) {
            if let Some(raw) = c.get(name) {
                let key = match raw {
                    Obj::Ref(n, _) => *n as usize,
                    _ => 0x4000_0000 + Rc::as_ptr(&c) as usize + c.position(name).unwrap_or(0),
                };
                return Some((key, doc.resolve(raw)));
            }
        }
    }
    None
}

/// Skips an inline image (`BI ... ID <data> EI`) the way PDFium measures it: an unfiltered image is its pitch
/// times its height in bytes, a filtered one is decoded to find its end; then everything up to the `EI` keyword.
fn skip_inline_image(lx: &mut Lexer, doc: &Doc, res: &Option<Rc<Dict>>, page_res: &Option<Rc<Dict>>) {
    let mut d = Dict::default();
    let save = lx.pos;
    loop {
        let before = lx.pos;
        match lx.next() {
            Tok::Word(b"ID", _) => break,
            Tok::Name(n) => {
                let key = expand_abbr(&obj::decode_name(n));
                if let Some(Parsed::Obj(v)) = obj::parse_obj(lx, 0, false) {
                    d.insert(&key, v);
                }
            }
            Tok::Eof => return,
            Tok::Word(_, _) => {
                lx.pos = save;
                return;
            }
            _ => {
                lx.pos = before + 1;
                break;
            }
        }
    }
    let b = lx.b;
    let mut pos = lx.pos;
    if pos < b.len() && obj::is_ws(b[pos]) {
        pos += 1;
    }
    let filter = d.get(b"Filter").cloned();
    let width = d.get(b"Width").and_then(|o| o.int()).unwrap_or(0).max(0) as u64;
    let height = d.get(b"Height").and_then(|o| o.int()).unwrap_or(0).max(0) as u64;
    let has_cs = d.has(b"ColorSpace");
    let comps: u64 = match d.get(b"ColorSpace") {
        Some(Obj::Name(n)) => match n.as_slice() {
            b"G" | b"DeviceGray" | b"CalGray" => 1,
            b"RGB" | b"DeviceRGB" | b"CalRGB" => 3,
            b"CMYK" | b"DeviceCMYK" => 4,
            b"I" | b"Indexed" => 1,
            other => match find_resource(doc, res, page_res, b"ColorSpace", other) {
                Some((_, Obj::Array(a))) => match a.first().and_then(|o| o.as_name()) {
                    Some(b"Indexed") | Some(b"I") | Some(b"Separation") => 1,
                    Some(b"DeviceGray") | Some(b"CalGray") => 1,
                    Some(b"DeviceCMYK") => 4,
                    Some(b"ICCBased") => 3,
                    _ => 3,
                },
                Some((_, Obj::Name(nn))) if nn.as_slice() == b"DeviceGray" => 1,
                Some((_, Obj::Name(nn))) if nn.as_slice() == b"DeviceCMYK" => 4,
                _ => 3,
            },
        },
        Some(Obj::Array(a)) => match a.first().and_then(|o| o.as_name()) {
            Some(b"Indexed") | Some(b"I") => 1,
            _ => 3,
        },
        _ => 1,
    };
    let bpc = if has_cs { d.get(b"BitsPerComponent").and_then(|o| o.int()).unwrap_or(0).max(0) as u64 } else { 1 };
    let pitch = (bpc * comps * width).div_ceil(8);
    let size = pitch.saturating_mul(height).min(b.len() as u64) as usize;
    match filter {
        None | Some(Obj::Null) => {
            pos = pos.saturating_add(size).min(b.len());
        }
        Some(_) => {
            // Find the EI keyword: whitespace, EI, then whitespace or the end.
            let mut p = pos;
            loop {
                match crate::doc::find(b, b"EI", p) {
                    None => {
                        pos = b.len();
                        break;
                    }
                    Some(q) => {
                        let ok_before = q == 0 || obj::is_ws(b[q - 1]);
                        let ok_after = q + 2 >= b.len() || obj::is_ws(b[q + 2]) || obj::is_delim(b[q + 2]);
                        if ok_before && ok_after {
                            pos = q;
                            break;
                        }
                        p = q + 1;
                    }
                }
            }
        }
    }
    lx.pos = pos;
    // Everything up to the EI keyword is image data.
    loop {
        match lx.next() {
            Tok::Eof => return,
            Tok::Word(b"EI", _) => return,
            Tok::StrStart => {
                lx.read_string();
            }
            Tok::HexStart => {
                lx.read_hex();
            }
            _ => {}
        }
    }
}

fn expand_abbr(k: &[u8]) -> Vec<u8> {
    match k {
        b"BPC" => b"BitsPerComponent".to_vec(),
        b"CS" => b"ColorSpace".to_vec(),
        b"D" => b"Decode".to_vec(),
        b"DP" => b"DecodeParms".to_vec(),
        b"F" => b"Filter".to_vec(),
        b"H" => b"Height".to_vec(),
        b"IM" => b"ImageMask".to_vec(),
        b"I" => b"Interpolate".to_vec(),
        b"W" => b"Width".to_vec(),
        _ => k.to_vec(),
    }
}

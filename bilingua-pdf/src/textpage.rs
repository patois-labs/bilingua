//! The page text, as PDFium's `CPDF_TextPage` builds it (Copyright 2014 The PDFium Authors, BSD-3-Clause; the
//! algorithm is re-written here in Rust): text objects grouped into lines, sorted left to right inside a line,
//! spaces generated from gaps and kerning, `\r\n` between lines, a hyphen at a line end turned into U+FFFE, bidi
//! runs reordered, duplicate (fake bold) glyphs dropped, `/ActualText` honoured.
//!
//! The result is what `FPDFText_GetText` returns for the whole page, which is the input Localization OS splits
//! into units.

use crate::content::{Matrix, PageObj, Rect, TextObj};
use crate::font::INVALID_CODE;
use crate::text::{self, Dir};
use std::collections::BTreeMap;
use std::rc::Rc;

const SIZE_EPS: f32 = 0.01;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CharType {
    Normal,
    Generated,
    NotUnicode,
    Hyphen,
    Piece,
    ActualText,
}

#[derive(Clone)]
struct CharInfo {
    ty: CharType,
    code: u32,
    unicode: u16,
    origin: (f32, f32),
    bbox: Rect,
    obj: Option<Rc<TextObj>>,
}

#[derive(Clone)]
struct TObj {
    t: Rc<TextObj>,
    form: Matrix,
}

const NIL: usize = usize::MAX;

/// The objects of the current line in PDFium's order (`m_LineObj`): a new object goes right after the last one,
/// scanning from the end, whose x is not greater than its own, or first when there is none. PDFium inserts into a
/// vector, which costs time in proportion to the line; a hostile page with hundreds of thousands of objects on one
/// line made that quadratic. Here the objects are a linked list and the numeric x positions an ordered map: the
/// numeric objects of the list are always in ascending x (equal x in arrival order), so "the last one not greater"
/// is the map's greatest key not above (x, any). An object with a NaN x compares false with everything, as in the
/// scan: it goes first and is never a place to insert after.
struct Line<T> {
    nodes: Vec<(T, usize)>,
    head: usize,
    tail: usize,
    by_x: BTreeMap<(u32, usize), usize>,
}

/// An order-preserving key of a non-NaN float (`-0.0` is `0.0`, as `>=` sees it).
fn x_key(x: f32) -> u32 {
    let b = (x + 0.0).to_bits();
    if b & 0x8000_0000 != 0 {
        !b
    } else {
        b | 0x8000_0000
    }
}

impl<T> Default for Line<T> {
    fn default() -> Self {
        Line { nodes: Vec::new(), head: NIL, tail: NIL, by_x: BTreeMap::new() }
    }
}

impl<T> Line<T> {
    fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
    fn clear(&mut self) {
        self.nodes.clear();
        self.by_x.clear();
    }
    fn last(&self) -> Option<&T> {
        if self.nodes.is_empty() {
            None
        } else {
            Some(&self.nodes[self.tail].0)
        }
    }
    fn insert(&mut self, o: T, x: f32) {
        let id = self.nodes.len();
        if id == 0 {
            self.nodes.push((o, NIL));
            self.head = 0;
            self.tail = 0;
        } else {
            let after = if x.is_nan() { None } else { self.by_x.range(..=(x_key(x), usize::MAX)).next_back().map(|(_, &n)| n) };
            match after {
                Some(p) => {
                    let next = self.nodes[p].1;
                    self.nodes.push((o, next));
                    self.nodes[p].1 = id;
                    if next == NIL {
                        self.tail = id;
                    }
                }
                None => {
                    self.nodes.push((o, self.head));
                    self.head = id;
                }
            }
        }
        if !x.is_nan() {
            self.by_x.insert((x_key(x), id), id);
        }
    }
    fn take(&mut self) -> Vec<T> {
        let links: Vec<usize> = self.nodes.iter().map(|n| n.1).collect();
        let mut slots: Vec<Option<T>> = std::mem::take(&mut self.nodes).into_iter().map(|(o, _)| Some(o)).collect();
        let mut out = Vec::with_capacity(slots.len());
        let mut i = if slots.is_empty() { NIL } else { self.head };
        while i != NIL {
            if let Some(o) = slots[i].take() {
                out.push(o);
            }
            i = links[i];
        }
        self.by_x.clear();
        out
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Orient {
    Unknown,
    Horizontal,
    Vertical,
}

#[derive(PartialEq, Eq)]
enum Gen {
    None,
    Space,
    LineBreak,
    Hyphen,
}

#[derive(PartialEq, Eq)]
enum Mkc {
    Pass,
    Done,
    Delay,
}

pub struct TextPage {
    display: Matrix,
    rtl: bool,
    chars: Vec<CharInfo>,
    buf: Vec<u16>,
    tchars: Vec<CharInfo>,
    tbuf: Vec<u16>,
    objs: Line<TObj>,
    prev: Option<Rc<TextObj>>,
    prev_matrix: Matrix,
    curline: Rect,
    line_dir: Orient,
}

/// The page display matrix of `CPDF_Page::GetDisplayMatrix` for a page box and rotation.
pub fn display_matrix(bbox: [f32; 4], rotation: i64) -> (Matrix, f32, f32) {
    let (l, b, r, t) = (bbox[0], bbox[1], bbox[2], bbox[3]);
    let (mut w, mut h) = (r - l, t - b);
    let page = match rotation {
        1 => {
            std::mem::swap(&mut w, &mut h);
            Matrix::new(0.0, -1.0, 1.0, 0.0, -b, r)
        }
        2 => Matrix::new(-1.0, 0.0, 0.0, -1.0, r, t),
        3 => {
            std::mem::swap(&mut w, &mut h);
            Matrix::new(0.0, 1.0, -1.0, 0.0, t, -l)
        }
        _ => Matrix::new(1.0, 0.0, 0.0, 1.0, -l, -b),
    };
    if w == 0.0 || h == 0.0 {
        return (Matrix::default(), w, h);
    }
    // rotation 0 for the display rect (0, 0, w, h): x0 = left, y0 = top, x1 = left, y1 = bottom, x2 = right, y2 = top
    let (x0, y0, x1, y1, x2, y2) = (0.0, h, 0.0, 0.0, w, h);
    let m = Matrix::new((x2 - x0) / w, (y2 - y0) / w, (x1 - x0) / h, (y1 - y0) / h, x0, y0);
    (page.then(&m), w, h)
}

fn is_hyphen_code(c: u16) -> bool {
    c == 0x2D || c == 0xAD
}

fn is_control(c: &CharInfo) -> bool {
    matches!(c.unicode, 0x2 | 0x3 | 0x93 | 0x94 | 0x96 | 0x97 | 0x98 | 0xfffe) && c.ty != CharType::Hyphen
}

fn is_normal(c: &CharInfo) -> bool {
    if c.unicode != 0 {
        !is_control(c)
    } else {
        c.code != 0
    }
}

fn normalize_threshold(t: f32, t1: f32, t2: f32, t3: f32) -> f32 {
    if t < t1 {
        t / 2.0
    } else if t < t2 {
        t / 4.0
    } else if t < t3 {
        t / 5.0
    } else {
        t / 6.0
    }
}

fn char_width(code: u32, t: &TextObj) -> f32 {
    t.font.char_width_or_box(code) as f32
}

fn first_unicode(t: &TextObj, code: u32) -> u16 {
    let u = t.font.unicode(code);
    match u.first() {
        Some(&c) => c,
        None => code as u16,
    }
}

fn is_rtl_obj(t: &TextObj) -> bool {
    let mut s = Vec::with_capacity(t.codes.len());
    for &c in &t.codes {
        let u = t.font.unicode(c);
        let mut wc = u.first().copied().unwrap_or(0);
        if wc == 0 {
            wc = c as u16;
        }
        if wc != 0 {
            s.push(wc);
        }
    }
    text::bidi(&s, false).1 == Dir::Right
}

impl TextPage {
    pub fn new(objs: &[PageObj], bbox: [f32; 4], rotation: i64, rtl: bool) -> TextPage {
        let (display, w, h) = display_matrix(bbox, rotation);
        let mut p = TextPage {
            display,
            rtl,
            chars: Vec::new(),
            buf: Vec::new(),
            tchars: Vec::new(),
            tbuf: Vec::new(),
            objs: Line::default(),
            prev: None,
            prev_matrix: Matrix::default(),
            curline: Rect::default(),
            line_dir: Orient::Unknown,
        };
        if objs.is_empty() {
            return p;
        }
        p.line_dir = flow_orientation(objs, w, h);
        let ident = Matrix::default();
        for (i, o) in objs.iter().enumerate() {
            match o {
                PageObj::Text(t) => p.process_text_object(t, ident, objs, i),
                PageObj::Form(m, inner) => p.process_form(inner, m.then(&ident)),
                PageObj::Other => {}
            }
        }
        p.process_transformed();
        p.objs.clear();
        p.close_temp_line();
        p
    }

    /// The page text, decoded as pypdfium2 decodes it (lone surrogates dropped).
    pub fn text(&self) -> String {
        char::decode_utf16(self.buf.iter().copied()).filter_map(|r| r.ok()).collect()
    }

    fn process_form(&mut self, objs: &[PageObj], m: Matrix) {
        for (i, o) in objs.iter().enumerate() {
            match o {
                PageObj::Text(t) => self.process_text_object(t, m, objs, i),
                PageObj::Form(fm, inner) => self.process_form(inner, fm.then(&m)),
                PageObj::Other => {}
            }
        }
    }

    fn process_text_object(&mut self, t: &Rc<TextObj>, form: Matrix, list: &[PageObj], idx: usize) {
        if t.rect.width().abs() < SIZE_EPS {
            return;
        }
        let new = TObj { t: t.clone(), form };
        let tp = form.apply(t.pos.0, t.pos.1);
        let this_pos = self.display.apply(tp.0, tp.1);
        if self.objs.is_empty() {
            self.objs.insert(new, this_pos.0);
            return;
        }
        if self.same_as_previous(t, list, idx) {
            return;
        }
        let prev = self.objs.last().unwrap().clone();
        let n = prev.t.codes.len();
        if n == 0 {
            return;
        }
        let pt = &prev.t;
        let mut prev_width = char_width(pt.codes[n - 1], pt) * pt.size / 1000.0;
        let prev_m = pt.text_matrix().then(&prev.form);
        prev_width = prev_m.distance(prev_width.abs());
        let mut this_width = (char_width(t.codes[0], t) * t.size / 1000.0).abs();
        let this_m = t.text_matrix().then(&form);
        this_width = this_m.distance(this_width.abs());
        let threshold = prev_width.max(this_width) / 4.0;
        let pp = prev.form.apply(pt.pos.0, pt.pos.1);
        let prev_pos = self.display.apply(pp.0, pp.1);
        if (this_pos.1 - prev_pos.1).abs() > threshold * 2.0 {
            self.process_transformed();
            self.objs.clear();
            self.objs.insert(new, this_pos.0);
            return;
        }
        self.objs.insert(new, this_pos.0);
    }

    fn same_as_previous(&self, t: &Rc<TextObj>, list: &[PageObj], idx: usize) -> bool {
        let mut i = 0;
        let mut k = idx;
        while i < 5 && k > 0 {
            k -= 1;
            let PageObj::Text(other) = &list[k] else { continue };
            if Rc::ptr_eq(other, t) {
                continue;
            }
            if self.same_text_object(other, t) {
                return true;
            }
            i += 1;
        }
        false
    }

    /// `IsSameTextObject(obj1 = other, obj2 = t)`.
    fn same_text_object(&self, o1: &TextObj, o2: &TextObj) -> bool {
        let mut prev_rect = o2.rect;
        let cur_rect = o1.rect;
        if prev_rect.is_empty() && cur_rect.is_empty() {
            let xdiff = (prev_rect.left - cur_rect.left).abs();
            let n = self.chars.len();
            if n >= 2 {
                let db = self.chars[n - 2].bbox.width();
                if xdiff > db {
                    return false;
                }
            }
        }
        if !prev_rect.is_empty() || !cur_rect.is_empty() {
            prev_rect = prev_rect.intersect(&cur_rect);
            if prev_rect.is_empty() {
                return false;
            }
            if (prev_rect.width() - cur_rect.width()).abs() > cur_rect.width() / 2.0 {
                return false;
            }
            if o2.size != o1.size {
                return false;
            }
        }
        let n = o2.codes.len();
        if n != o1.codes.len() {
            return false;
        }
        if n == 0 {
            return true;
        }
        if o1.codes != o2.codes {
            return false;
        }
        let dx = o1.pos.0 - o2.pos.0;
        let dy = o1.pos.1 - o2.pos.1;
        let fs = o2.size;
        let cs = char_width(o2.codes[n - 1], o2);
        let max_pre = prev_rect.height().max(prev_rect.width()).max(fs);
        dx.abs() <= 0.9 * cs * fs / 1000.0 && dy.abs() <= max_pre / 8.0
    }

    fn process_transformed(&mut self) {
        let objs = self.objs.take();
        for o in &objs {
            let t = &o.t;
            if t.rect.width().abs() < SIZE_EPS {
                continue;
            }
            let form = o.form;
            let pre = self.pre_marked_content(t);
            if pre == Mkc::Done {
                self.prev = Some(t.clone());
                self.prev_matrix = form;
                continue;
            }
            if self.prev.is_some() {
                let g = self.insert_object(t, form);
                if g == Gen::LineBreak {
                    self.curline = t.rect;
                } else {
                    self.curline = self.curline.union(&t.rect);
                }
                if !self.generate_char(g, t, form) {
                    continue;
                }
            } else {
                self.curline = t.rect;
            }
            let ci = self.tchars.len();
            let bi = self.tbuf.len();
            if pre == Mkc::Delay {
                if self.marked_content(t, form) {
                    self.reverse_temp(ci, bi);
                }
                self.prev = Some(t.clone());
                self.prev_matrix = form;
                continue;
            }
            self.prev = Some(t.clone());
            self.prev_matrix = form;
            let m = t.text_matrix().then(&form);
            if self.object_items(t, form, m) {
                self.reverse_temp(ci, bi);
            }
        }
    }

    fn reverse_temp(&mut self, ci: usize, bi: usize) {
        if ci < self.tchars.len() {
            self.tchars[ci..].reverse();
        }
        if bi < self.tbuf.len() {
            self.tbuf[bi..].reverse();
        }
    }

    fn pre_marked_content(&self, t: &TextObj) -> Mkc {
        let marks = &t.marks;
        if marks.is_empty() {
            return Mkc::Pass;
        }
        let mut actual: Vec<u16> = Vec::new();
        let mut exist = false;
        for m in marks.iter() {
            if m.param.is_none() {
                continue;
            }
            if let Some(a) = &m.actual_text {
                exist = true;
                actual = a.clone();
            }
        }
        if !exist {
            return Mkc::Pass;
        }
        let last = marks.last().unwrap().param;
        if let Some(p) = &self.prev {
            if p.marks.len() == marks.len() && p.marks.last().unwrap().param == last {
                return Mkc::Done;
            }
        }
        if actual.is_empty() {
            return Mkc::Pass;
        }
        let printable = actual.iter().any(|&wc| (wc > 0x80 && wc < 0xFFFD) || (wc <= 0x80 && (0x20..0x7f).contains(&wc)));
        if !printable {
            return Mkc::Done;
        }
        Mkc::Delay
    }

    fn marked_content(&mut self, t: &Rc<TextObj>, _form: Matrix) -> bool {
        let mut actual: Vec<u16> = Vec::new();
        for m in t.marks.iter() {
            if m.param.is_some() {
                actual = m.actual_text.clone().unwrap_or_default();
            }
        }
        if actual.is_empty() {
            return false;
        }
        let rtl = is_rtl_obj(t) || (self.prev.as_ref().is_some_and(|p| is_rtl_obj(p)) && text::bidi(&actual, false).1 == Dir::Right);
        let mut rect = t.rect;
        let n = actual.len() as f32;
        let step;
        if rtl {
            rect.left = rect.right - rect.width() / n;
            step = -rect.width();
        } else {
            rect.right = rect.left + rect.width() / n;
            step = rect.width();
        }
        for (k, &wc0) in actual.iter().enumerate() {
            let mut wc = wc0;
            if wc <= 0x80 && !(0x20..0x7f).contains(&wc) {
                wc = 0x20;
            }
            if wc >= 0xFFFD {
                continue;
            }
            let mut b = rect;
            b.left += k as f32 * step;
            b.right += k as f32 * step;
            self.tbuf.push(wc);
            self.tchars.push(CharInfo { ty: CharType::ActualText, code: INVALID_CODE, unicode: wc, origin: t.pos, bbox: b, obj: Some(t.clone()) });
        }
        rtl
    }

    fn prev_char(&self) -> Option<&CharInfo> {
        self.tchars.last().or(self.chars.last())
    }

    fn writing_mode(&self, t: &TextObj) -> Orient {
        let n = t.codes.len();
        if n <= 1 {
            return self.line_dir;
        }
        let tm = t.text_matrix();
        let f = t.item_origin(0);
        let l = t.item_origin(n - 1);
        let f = tm.apply(f.0, f.1);
        let l = tm.apply(l.0, l.1);
        let dx = (l.0 - f.0).abs();
        let dy = (l.1 - f.1).abs();
        if dx <= 0.0001 && dy <= 0.0001 {
            return Orient::Unknown;
        }
        let len = (dx * dx + dy * dy).sqrt();
        let (vx, vy) = (dx / len, dy / len);
        let under = vx <= 0.0872;
        if vy <= 0.0872 {
            return if under { self.line_dir } else { Orient::Horizontal };
        }
        if under {
            Orient::Vertical
        } else {
            self.line_dir
        }
    }

    fn is_hyphen(&self, cur: u16) -> bool {
        let s: &[u16] = if self.tbuf.is_empty() { &self.buf } else { &self.tbuf };
        if s.is_empty() {
            return false;
        }
        let mut i = s.len() - 1;
        while i > 0 && s[i] == 0x20 {
            i -= 1;
        }
        if !is_hyphen_code(s[i]) {
            return false;
        }
        if i > 0 {
            let p = s[i - 1];
            if text::is_alpha(p) && text::is_alnum(cur) {
                return true;
            }
        }
        self.prev_char().is_some_and(|c| matches!(c.ty, CharType::Piece | CharType::ActualText) && is_hyphen_code(c.unicode))
    }

    fn insert_object(&mut self, t: &Rc<TextObj>, form: Matrix) -> Gen {
        if let Some(c) = self.prev_char() {
            if let Some(o) = &c.obj {
                self.prev = Some(o.clone());
            }
        }
        let Some(prev) = self.prev.clone() else { return Gen::None };
        let mut mode = self.writing_mode(t);
        if mode == Orient::Unknown {
            mode = self.writing_mode(&prev);
        }
        let n = prev.codes.len();
        if n == 0 {
            return Gen::None;
        }
        let prev_code = prev.codes[n - 1];
        let code = t.codes[0];
        let this_rect = t.rect;
        let prev_rect = prev.rect;
        let current = first_unicode(t, code);
        if mode == Orient::Horizontal {
            if this_rect.height() > 4.5 && prev_rect.height() > 4.5 {
                let top = this_rect.top.min(prev_rect.top);
                let bottom = this_rect.bottom.max(prev_rect.bottom);
                if bottom >= top {
                    return if self.is_hyphen(current) { Gen::Hyphen } else { Gen::LineBreak };
                }
            }
        } else if mode == Orient::Vertical
            && this_rect.width() > t.size * 0.1
            && prev_rect.width() > prev.size * 0.1
            && this_rect.right.min(self.curline.right) <= this_rect.left.max(self.curline.left)
        {
            return if self.is_hyphen(current) { Gen::Hyphen } else { Gen::LineBreak };
        }
        let last_pos = prev.item_origin(n - 1).0;
        let n_last = char_width(prev_code, &prev);
        let last_width = (n_last * prev.size / 1000.0).abs();
        let n_this = char_width(code, t);
        let this_width = (n_this * t.size / 1000.0).abs();
        let mut threshold = last_width.max(this_width) / 4.0;
        let prev_m = prev.text_matrix().then(&self.prev_matrix);
        let inv = prev_m.inverse();
        let fp = form.apply(t.pos.0, t.pos.1);
        let pos = inv.apply(fp.0, fp.1);
        if last_width < this_width {
            threshold = inv.distance(threshold);
        }
        let mut newline = false;
        if mode == Orient::Horizontal {
            let rh = prev.rect.height();
            let r = prev.rect.normalized();
            if (r.is_empty() && rh > 5.0) || ((pos.1 > threshold * 2.0 || pos.1 < threshold * -3.0) && (pos.1.abs() >= 1.0 || pos.1.abs() > pos.0.abs())) {
                newline = true;
                if n > 1 {
                    let first = prev.item_origin(0);
                    let pm = prev.m;
                    let d = &self.display;
                    if prev.item_origin(n - 1).0 > first.0 && d.a > 0.9 && d.b < 0.1 && d.c < 0.1 && d.d < -0.9 && pm.b < 0.1 && pm.c < 0.1 {
                        let re = Rect { left: 0.0, bottom: prev.rect.bottom, right: 1000.0, top: prev.rect.top };
                        if re.contains(t.pos.0, t.pos.1) {
                            newline = false;
                        } else {
                            let re2 = Rect { left: 0.0, bottom: t.rect.bottom, right: 1000.0, top: t.rect.top };
                            if re2.contains(prev.pos.0, prev.pos.1) {
                                newline = false;
                            }
                        }
                    }
                }
            }
        }
        if newline {
            return if self.is_hyphen(current) { Gen::Hyphen } else { Gen::LineBreak };
        }
        if t.codes.len() == 1 && is_hyphen_code(current) && self.is_hyphen(current) {
            return Gen::Hyphen;
        }
        if current == 0x20 {
            return Gen::None;
        }
        let prev_str = prev.font.unicode(prev_code);
        if prev_str.last() == Some(&0x20) {
            return Gen::None;
        }
        let m = t.text_matrix().then(&form);
        let mut t2 = n_last.max(n_this);
        t2 = normalize_threshold(t2, 400.0, 700.0, 800.0);
        if n_last >= n_this {
            t2 *= prev.size.abs();
        } else {
            t2 *= t.size.abs();
            t2 = m.distance(t2);
            t2 = inv.distance(t2);
        }
        t2 /= 1000.0;
        let t2f = t2;
        if (t2f < 1.4881 && t2f > 1.4879) || (t2f < 1.39001 && t2f > 1.38999) {
            t2 *= 1.5;
        }
        if generate_space(pos, last_pos, this_width, last_width, t2) {
            Gen::Space
        } else {
            Gen::None
        }
    }

    fn generated_char(&self, u: u16) -> Option<CharInfo> {
        let p = self.prev_char()?;
        let mut pre_width = 0.0;
        let font_size;
        match &p.obj {
            Some(o) => {
                if p.code != INVALID_CODE {
                    pre_width = char_width(p.code, o);
                }
                font_size = o.size;
            }
            None => font_size = p.bbox.height(),
        }
        let fs = if font_size == 0.0 { 1.0 } else { font_size };
        let origin = (p.origin.0 + pre_width * fs / 1000.0, p.origin.1);
        Some(CharInfo { ty: CharType::Generated, code: INVALID_CODE, unicode: u, origin, bbox: Rect { left: origin.0, right: origin.0, bottom: origin.1, top: origin.1 }, obj: None })
    }

    fn generate_char(&mut self, g: Gen, t: &TextObj, _form: Matrix) -> bool {
        match g {
            Gen::None => true,
            Gen::Space => {
                if let Some(c) = self.generated_char(0x20) {
                    self.tbuf.push(0x20);
                    self.tchars.push(c);
                }
                true
            }
            Gen::LineBreak => {
                self.close_temp_line();
                if !self.buf.is_empty() {
                    for u in [0x0D, 0x0A] {
                        if let Some(c) = self.generated_char(u) {
                            self.buf.push(u);
                            self.chars.push(c);
                        }
                    }
                }
                true
            }
            Gen::Hyphen => {
                if t.codes.len() == 1 {
                    let cur = first_unicode(t, t.codes[0]);
                    if is_hyphen_code(cur) {
                        return false;
                    }
                }
                while self.tbuf.last() == Some(&0x20) {
                    self.tbuf.pop();
                    self.tchars.pop();
                }
                if let Some(ci) = self.tchars.last_mut() {
                    self.tbuf.pop();
                    ci.ty = CharType::Hyphen;
                    ci.unicode = 0x2;
                    self.tbuf.push(0xfffe);
                }
                true
            }
        }
    }

    fn object_items(&mut self, t: &Rc<TextObj>, _form: Matrix, m: Matrix) -> bool {
        let base_space = base_space(t, &m) + base_space_adjust(t, &m);
        let mut spacing = 0.0;
        let n = t.codes.len();
        for i in 0..n {
            let code = t.codes[i];
            if i > 0 && t.kernings[i - 1] != 0.0 {
                let s: &[u16] = if self.tbuf.is_empty() { &self.buf } else { &self.tbuf };
                if let Some(&last) = s.last() {
                    if last != 0x20 {
                        spacing = -t.font_size_h() * t.kernings[i - 1] / 1000.0;
                    }
                }
            }
            spacing -= base_space;
            if spacing != 0.0 && i > 0 {
                let th = space_threshold(t, code);
                if th != 0.0 && spacing >= th {
                    self.tbuf.push(0x20);
                    let io = t.item_origin(i);
                    let origin = m.apply(io.0, io.1);
                    self.tchars.push(CharInfo {
                        ty: CharType::Generated,
                        code: INVALID_CODE,
                        unicode: 0x20,
                        origin,
                        bbox: Rect { left: origin.0, right: origin.0, bottom: origin.1, top: origin.1 },
                        obj: Some(t.clone()),
                    });
                }
            }
            spacing = 0.0;
            let mut unicode = t.font.unicode(code);
            let mut ty = CharType::Normal;
            if unicode.is_empty() && code != 0 {
                unicode = vec![code as u16];
                ty = CharType::NotUnicode;
            }
            let r = t.font.char_bbox(code);
            let fs = t.size / 1000.0;
            let io = t.item_origin(i);
            let mut b = Rect { left: r[0] as f32 * fs + io.0, bottom: r[1] as f32 * fs + io.1, right: r[2] as f32 * fs + io.0, top: r[3] as f32 * fs + io.1 };
            if (b.top - b.bottom).abs() < SIZE_EPS {
                b.top = b.bottom + fs;
            }
            if (b.right - b.left).abs() < SIZE_EPS {
                b.right = b.left + t.char_width(code);
            }
            let bbox = m.rect(&b);
            let origin = m.apply(io.0, io.1);
            let mut ci = CharInfo { ty, code, unicode: 0, origin, bbox, obj: Some(t.clone()) };
            if unicode.is_empty() {
                self.tchars.push(ci);
                self.tbuf.push(0xfffe);
                continue;
            }
            let mut add = true;
            let count = self.tchars.len().min(7);
            let threshold = m.x_distance(0.07 * t.size);
            let len = self.tchars.len();
            for k in (len - count..len).rev() {
                let c = &self.tchars[k];
                if c.code != ci.code {
                    continue;
                }
                let Some(co) = &c.obj else { continue };
                if co.font.id != t.font.id {
                    continue;
                }
                let dx = c.origin.0 - ci.origin.0;
                let dy = c.origin.1 - ci.origin.1;
                if dx.abs() < threshold && dy.abs() < threshold {
                    add = false;
                    break;
                }
            }
            if add {
                for c in unicode {
                    ci.unicode = c;
                    self.tbuf.push(if c != 0 { c } else { 0xfffe });
                    self.tchars.push(ci.clone());
                }
            } else if i == 0 && self.tbuf.last() == Some(&0x20) {
                self.tbuf.pop();
                self.tchars.pop();
            }
        }
        is_rtl_obj(t) && (m.a * m.d - m.b * m.c) < 0.0
    }

    fn add_char(&mut self, wc0: u16, info: &CharInfo, rtl: bool) {
        if !is_normal(info) {
            self.chars.push(info.clone());
            return;
        }
        let mut wc = wc0;
        if rtl {
            wc = text::mirror(wc);
        }
        let normalized = if rtl || (0xFB00..=0xFB06).contains(&wc) { text::normalize(wc) } else { Vec::new() };
        let mut ci = info.clone();
        if normalized.is_empty() {
            self.buf.push(wc);
            if rtl {
                ci.unicode = wc;
            }
            self.chars.push(ci);
            return;
        }
        ci.ty = CharType::Piece;
        for c in normalized {
            ci.unicode = c;
            self.buf.push(c);
            self.chars.push(ci.clone());
        }
    }

    fn close_temp_line(&mut self) {
        if self.tchars.is_empty() {
            return;
        }
        // Collapse runs of spaces.
        let mut s: Vec<u16> = Vec::with_capacity(self.tbuf.len());
        let mut cs: Vec<CharInfo> = Vec::with_capacity(self.tchars.len());
        let mut prev_space = false;
        let tbuf = std::mem::take(&mut self.tbuf);
        let tchars = std::mem::take(&mut self.tchars);
        for (i, &c) in tbuf.iter().enumerate() {
            if c != 0x20 {
                prev_space = false;
            } else if prev_space {
                continue;
            } else {
                prev_space = true;
            }
            s.push(c);
            if let Some(ci) = tchars.get(i) {
                cs.push(ci.clone());
            }
        }
        let (segs, overall) = text::bidi(&s, self.rtl);
        let mut cur = overall;
        for seg in segs {
            let end = (seg.start + seg.count).min(s.len()).min(cs.len());
            if seg.start >= end {
                if seg.dir == Dir::Right || (seg.dir == Dir::Neutral && cur == Dir::Right) {
                    cur = Dir::Right;
                } else if seg.dir != Dir::LeftWeak {
                    cur = Dir::Left;
                }
                continue;
            }
            if seg.dir == Dir::Right || (seg.dir == Dir::Neutral && cur == Dir::Right) {
                cur = Dir::Right;
                if cs[seg.start].ty == CharType::ActualText {
                    for k in seg.start..end {
                        let ci = cs[k].clone();
                        self.add_char(s[k], &ci, true);
                    }
                } else {
                    for k in (seg.start..end).rev() {
                        let ci = cs[k].clone();
                        self.add_char(s[k], &ci, true);
                    }
                }
            } else {
                if seg.dir != Dir::LeftWeak {
                    cur = Dir::Left;
                }
                for k in seg.start..end {
                    let ci = cs[k].clone();
                    self.add_char(s[k], &ci, false);
                }
            }
        }
    }
}

fn generate_space(pos: (f32, f32), last_pos: f32, this_width: f32, last_width: f32, threshold: f32) -> bool {
    if (last_pos + last_width - pos.0).abs() <= threshold {
        return false;
    }
    let tp = threshold + last_width;
    let diff = pos.0 - last_pos;
    if diff.abs() > tp {
        return true;
    }
    if pos.0 < 0.0 && -tp > diff {
        return true;
    }
    diff > this_width + last_width
}

fn base_space(t: &TextObj, m: &Matrix) -> f32 {
    let cs = t.char_space;
    if cs == 0.0 || t.codes.len() < 2 {
        return 0.0;
    }
    let mut has_kerning = false;
    let spacing = m.distance(cs);
    let fs = t.font_size_h();
    let mut base = spacing;
    for &k in &t.kernings {
        if k != 0.0 {
            let kern = -fs * k / 1000.0;
            base = base.min(kern + spacing);
            has_kerning = true;
        }
    }
    if base < 0.0 || (t.codes.len() == 2 && has_kerning) {
        return 0.0;
    }
    base
}

fn base_space_adjust(t: &TextObj, m: &Matrix) -> f32 {
    let cs = t.char_space;
    if cs > 0.001 {
        -m.distance(cs)
    } else if cs < -0.001 {
        m.distance(cs.abs())
    } else {
        0.0
    }
}

fn space_threshold(t: &TextObj, code: u32) -> f32 {
    let fs = t.font_size_h();
    let sc = t.font.code_from_unicode(0x20);
    let mut th = 0.0;
    if sc != INVALID_CODE {
        th = fs * t.font.width(sc) as f32 / 1000.0;
    }
    if th > fs / 3.0 {
        th = 0.0;
    } else {
        th /= 2.0;
    }
    if th == 0.0 {
        th = char_width(code, t);
        th = normalize_threshold(th, 300.0, 500.0, 700.0);
        th = fs * th / 1000.0;
    }
    th
}

/// PDFium's `FindTextlineFlowOrientation` over the page's top-level text objects.
fn flow_orientation(objs: &[PageObj], w: f32, h: f32) -> Orient {
    let (pw, ph) = (w as i64, h as i64);
    if pw <= 0 || ph <= 0 || pw > 100_000 || ph > 100_000 {
        return Orient::Unknown;
    }
    let mut hm = vec![false; pw as usize];
    let mut vm = vec![false; ph as usize];
    let mut line_h = 0.0f32;
    let (mut sh, mut eh, mut sv, mut ev) = (pw, 0i64, ph, 0i64);
    for o in objs {
        let PageObj::Text(t) = o else { continue };
        let r = t.rect;
        let min_h = r.left.clamp(0.0, pw as f32) as i64;
        let max_h = r.right.clamp(0.0, pw as f32) as i64;
        let min_v = r.bottom.clamp(0.0, ph as f32) as i64;
        let max_v = r.top.clamp(0.0, ph as f32) as i64;
        if min_h >= max_h || min_v >= max_v {
            continue;
        }
        for x in hm.iter_mut().take(max_h as usize).skip(min_h as usize) {
            *x = true;
        }
        for x in vm.iter_mut().take(max_v as usize).skip(min_v as usize) {
            *x = true;
        }
        sh = sh.min(min_h);
        eh = eh.max(max_h);
        sv = sv.min(min_v);
        ev = ev.max(max_v);
        if line_h <= 0.0 {
            line_h = r.height();
        }
    }
    let dbl = (2.0 * line_h) as i64;
    if ev - sv < dbl {
        return Orient::Horizontal;
    }
    if eh - sh < dbl {
        return Orient::Vertical;
    }
    let pct = |m: &[bool], s: i64, e: i64| -> f32 {
        if s >= e {
            return 0.0;
        }
        m[s as usize..e as usize].iter().filter(|x| **x).count() as f32 / (e - s) as f32
    };
    let sum_h = pct(&hm, sh, eh);
    if sum_h > 0.8 {
        return Orient::Horizontal;
    }
    let sum_v = pct(&vm, sv, ev);
    if sum_h > sum_v {
        Orient::Horizontal
    } else if sum_h < sum_v {
        Orient::Vertical
    } else {
        Orient::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PDFium's vector insertion, the reference for [`Line`].
    fn naive(xs: &[f32]) -> Vec<usize> {
        let mut v: Vec<(usize, f32)> = Vec::new();
        for (id, &x) in xs.iter().enumerate() {
            let mut at = 0;
            for i in (1..=v.len()).rev() {
                if x >= v[i - 1].1 {
                    at = i;
                    break;
                }
            }
            v.insert(at, (id, x));
        }
        v.into_iter().map(|(id, _)| id).collect()
    }

    #[test]
    fn line_order_is_pdfiums_vector_order() {
        let mut seed = 12345u64;
        let mut next = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as u32
        };
        let pool = [0.0f32, -0.0, 1.0, 2.5, -3.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1e-30, 7.0];
        for _ in 0..3000 {
            let n = (next() % 40) as usize;
            let xs: Vec<f32> = (0..n).map(|_| if next() % 3 == 0 { pool[(next() % 10) as usize] } else { (next() % 9) as f32 - 4.0 }).collect();
            let mut l = Line::default();
            for (id, &x) in xs.iter().enumerate() {
                l.insert(id, x);
                let want = naive(&xs[..=id]);
                assert_eq!(l.last(), want.last(), "{xs:?}");
            }
            assert_eq!(l.take(), naive(&xs), "{xs:?}");
        }
    }
}

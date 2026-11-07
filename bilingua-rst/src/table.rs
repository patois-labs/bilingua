//! Grid and simple tables: finding the cells, and writing translated cells back inside their columns (LocOS
//! `_Grid` and `_Simple`). Column widths never change; a translation that cannot fit is an error naming the
//! table line and the cell.

use crate::pyu::*;
use crate::rx;
use std::collections::{BinaryHeap, HashMap, HashSet};

pub type Rect = (usize, usize, usize, usize);

const GRID_MAX_AREA: usize = 400_000;
const GRID_MAX_STEPS: i64 = 4_000_000;

/// The grid scan used too many probes (a hostile table).
pub struct Complex;

/// One entry per display column: a wide character is followed by an empty pad entry, a combining mark stays with
/// its base character.
pub fn entries(s: &str) -> Vec<&str> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (i, ch) in s.char_indices() {
        let w = cw(ch);
        let e = i + ch.len_utf8();
        if w == 0 {
            if let Some(last) = out.last_mut() {
                last.1 = e;
                continue;
            }
        }
        out.push((i, e));
        if w == 2 {
            out.push((e, e));
        }
    }
    out.into_iter().map(|(a, b)| &s[a..b]).collect()
}

fn pad_to(v: &mut Vec<String>, n: usize) {
    while v.len() < n {
        v.push(" ".to_string());
    }
}

fn owned(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn joined(row: &[&str], a: usize, b: Option<usize>) -> String {
    let n = row.len();
    let lo = a.min(n);
    let hi = b.map_or(n, |b| b.min(n)).max(lo);
    row[lo..hi].concat()
}

/// Greedy wrap by display width; a word wider than `width` breaks only after a wide character. None when
/// something cannot fit.
pub fn wrap(text: &str, width: usize) -> Option<Vec<String>> {
    let mut words: Vec<String> = Vec::new();
    // a word that ends in an odd number of backslashes is an escaped blank (`\ `): it stays with the next word
    let mut glued: Vec<String> = Vec::new();
    let mut carry: Option<String> = None;
    for w in split_ws(text) {
        let w = match carry.take() {
            Some(c) => format!("{c} {w}"),
            None => w.to_string(),
        };
        let slashes = w.len() - w.trim_end_matches('\\').len();
        if slashes % 2 == 1 {
            carry = Some(w);
        } else {
            glued.push(w);
        }
    }
    glued.extend(carry);
    for w in glued.iter().map(String::as_str) {
        if column_width(w) <= width {
            words.push(w.to_string());
            continue;
        }
        let mut piece = String::new();
        let mut pw = 0usize;
        for ch in w.chars() {
            if pw + cw(ch) > width {
                if piece.is_empty() || piece.chars().next_back().map(cw) != Some(2) {
                    return None;
                }
                words.push(std::mem::take(&mut piece));
                pw = 0;
            }
            piece.push(ch);
            pw += cw(ch);
        }
        words.push(piece);
    }
    let mut lines: Vec<String> = Vec::new();
    let (mut cur, mut curw) = (String::new(), 0usize);
    for w in words {
        let ww = column_width(&w);
        if !cur.is_empty() && curw + 1 + ww <= width {
            cur.push(' ');
            cur.push_str(&w);
            curw += 1 + ww;
        } else if cur.is_empty() {
            cur = w;
            curw = ww;
        } else {
            lines.push(std::mem::replace(&mut cur, w));
            curw = ww;
        }
    }
    lines.push(cur);
    Some(lines)
}

/// A cell maps to one unit only when it is one plain paragraph.
fn cell_ok(lines: &[String]) -> bool {
    let nb: Vec<&String> = lines.iter().filter(|l| !blank(l)).collect();
    if nb.is_empty() {
        return true;
    }
    let first = lines.iter().position(|l| !blank(l)).unwrap_or(0);
    let last = lines.iter().rposition(|l| !blank(l)).unwrap_or(0);
    if lines[first..last].iter().any(|l| blank(l)) {
        return false;
    }
    let s = strip(nb[0]);
    !(rx::bullet(s).is_some() || rx::enum_match(s).is_some() || s.starts_with("..") || rx::grid_top_prefix(s))
}

fn cell_raw(lines: &[String]) -> Option<String> {
    if !cell_ok(lines) {
        return None;
    }
    Some(lines.iter().map(|l| strip(l)).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n"))
}

pub fn fit_error(line: usize, txt: &str, width: usize, grow: bool, first: bool) -> String {
    let short: String = if txt.chars().count() <= 40 { txt.to_string() } else { format!("{}...", txt.chars().take(37).collect::<String>()) };
    let why = if first {
        format!("the first column of a simple table holds one line of {width} columns")
    } else if grow {
        "the row cannot grow (it shares its last line with a spanning cell)".to_string()
    } else {
        format!("a word is wider than the column ({width} columns)")
    };
    format!(
        "reStructuredText table at line {line}: the translation '{short}' does not fit its cell: {why}. Column widths are kept as in the source; shorten the translation or widen the column in the source file."
    )
}

// ------------------------------------------------------------------------------------------------ grid

pub struct Grid<'a> {
    orig: Vec<&'a str>,
    rows: Vec<Vec<&'a str>>,
    width: usize,
    pub cells: Vec<Rect>,
    steps: i64,
}

impl<'a> Grid<'a> {
    pub fn new(lines: &[&'a str]) -> Grid<'a> {
        let rows: Vec<Vec<&'a str>> = lines.iter().map(|s| entries(rstrip(s))).collect();
        let width = rows.iter().map(Vec::len).max().unwrap_or(0);
        let mut g = Grid { orig: lines.to_vec(), rows, width, cells: Vec::new(), steps: 0 };
        for r in &mut g.rows {
            while r.len() < width {
                r.push(" ");
            }
        }
        g
    }

    fn at(&mut self, r: usize, c: usize) -> Result<&'a str, Complex> {
        self.steps += 1;
        if self.steps > GRID_MAX_STEPS {
            return Err(Complex);
        }
        let e = self.rows[r][c];
        Ok(if e == "=" { "-" } else { e })
    }

    fn get(&self, r: usize, c: usize) -> &'a str {
        let e = self.rows[r][c];
        if e == "=" {
            "-"
        } else {
            e
        }
    }

    pub fn parse(&mut self) -> Result<bool, Complex> {
        let (h, w) = (self.rows.len(), self.width);
        if h < 3 || self.at(0, 0)? != "+" || self.at(h - 1, 0)? != "+" || h * w > GRID_MAX_AREA {
            return Ok(false);
        }
        self.steps = 0;
        let mut corners: BinaryHeap<std::cmp::Reverse<(usize, usize)>> = BinaryHeap::new();
        corners.push(std::cmp::Reverse((0, 0)));
        let mut done: HashSet<(usize, usize)> = HashSet::new();
        while let Some(std::cmp::Reverse((top, left))) = corners.pop() {
            if done.contains(&(top, left)) || top >= h - 1 || left >= w - 1 {
                continue;
            }
            done.insert((top, left));
            let Some((bottom, right)) = self.scan(top, left)? else { continue };
            self.cells.push((top, left, bottom, right));
            corners.push(std::cmp::Reverse((top, right)));
            corners.push(std::cmp::Reverse((bottom, left)));
        }
        if self.cells.is_empty() {
            return Ok(false);
        }
        let mut covered = vec![false; h * w];
        for &(t, lf, b, rt) in &self.cells {
            for r in t..=b {
                for c in lf..=rt {
                    covered[r * w + c] = true;
                }
            }
        }
        let right_edge = (0..w).filter(|&c| self.rows[0][c] == "+").max().unwrap_or(0);
        for r in 0..h {
            for c in 0..=right_edge {
                if !covered[r * w + c] {
                    return Ok(false);
                }
            }
        }
        self.cells.sort();
        Ok(true)
    }

    fn scan(&mut self, top: usize, left: usize) -> Result<Option<(usize, usize)>, Complex> {
        for right in left + 1..self.width {
            let e = self.at(top, right)?;
            if e == "+" {
                if let Some(bottom) = self.down(top, left, right)? {
                    return Ok(Some((bottom, right)));
                }
            } else if e != "-" {
                return Ok(None);
            }
        }
        Ok(None)
    }

    fn down(&mut self, top: usize, left: usize, right: usize) -> Result<Option<usize>, Complex> {
        for b in top + 1..self.rows.len() {
            let e = self.at(b, right)?;
            if e == "+" {
                if self.closed(top, left, b, right)? {
                    return Ok(Some(b));
                }
            } else if e != "|" {
                return Ok(None);
            }
        }
        Ok(None)
    }

    fn closed(&mut self, top: usize, left: usize, bottom: usize, right: usize) -> Result<bool, Complex> {
        if self.at(bottom, left)? != "+" {
            return Ok(false);
        }
        for c in left + 1..right {
            let e = self.at(bottom, c)?;
            if !(e.is_empty() || e == "-" || e == "+") {
                return Ok(false);
            }
        }
        for r in top + 1..bottom {
            let e = self.at(r, left)?;
            if !(e.is_empty() || e == "|" || e == "+") {
                return Ok(false);
            }
        }
        Ok(true)
    }

    pub fn cell_text(&self, rect: Rect) -> Option<String> {
        let (t, lf, b, rt) = rect;
        let lines: Vec<String> = (t + 1..b).map(|r| self.rows[r][lf + 1..rt].concat()).collect();
        cell_raw(&lines)
    }

    /// Rebuilds the table lines with the new cell texts (rect to one-line text, in the order given).
    pub fn render(&self, new_cells: &[(Rect, String)], slot_line: usize) -> Result<Vec<String>, String> {
        let mut growth: HashMap<usize, usize> = HashMap::new();
        let mut wrapped: Vec<(Rect, Vec<String>)> = Vec::new();
        for (rect, txt) in new_cells {
            let (t, lf, b, rt) = *rect;
            let inner = rt - lf - 1;
            let width = inner.saturating_sub(2).max(1);
            let lines = if blank(txt) { Some(vec![String::new()]) } else { wrap(txt, width) };
            let Some(lines) = lines else { return Err(fit_error(slot_line + t + 1, txt, width, false, false)) };
            let extra = lines.len() as isize - (b - t - 1) as isize;
            if extra > 0 {
                let anchor = b - 1;
                let edges: HashSet<usize> = self.cells.iter().flat_map(|c| [c.1, c.3]).collect();
                if edges.iter().any(|&c| self.get(anchor, c) == "+") {
                    return Err(fit_error(slot_line + t + 1, txt, width, true, false));
                }
                let g = growth.entry(anchor).or_insert(0);
                *g = (*g).max(extra as usize);
            }
            wrapped.push((*rect, lines));
        }
        let mut rows: Vec<Vec<String>> = Vec::new();
        let mut orig_of: Vec<Option<usize>> = Vec::new();
        let mut newpos: Vec<usize> = vec![0; self.rows.len()];
        for (r, row) in self.rows.iter().enumerate() {
            newpos[r] = rows.len();
            rows.push(owned(row));
            orig_of.push(Some(r));
            if let Some(&g) = growth.get(&r) {
                let mut frame = owned(row);
                for &(t, lf, b, rt) in &self.cells {
                    if t < r && r < b {
                        for cell in frame.iter_mut().take(rt).skip(lf + 1) {
                            *cell = " ".to_string();
                        }
                    }
                }
                for _ in 0..g {
                    rows.push(frame.clone());
                    orig_of.push(None);
                }
            }
        }
        for ((t, lf, b, rt), lines) in &wrapped {
            let inner = rt - lf - 1;
            for (k, nr) in (newpos[*t] + 1..newpos[*b]).enumerate() {
                let content = match lines.get(k) {
                    Some(l) if !l.is_empty() => format!(" {l}"),
                    _ => String::new(),
                };
                let mut ent = owned(&entries(&content));
                pad_to(&mut ent, inner);
                ent.truncate(inner);
                rows[nr].splice(lf + 1..*rt, ent);
            }
        }
        let mut out = Vec::with_capacity(rows.len());
        for (nr, row) in rows.iter().enumerate() {
            match orig_of[nr] {
                Some(r) if row.len() == self.rows[r].len() && row.iter().zip(&self.rows[r]).all(|(a, b)| a == b) => out.push(self.orig[r].to_string()),
                _ => out.push(rstrip(&row.concat()).to_string()),
            }
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------------------------------- simple

pub struct SimpleCell {
    pub rows: Vec<usize>,
    pub a: usize,
    pub b: Option<usize>,
    pub first: bool,
    pub raw: Option<String>,
}

pub struct Simple<'a> {
    orig: Vec<&'a str>,
    rows: Vec<Vec<&'a str>>,
    cols: Vec<(usize, usize)>,
    pub cells: Vec<SimpleCell>,
    pub reason: &'static str,
}

fn runs(row: &[&str]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut c = 0usize;
    while c < row.len() {
        if row[c] == "-" {
            let s = c;
            while c < row.len() && row[c] == "-" {
                c += 1;
            }
            out.push((s, c));
        } else {
            c += 1;
        }
    }
    out
}

impl<'a> Simple<'a> {
    pub fn new(lines: &[&'a str]) -> Simple<'a> {
        Simple { orig: lines.to_vec(), rows: lines.iter().map(|s| entries(rstrip(s))).collect(), cols: Vec::new(), cells: Vec::new(), reason: "" }
    }

    pub fn parse(&mut self) -> bool {
        let top = &self.rows[0];
        let mut c = 0usize;
        while c < top.len() {
            if top[c] == "=" {
                let s = c;
                while c < top.len() && top[c] == "=" {
                    c += 1;
                }
                self.cols.push((s, c));
            } else {
                c += 1;
            }
        }
        if self.cols.len() < 2 {
            return false;
        }
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut spans: HashMap<usize, Vec<(usize, usize)>> = HashMap::new();
        let mut cur: Option<usize> = None;
        for r in 1..self.rows.len() {
            let s = self.orig[r];
            if blank(s) || rx::simple_border(s) {
                cur = None;
                continue;
            }
            if rx::simple_span(s) {
                if !groups.is_empty() {
                    spans.insert(groups.len() - 1, runs(&self.rows[r]));
                }
                cur = None;
                continue;
            }
            let (a, b) = self.cols[0];
            let first_blank = blank(&joined(&self.rows[r], a, Some(b)));
            match cur {
                Some(g) if first_blank => groups[g].push(r),
                _ => {
                    groups.push(vec![r]);
                    cur = Some(groups.len() - 1);
                }
            }
        }
        for (gi, rows) in groups.iter().enumerate() {
            let base = spans.get(&gi).cloned().unwrap_or_else(|| self.cols.clone());
            let mut ranges: Vec<(usize, Option<usize>)> = base.iter().map(|&(a, b)| (a, Some(b))).collect();
            if let Some(l) = ranges.last_mut() {
                l.1 = None;
            }
            for &r in rows {
                let row = &self.rows[r];
                let mut pos = 0usize;
                for &(a, b) in &ranges {
                    if !blank(&joined(row, pos, Some(a))) {
                        self.reason = "text across a simple table column gap";
                        return false;
                    }
                    pos = b.unwrap_or(row.len());
                }
            }
            for (ci, &(a, b)) in ranges.iter().enumerate() {
                let lines: Vec<String> = rows.iter().map(|&r| joined(&self.rows[r], a, b)).collect();
                self.cells.push(SimpleCell { rows: rows.clone(), a, b, first: ci == 0, raw: cell_raw(&lines) });
            }
        }
        true
    }

    pub fn render(&self, new_cells: &[(usize, String)], slot_line: usize) -> Result<Vec<String>, String> {
        let mut rows: Vec<Vec<String>> = self.rows.iter().map(|r| owned(r)).collect();
        let mut inserts: HashMap<usize, usize> = HashMap::new();
        let mut plan: Vec<(&SimpleCell, Vec<String>)> = Vec::new();
        for (ci, txt) in new_cells {
            let cell = &self.cells[*ci];
            let line = slot_line + cell.rows[0];
            let lines = match cell.b {
                None => Some(vec![txt.clone()]),
                Some(b) => wrap(txt, b - cell.a),
            };
            let bad = match &lines {
                None => true,
                Some(l) => cell.first && l.len() > 1,
            };
            if bad {
                return Err(fit_error(line, txt, cell.b.unwrap_or(0).saturating_sub(cell.a), false, cell.first));
            }
            let lines = lines.unwrap_or_default();
            let extra = lines.len() as isize - cell.rows.len() as isize;
            if extra > 0 {
                let last = *cell.rows.last().unwrap_or(&0);
                let g = inserts.entry(last).or_insert(0);
                *g = (*g).max(extra as usize);
            }
            plan.push((cell, lines));
        }
        let mut layout: Vec<(Option<usize>, Vec<String>)> = Vec::new();
        let mut index_of: Vec<usize> = vec![0; rows.len()];
        for (r, row) in rows.drain(..).enumerate() {
            index_of[r] = layout.len();
            layout.push((Some(r), row));
            for _ in 0..inserts.get(&r).copied().unwrap_or(0) {
                layout.push((None, Vec::new()));
            }
        }
        for (cell, lines) in &plan {
            let (a, b) = (cell.a, cell.b);
            let first = index_of[cell.rows[0]];
            let mut last = index_of[*cell.rows.last().unwrap_or(&0)];
            while last + 1 < layout.len() && layout[last + 1].0.is_none() {
                last += 1;
            }
            for (k, li) in (first..=last).enumerate() {
                let row = &mut layout[li].1;
                let txt = lines.get(k).map(String::as_str).unwrap_or("");
                let mut ent = owned(&entries(txt));
                match b {
                    None => {
                        row.truncate(a);
                        pad_to(row, a);
                        row.extend(ent);
                    }
                    Some(b) => {
                        pad_to(row, b);
                        pad_to(&mut ent, b - a);
                        row.splice(a..b, ent);
                    }
                }
            }
        }
        let mut out = Vec::new();
        for (r, row) in &layout {
            if let Some(r) = r {
                if row.len() == self.rows[*r].len() && row.iter().zip(&self.rows[*r]).all(|(x, y)| x == y) {
                    out.push(self.orig[*r].to_string());
                    continue;
                }
            }
            let line = rstrip(&row.concat()).to_string();
            if line.is_empty() && r.is_none_or(|r| !blank(self.orig[r])) {
                continue;
            }
            out.push(line);
        }
        Ok(out)
    }
}

/// `_simple_ends`: ends[j] is the first k >= j that is a simple-table border followed by a blank line or the end
/// (n when there is none).
pub fn simple_ends(lines: &[&str]) -> Vec<usize> {
    let n = lines.len();
    let mut out = vec![n; n + 1];
    for j in (0..n).rev() {
        let closes = rx::simple_border(lines[j]) && (j + 1 >= n || blank(lines[j + 1]));
        out[j] = if closes { j } else { out[j + 1] };
    }
    out
}

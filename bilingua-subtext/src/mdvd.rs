//! MicroDVD cue bodies: control codes as tags (paired for line-scoped style codes, #680) and the way back.

use crate::scan::*;
use bilingua_model::*;
use std::collections::{BTreeMap, HashMap, HashSet};

pub struct Built {
    pub inl: Vec<Inline>,
    pub codes: BTreeMap<String, CodeData>,
    /// Pair ids whose style code opens its line in the source.
    pub heads: Vec<String>,
}

enum Piece<'a> {
    Text(&'a str),
    Code(&'a str),
}

fn add_text<'a>(lines: &mut Vec<Vec<Piece<'a>>>, s: &'a str) {
    for (j, seg) in s.split('|').enumerate() {
        if j > 0 {
            lines.push(vec![]);
        }
        if !seg.is_empty() {
            lines.last_mut().unwrap().push(Piece::Text(seg));
        }
    }
}

fn push_text(v: &mut Vec<Inline>, s: &str) {
    if s.is_empty() {
        return;
    }
    if let Some(Inline::Text(t)) = v.last_mut() {
        t.push_str(s);
    } else {
        v.push(Inline::Text(s.to_string()));
    }
}

fn label(code: &str) -> &'static str {
    match code.as_bytes()[1].to_ascii_lowercase() {
        b'y' => "style",
        b'c' => "colour",
        b'f' => "font",
        b's' => "size",
        b'p' => "position",
        b'h' => "charset",
        _ => "control code",
    }
}

/// True when a text piece holds words once LocOS tokens are removed. Text pieces of a unit are always separated by
/// tags (tokens in LocOS's string), and a token never spans one, so piece by piece is the LocOS reading of the whole.
fn piece_words(t: &str) -> bool {
    !py_strip(&strip_placeholders(t)).is_empty()
}

/// The unit of a cue body (leading codes already outside it). Linear in the body (gate: the "words before this
/// pair" and "words after this run" checks were quadratic in the number of codes on a line).
pub fn build(body: &str) -> Built {
    let mut lines: Vec<Vec<Piece>> = vec![vec![]];
    let mut pos = 0;
    for (a, b) in find_codes(body) {
        add_text(&mut lines, &body[pos..a]);
        lines.last_mut().unwrap().push(Piece::Code(&body[a..b]));
        pos = b;
    }
    add_text(&mut lines, &body[pos..]);

    let mut codes = BTreeMap::new();
    let mut heads = Vec::new();
    let mut n = 0usize;
    let mut out: Vec<Inline> = Vec::new();
    for (li, line) in lines.iter().enumerate() {
        if li > 0 {
            push_text(&mut out, "\n");
        }
        let mut cur: Vec<Inline> = Vec::new();
        let mut opened: Vec<usize> = Vec::new();
        // words_after[i]: a text piece at or after i has a non-space character (LocOS: `rest.strip()`)
        let mut words_after = vec![false; line.len() + 1];
        for i in (0..line.len()).rev() {
            words_after[i] = words_after[i + 1] || matches!(line[i], Piece::Text(t) if !py_strip(t).is_empty());
        }
        let mut words_before = false;
        let mut k = 0;
        while k < line.len() {
            if let Piece::Text(t) = line[k] {
                push_text(&mut cur, t);
                words_before |= piece_words(t);
                k += 1;
                continue;
            }
            let mut j = k;
            while j < line.len() && matches!(line[j], Piece::Code(_)) {
                j += 1;
            }
            let run: Vec<&str> = line[k..j].iter().filter_map(|p| if let Piece::Code(c) = p { Some(*c) } else { None }).collect();
            let rest_words = words_after[j];
            let styles: Vec<&str> = if rest_words { run.iter().copied().filter(|c| c.as_bytes()[1].is_ascii_lowercase()).collect() } else { vec![] };
            for code in &run {
                if !styles.contains(code) {
                    n += 1;
                    let id = n.to_string();
                    codes.insert(id.clone(), CodeData { standalone: Some(code.to_string()), label: Some(label(code).into()), ..Default::default() });
                    cur.push(Inline::tag(id, TagKind::Standalone, *code));
                }
            }
            if !styles.is_empty() {
                n += 1;
                let id = n.to_string();
                let joined = styles.concat();
                codes.insert(id.clone(), CodeData { open: Some(joined.clone()), close: Some(String::new()), label: Some(label(styles[0]).into()), ..Default::default() });
                if !words_before {
                    heads.push(id.clone());
                }
                cur.push(Inline::tag(id, TagKind::Open, joined));
                opened.push(n);
            }
            k = j;
        }
        if let Some(Inline::Text(t)) = cur.first_mut() {
            *t = t.trim_start_matches([' ', '\t']).to_string();
        }
        if let Some(Inline::Text(t)) = cur.last_mut() {
            *t = t.trim_end_matches([' ', '\t']).to_string();
        }
        cur.retain(|i| !matches!(i, Inline::Text(t) if t.is_empty()));
        for g in opened.iter().rev() {
            cur.push(Inline::tag(g.to_string(), TagKind::Close, "{/}"));
        }
        for i in cur {
            match i {
                Inline::Text(t) => push_text(&mut out, &t),
                tag => out.push(tag),
            }
        }
    }
    Built { inl: out, codes, heads }
}

/// True when the unit holds something to translate besides codes.
pub fn has_words(inl: &[Inline]) -> bool {
    inl.iter().any(|i| matches!(i, Inline::Text(t) if piece_words(t)))
}

struct R {
    out: Vec<String>,
    head: usize,
    open_pairs: Vec<String>,
    emitted: usize,
    /// Bytes written so far and the most this cue may take: every line break inside open pairs writes their codes
    /// again, so a translation that opens many pairs and then breaks many lines would multiply them (gate).
    size: usize,
    cap: usize,
}

impl R {
    fn push(&mut self, s: String) {
        self.size += s.len();
        self.out.push(s);
    }

    fn text(&mut self, s: &str, pair_code: &dyn Fn(&str) -> String) {
        for (j, seg) in split_newlines(s).into_iter().enumerate() {
            if self.size > self.cap {
                return;
            }
            if j > 0 {
                self.push("\n".into());
                self.head = self.out.len();
                let mut codes = String::new();
                for id in &self.open_pairs {
                    let c = pair_code(id);
                    self.emitted += count_codes(&c);
                    codes.push_str(&c);
                    if self.size + codes.len() > self.cap {
                        break;
                    }
                }
                self.push(codes);
            }
            self.push(seg.to_string());
        }
    }
}

/// A translated unit as MicroDVD text (`|` for line breaks). Errors say why the translation cannot be written.
pub fn render(target: &[Inline], codes: &BTreeMap<String, CodeData>, heads: &HashSet<&str>, fps: bool) -> Result<String, String> {
    let mut t = merge_text(target.to_vec());
    if let Some(Inline::Text(s)) = t.first_mut() {
        *s = py_lstrip(s).to_string();
    }
    if let Some(Inline::Text(s)) = t.last_mut() {
        *s = py_rstrip(s).to_string();
    }
    t.retain(|i| !matches!(i, Inline::Text(s) if s.is_empty()));
    let point = |id: &str| if id.starts_with('~') { None } else { codes.get(id).and_then(|c| c.standalone.as_deref()) };
    let pair = |id: &str| codes.get(id).filter(|c| !id.starts_with('~') && c.close.is_some()).and_then(|c| c.open.as_deref());
    let mut opens: HashMap<&str, usize> = HashMap::new();
    let mut closes: HashMap<&str, usize> = HashMap::new();
    for (k, i) in t.iter().enumerate() {
        if let Inline::Tag { id, kind, .. } = i {
            if pair(id).is_none() {
                continue;
            }
            match kind {
                TagKind::Open => {
                    opens.entry(id).or_insert(k);
                }
                TagKind::Close if opens.contains_key(id.as_str()) => {
                    closes.entry(id).or_insert(k);
                }
                _ => {}
            }
        }
    }
    // words[k]: text pieces with words before inline k, so "is this pair empty" is one subtraction (gate: nested pairs
    // made the old per-pair scan quadratic)
    let mut words = Vec::with_capacity(t.len() + 1);
    words.push(0usize);
    for i in &t {
        let w = matches!(i, Inline::Text(s) if piece_words(s));
        words.push(words.last().unwrap() + w as usize);
    }
    let mut empty: HashSet<&str> = HashSet::new();
    for (id, &b) in &closes {
        let a = opens[id];
        if words[b] == words[a + 1] {
            empty.insert(id);
        }
    }
    let pair_code = |id: &str| -> String { codes.get(id).and_then(|c| c.open.clone()).unwrap_or_default() };
    let input: usize = t.iter().map(|i| if let Inline::Text(s) = i { s.len() } else { 0 }).sum::<usize>()
        + codes.iter().filter(|(k, _)| !k.starts_with('~')).map(|(_, c)| c.open.as_ref().or(c.standalone.as_ref()).map_or(0, |s| s.len())).sum::<usize>();
    let cap = input.saturating_mul(16).saturating_add(64 * 1024);
    let mut r = R { out: vec![String::new()], head: 0, open_pairs: vec![], emitted: 0, size: 0, cap };
    for (k, i) in t.iter().enumerate() {
        match i {
            Inline::Text(s) => r.text(s, &pair_code),
            Inline::Tag { id, kind: TagKind::Standalone, .. } => {
                if let Some(c) = point(id) {
                    r.emitted += count_codes(c);
                    r.push(c.to_string());
                }
            }
            Inline::Tag { id, kind, .. } => {
                let (Some(&a), Some(&b)) = (opens.get(id.as_str()), closes.get(id.as_str())) else { continue };
                if empty.contains(id.as_str()) {
                    continue;
                }
                match kind {
                    TagKind::Open if k == a => {
                        let c = pair_code(id);
                        r.emitted += count_codes(&c);
                        if heads.contains(id.as_str()) {
                            let h = r.head;
                            r.size += c.len();
                            r.out[h].push_str(&c);
                        } else {
                            r.push(c);
                        }
                        r.open_pairs.push(id.clone());
                    }
                    TagKind::Close if k == b => {
                        // pairs close mostly in reverse order: search from the end
                        if let Some(p) = r.open_pairs.iter().rposition(|x| x == id) {
                            r.open_pairs.remove(p);
                        }
                    }
                    _ => {}
                }
            }
        }
        if r.size > r.cap {
            return Err("the translation would write its style codes again on too many lines".into());
        }
    }
    let joined = r.out.concat();
    let joined = py_strip(&joined);
    let res = collapse_newlines(joined, "|");
    if count_codes(&res) != r.emitted {
        return Err("the translation holds text that reads as a MicroDVD control code ({y:i}, {c:$..})".into());
    }
    let mut bare = String::with_capacity(res.len());
    let mut pos = 0;
    for (a, b) in find_codes(&res) {
        bare.push_str(&res[pos..a]);
        pos = b;
    }
    bare.push_str(&res[pos..]);
    if py_strip(&strip_placeholders(&bare.replace('|', ""))).is_empty() {
        return Err("the translation is empty (nothing but control codes and line breaks)".into());
    }
    if fps && fps_text(&res) {
        return Err("the translation reads as a frame-rate declaration".into());
    }
    Ok(res)
}

//! Inline tokenizer: turns the source text of one unit into text plus tags. A point tag stands for something that is
//! never translated (a template, a URL, a link target, a tag); a paired tag wraps text that is (a link label, bold,
//! italic, a footnote). The search order of the alternatives is the one of the LocOS filter's regular expression.

use crate::lex::*;
use bilingua_model::{CodeData, Inline, TagKind};
use std::collections::{BTreeMap, HashMap};

const MAX_TOKENS: usize = 1000;
const MAX_DEPTH: usize = 8;

pub struct Ctx<'a> {
    pub text: &'a str,
    pub mask: &'a str,
    /// Start of each protected region to its end.
    pub starts: &'a HashMap<usize, usize>,
    /// `[[` start to its `]]` start (MediaWiki).
    pub links: &'a HashMap<usize, usize>,
    pub dw: bool,
}

enum Kind {
    Rg,
    Wl,
    El(Option<usize>),
    Plain, // url, tag, entity, email, line break, monospace
    Quote(usize, usize),
    DLink(usize, usize),
    Media(usize, usize),
    Foot(usize, usize),
    Bold(usize, usize),
    Ital(usize, usize),
    Under(usize, usize),
}

struct Hit {
    s: usize,
    e: usize,
    k: Kind,
}

// ---------------------------------------------------------------------------------------- scanners

/// `(?<![\w/])(?:https?|ftps?)://[^\s\[\]<>"\x00]{1,2000}` (DokuWiki also excludes `|`).
fn url_at(m: &str, i: usize, limit: usize, dw: bool) -> Option<usize> {
    if char_before(m, i).is_some_and(|c| is_word(c) || c == '/') {
        return None;
    }
    let b = m.as_bytes();
    let rest = &b[i..limit];
    let after = if rest.starts_with(b"https://") {
        i + 8
    } else if rest.starts_with(b"http://") || rest.starts_with(b"ftps://") {
        i + 7
    } else if rest.starts_with(b"ftp://") {
        i + 6
    } else {
        return None;
    };
    let e = run(m, after, limit, 2000, |c| !(is_space(c) || matches!(c, '[' | ']' | '<' | '>' | '"' | '\0') || (dw && c == '|')));
    (e > after).then_some(e)
}

/// `</?[A-Za-z][A-Za-z0-9]{0,40}(?:[\s/][^<>\r\n]{0,1000})?>`
fn tag_at(m: &str, i: usize, limit: usize) -> Option<usize> {
    let b = m.as_bytes();
    let mut j = i + 1;
    if b.get(j) == Some(&b'/') {
        j += 1;
    }
    if j >= limit || !b[j].is_ascii_alphabetic() {
        return None;
    }
    j += 1;
    let mut n = 0;
    while n < 40 && j < limit && b[j].is_ascii_alphanumeric() {
        j += 1;
        n += 1;
    }
    if j >= limit {
        return None;
    }
    if b[j] == b'>' {
        return Some(j + 1);
    }
    let c = char_at(m, j)?;
    if !(is_space(c) || c == '/') {
        return None;
    }
    j += c.len_utf8();
    let e = run(m, j, limit, 1000, |c| !matches!(c, '<' | '>' | '\r' | '\n'));
    (e < limit && b[e] == b'>').then_some(e + 1)
}

/// `&(?:#\d{1,7}|#[xX][0-9a-fA-F]{1,6}|[A-Za-z][A-Za-z0-9]{1,30});`
fn ent_at(m: &str, i: usize, limit: usize) -> Option<usize> {
    let b = m.as_bytes();
    let j = i + 1;
    if b.get(j) == Some(&b'#') && j < limit {
        let e = run(m, j + 1, limit, 7, is_decimal);
        if e > j + 1 && b.get(e) == Some(&b';') && e < limit {
            return Some(e + 1);
        }
        if matches!(b.get(j + 1), Some(b'x' | b'X')) {
            let e = run(m, j + 2, limit, 6, |c| c.is_ascii_hexdigit());
            if e > j + 2 && b.get(e) == Some(&b';') && e < limit {
                return Some(e + 1);
            }
        }
        return None;
    }
    if j < limit && b[j].is_ascii_alphabetic() {
        let e = run(m, j + 1, limit, 30, |c| c.is_ascii_alphanumeric());
        if e > j + 1 && b.get(e) == Some(&b';') && e < limit {
            return Some(e + 1);
        }
    }
    None
}

/// `\[(?:https?:|ftps?:|mailto:|irc:|//)[^\s\[\]\x00]{1,2000}(?:[ \t]+(?P<elt>[^\]\r\n]{0,2000}))?\]`
fn ext_link_at(m: &str, i: usize, limit: usize, sk: &mut Seekers) -> Option<Hit> {
    let b = m.as_bytes();
    let rest = &b[i + 1..limit];
    let scheme = ["https:", "http:", "ftps:", "ftp:", "mailto:", "irc:", "//"].iter().find(|s| rest.starts_with(s.as_bytes()))?;
    let after = i + 1 + scheme.len();
    let e = run(m, after, limit, 2000, |c| !(is_space(c) || matches!(c, '[' | ']' | '\0')));
    if e == after || e >= limit {
        return None;
    }
    if b[e] == b']' {
        return Some(Hit { s: i, e: e + 1, k: Kind::El(None) });
    }
    if b[e] != b' ' && b[e] != b'\t' {
        return None;
    }
    let mut es = e;
    while es < limit && (b[es] == b' ' || b[es] == b'\t') {
        es += 1;
    }
    let ee = sk.label_end(m, es, limit)?;
    Some(Hit { s: i, e: ee + 1, k: Kind::El(Some(es)) })
}

fn quote_at(m: &str, i: usize, limit: usize) -> Option<Hit> {
    let b = m.as_bytes();
    if at(b, i, limit, b"'''''") {
        if let Some((ie, e)) = lazy_close(m, i + 5, limit, 1, 500, b"'''''") {
            return Some(Hit { s: i, e, k: Kind::Quote(i + 5, ie) });
        }
    }
    if at(b, i, limit, b"'''") {
        if let Some((ie, e)) = lazy_close(m, i + 3, limit, 1, 500, b"'''") {
            return Some(Hit { s: i, e, k: Kind::Quote(i + 3, ie) });
        }
    }
    if at(b, i, limit, b"''") && i + 2 < limit {
        let c = char_at(m, i + 2)?;
        if !matches!(c, '\r' | '\n' | '\'') {
            if let Some((ie, e)) = lazy_close(m, i + 2 + c.len_utf8(), limit, 0, 499, b"''") {
                return Some(Hit { s: i, e, k: Kind::Quote(i + 2, ie) });
            }
        }
    }
    None
}

fn mw_search(m: &str, pos: usize, limit: usize, sk: &mut Seekers) -> Option<Hit> {
    let b = m.as_bytes();
    let mut i = pos;
    while i < limit {
        let c = b[i];
        let hit = match c {
            0 => {
                let e = b[i..limit].iter().position(|c| *c != 0).map_or(limit, |p| i + p);
                Some(Hit { s: i, e, k: Kind::Rg })
            }
            b'[' => {
                if at(b, i, limit, b"[[") {
                    Some(Hit { s: i, e: i + 2, k: Kind::Wl })
                } else {
                    ext_link_at(m, i, limit, sk)
                }
            }
            b'h' | b'f' => url_at(m, i, limit, false).map(|e| Hit { s: i, e, k: Kind::Plain }),
            b'\'' => quote_at(m, i, limit),
            b'<' => tag_at(m, i, limit).map(|e| Hit { s: i, e, k: Kind::Plain }),
            b'&' => ent_at(m, i, limit).map(|e| Hit { s: i, e, k: Kind::Plain }),
            _ => None,
        };
        if hit.is_some() {
            return hit;
        }
        i += 1;
    }
    None
}

fn email_at(m: &str, i: usize, limit: usize) -> Option<usize> {
    let b = m.as_bytes();
    let a = run(m, i + 1, limit, 200, |c| is_word(c) || matches!(c, '.' | '+' | '-'));
    if a == i + 1 || b.get(a) != Some(&b'@') || a >= limit {
        return None;
    }
    let d = run(m, a + 1, limit, 200, |c| is_word(c) || matches!(c, '.' | '-'));
    (d > a + 1 && d < limit && b[d] == b'>').then_some(d + 1)
}

fn dw_search(m: &str, pos: usize, limit: usize, sk: &mut Seekers) -> Option<Hit> {
    let b = m.as_bytes();
    let mut i = pos;
    while i < limit {
        let c = b[i];
        let hit = match c {
            0 => {
                let e = b[i..limit].iter().position(|c| *c != 0).map_or(limit, |p| i + p);
                Some(Hit { s: i, e, k: Kind::Rg })
            }
            b'[' if at(b, i, limit, b"[[") => sk.lazy_far(m, i + 2, limit, 0, 2000, false).map(|(ie, e)| Hit { s: i, e, k: Kind::DLink(i + 2, ie) }),
            b'{' if at(b, i, limit, b"{{") => {
                let ie = run(m, i + 2, limit, 2000, |c| !matches!(c, '{' | '}' | '\r' | '\n'));
                at(b, ie, limit, b"}}").then_some(Hit { s: i, e: ie + 2, k: Kind::Media(i + 2, ie) })
            }
            b'(' if at(b, i, limit, b"((") => sk.lazy_far(m, i + 2, limit, 1, 2000, true).map(|(ie, e)| Hit { s: i, e, k: Kind::Foot(i + 2, ie) }),
            b'<' => email_at(m, i, limit).or_else(|| tag_at(m, i, limit)).map(|e| Hit { s: i, e, k: Kind::Plain }),
            b'h' | b'f' => url_at(m, i, limit, true).map(|e| Hit { s: i, e, k: Kind::Plain }),
            b'\\' if at(b, i, limit, b"\\\\") && (i + 2 == limit || matches!(b[i + 2], b' ' | b'\t' | b'\r' | b'\n')) => {
                Some(Hit { s: i, e: i + 2, k: Kind::Plain })
            }
            b'\'' => dd(m, i, limit, b"''").map(|(_, e)| Hit { s: i, e, k: Kind::Plain }),
            b'*' => dd(m, i, limit, b"**").map(|(ie, e)| Hit { s: i, e, k: Kind::Bold(i + 2, ie) }),
            b'/' => dd(m, i, limit, b"//").map(|(ie, e)| Hit { s: i, e, k: Kind::Ital(i + 2, ie) }),
            b'_' => dd(m, i, limit, b"__").map(|(ie, e)| Hit { s: i, e, k: Kind::Under(i + 2, ie) }),
            _ => None,
        };
        if hit.is_some() {
            return hit;
        }
        i += 1;
    }
    None
}

// ---------------------------------------------------------------------------------------- file links

/// The File / Image / Media namespace names (and the common local aliases) of the larger wikis. MediaWiki
/// reads every one of them as a file link, so `[[Файл:a.jpg|мини|text]]` has options in front of its caption.
const FILE_NAMESPACES: &[&str] = &[
    "file", "image", "media", "datei", "bild", "medium", "fichier", "image", "archivo", "imagen", "immagine", "ficheiro", "arquivo", "imagem",
    "файл", "изображение", "медиа", "зображення", "ファイル", "画像", "メディア", "文件", "檔案", "档案", "图像", "圖像", "媒体", "ملف", "صورة", "وسائط",
    "bestand", "afbeelding", "plik", "grafika", "fil", "billede", "bilde", "tiedosto", "kuva", "soubor", "obrázek", "súbor", "obrázok", "fájl", "kép",
    "fișier", "imagine", "datoteka", "slika", "dosya", "resim", "파일", "그림", "קובץ", "תמונה", "پرونده", "تصویر", "tập tin", "hình", "berkas",
    "gambar", "fitxer", "arquivo", "αρχείο", "εικόνα", "ไฟล์", "ဖိုင်", "फ़ाइल", "फाइल", "चित्र", "मीडिया", "চিত্র", "ফাইল", "ಚಿತ್ರ", "ഫയൽ", "கோப்பு", "ឯកសារ", "файл", "суроат",
];

pub fn file_ns(target: &str) -> bool {
    let t = target.trim_start_matches([' ', '\t']);
    let Some((ns, _)) = t.split_once(':') else { return false };
    let ns = ns.trim_matches([' ', '\t']).replace('_', " ").to_lowercase();
    FILE_NAMESPACES.contains(&ns.as_str())
}

/// Localized spellings of the image options that decide how a picture is placed (the usual ones on the larger wikis).
const LOCAL_OPTIONS: &[&str] = &[
    "миниатюра", "мини", "слева", "справа", "центр", "без", "рамка", "миниатюр", "vignette", "gauche", "droite", "centré", "sans", "cadre", "miniatur",
    "mini", "links", "rechts", "zentriert", "ohne", "rahmen", "miniaturadeimagen", "izquierda", "derecha", "centro", "ninguna", "marco", "miniatura",
    "esquerda", "direita", "centro", "sinistra", "destra", "centrato", "senza", "cornice", "tegn", "venstre", "høyre", "midtstilt", "ramme",
    "thumb", "minigatuur", "links", "rechts", "gecentreerd", "geen", "kader", "mały", "lewej", "prawej", "środek", "ramka", "bez", "ingen",
    "تصغير", "يمين", "يسار", "مركز", "إطار", "بدون", "缩略图", "縮圖", "左", "右", "居中", "无", "無", "框", "サムネイル", "左", "右", "中央", "なし", "枠",
    "섬네일", "썸네일", "왼쪽", "오른쪽", "가운데", "없음", "küçükresim", "sol", "sağ", "ortala", "yok", "çerçeve", "ממוזער", "שמאל", "ימין", "מרכז",
    "بندانگشتی", "چپ", "راست", "وسط", "بدون‌قاب", "thumbnail", "trái", "phải", "giữa", "không", "khung", "bingkai", "kiri", "kanan", "tengah",
    "miniatyr", "vänster", "höger", "centrerad", "ingen", "ram", "pienoiskuva", "vasen", "oikea", "keskitetty", "pienoiskuva", "ei", "kehys",
    "náhled", "vlevo", "vpravo", "střed", "bez", "rámeček", "bélyegkép", "balra", "jobbra", "középre", "nincs", "keret", "μικρογραφία", "αριστερά",
    "δεξιά", "κέντρο", "χωρίς", "πλαίσιο", "अंगूठाकार", "अंगूठा", "बाएँ", "बाएं", "बायें", "दाएँ", "दाएं", "दायें", "केन्द्र", "केंद्र", "केंद्रित", "केन्द्रित", "कोई_नहीं", "कोई नहीं", "फ्रेम", "फ़्रेम", "फ़्रेमहीन", "फ्रेमहीन", "миниатюрa", "ліворуч", "праворуч", "центр", "міні", "мініатюра", "рамка",
];

/// Localized spellings of the `px` size unit (`250بك`).
const LOCAL_PX: &[&str] = &["بك", "بكسل", "پیکسل", "像素", "픽셀", "пкс", "ピクセル"];

/// A file link part that is an option (`thumb`, `200px`, `alt=..`), not a caption.
fn file_option(cap: &str) -> bool {
    let s = strip(cap).to_lowercase();
    const WORDS: &[&str] = &[
        "thumb", "thumbnail", "frame", "framed", "frameless", "border", "left", "right", "center", "centre", "none", "upright", "baseline", "sub",
        "super", "top", "text-top", "middle", "bottom", "text-bottom", "loop", "muted",
    ];
    // the local words are matched as written (MediaWiki's image options are case sensitive), so a one-word
    // caption such as `Marco` or `Links` stays a caption
    if WORDS.contains(&s.as_str()) || LOCAL_OPTIONS.contains(&strip(cap)) {
        return true;
    }
    // a size with a local unit, as in `250بك` (the units of the larger wikis only: `2010年` is a caption)
    {
        let digits = s.trim_start_matches(is_decimal);
        let unit = digits.strip_prefix('x').map_or(digits, |r| r.trim_start_matches(is_decimal));
        if unit.len() < s.len() && LOCAL_PX.contains(&unit) {
            return true;
        }
    }
    // \d*(?:x\d+)?[ \t]*px
    let mut it = s.as_str();
    it = it.trim_start_matches(is_decimal);
    if let Some(r) = it.strip_prefix('x') {
        let r2 = r.trim_start_matches(is_decimal);
        if r2.len() < r.len() {
            it = r2;
        }
    }
    if it.trim_start_matches([' ', '\t']) == "px" {
        return true;
    }
    for k in ["upright", "link", "alt", "page", "class", "lang", "start", "end", "thumbtime", "thumb"] {
        if let Some(r) = s.strip_prefix(k) {
            if r.trim_start_matches([' ', '\t']).starts_with('=') {
                return true;
            }
        }
    }
    false
}

fn plain(s: &str) -> String {
    if !s.contains(['\r', '\n']) {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut floor = 0usize;
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\r' || c == '\n' {
            if c == '\r' && it.peek() == Some(&'\n') {
                it.next();
            }
            while out.len() > floor && out.ends_with([' ', '\t']) {
                out.pop();
            }
            while matches!(it.peek(), Some(' ' | '\t')) {
                it.next();
            }
            out.push(' ');
            floor = out.len();
        } else {
            out.push(c);
        }
    }
    out
}

fn label_of(raw: &str) -> &'static str {
    if raw.starts_with("<!--") {
        "comment"
    } else if raw.starts_with("{{{") {
        "parameter"
    } else if raw.starts_with("{{") {
        "template"
    } else if raw.starts_with("[[") || raw.starts_with('[') {
        "link"
    } else if raw.starts_with("http") || raw.starts_with("ftp") {
        "url"
    } else if raw.starts_with('&') {
        "entity"
    } else if raw.starts_with("~~~") {
        "signature"
    } else if raw.starts_with("__") || raw.starts_with("~~") {
        "magic word"
    } else if raw.starts_with("\\\\") {
        "line break"
    } else if raw.starts_with("''") {
        "monospace"
    } else if raw.starts_with('<') {
        "html"
    } else {
        "code"
    }
}

fn pair_label(open: &str) -> &'static str {
    match open {
        "'''''" => "bold italic",
        "'''" | "**" => "bold",
        "''" | "//" => "italic",
        "__" => "underline",
        "((" => "footnote",
        _ if open.starts_with("{{") => "media caption",
        _ => "link",
    }
}

pub struct Tok<'a> {
    cx: &'a Ctx<'a>,
    n: usize,
    pub codes: BTreeMap<String, CodeData>,
    seek: Seekers,
}

impl<'a> Tok<'a> {
    pub fn new(cx: &'a Ctx<'a>) -> Tok<'a> {
        Tok { cx, n: 0, codes: BTreeMap::new(), seek: Seekers::default() }
    }

    fn point(&mut self, raw: &str, out: &mut Vec<Inline>) {
        self.n += 1;
        let id = self.n.to_string();
        self.codes.insert(id.clone(), CodeData::new().with_standalone(raw.to_string()).with_label(label_of(raw).to_string()));
        out.push(Inline::tag(id, TagKind::Standalone, raw));
    }

    #[allow(clippy::too_many_arguments)]
    fn pair(&mut self, s: usize, a: usize, b: usize, e: usize, depth: usize, out: &mut Vec<Inline>) {
        let t = self.cx.text;
        let (open, close) = (sl(t, s, a), sl(t, b, e));
        self.n += 1;
        let id = self.n.to_string();
        self.codes.insert(
            id.clone(),
            CodeData::new().with_open(open.to_string()).with_close(close.to_string()).with_label(pair_label(open).to_string()),
        );
        out.push(Inline::tag(id.clone(), TagKind::Open, open));
        self.run(a, b, depth + 1, out);
        out.push(Inline::tag(id, TagKind::Close, close));
    }

    /// A paired tag around `[a, b)` with the white space at its edges left in the markup.
    fn pair_trim(&mut self, s: usize, mut a: usize, mut b: usize, e: usize, depth: usize, out: &mut Vec<Inline>) {
        let t = self.cx.text;
        while a < b && char_at(t, a).is_some_and(is_space) {
            a += char_at(t, a).map_or(1, char::len_utf8);
        }
        while b > a && char_before(t, b).is_some_and(is_space) {
            b -= char_before(t, b).map_or(1, char::len_utf8);
        }
        self.pair(s, a, b, e, depth, out);
    }

    fn region_tokens(&mut self, a: usize, b: usize, out: &mut Vec<Inline>) {
        let mut pos = a;
        while pos < b {
            let mut e = match self.cx.starts.get(&pos) {
                Some(&e) if e <= b => e,
                _ => {
                    let mut e = pos + 1;
                    while e < b && !self.cx.starts.contains_key(&e) {
                        e += 1;
                    }
                    e
                }
            };
            if !self.cx.text.is_char_boundary(e) {
                e = b;
            }
            let raw = sl(self.cx.text, pos, e);
            self.point(raw, out);
            pos = e;
        }
    }

    /// Tokenizes `[a, b)` of the source, appending to `out`.
    pub fn run(&mut self, a: usize, b: usize, depth: usize, out: &mut Vec<Inline>) {
        let (t, m) = (self.cx.text, self.cx.mask);
        let mut pos = a;
        while pos < b {
            if self.n >= MAX_TOKENS {
                self.point(sl(t, pos, b), out);
                return;
            }
            let hit = if self.cx.dw { dw_search(m, pos, b, &mut self.seek) } else { mw_search(m, pos, b, &mut self.seek) };
            let Some(h) = hit else { break };
            let p = plain(sl(t, pos, h.s));
            if !p.is_empty() {
                out.push(Inline::Text(p));
            }
            let (s, e) = (h.s, h.e);
            pos = e;
            if matches!(h.k, Kind::Rg) {
                self.region_tokens(s, e, out);
            } else if depth >= MAX_DEPTH {
                if let (false, Kind::Wl) = (self.cx.dw, &h.k) {
                    if let Some(&end) = self.cx.links.get(&s).filter(|end| **end < b) {
                        pos = end + 2;
                    }
                }
                self.point(sl(t, s, pos.max(e)), out);
                pos = pos.max(e);
            } else if self.cx.dw {
                self.dw_inline(&h, depth, out);
            } else {
                pos = self.mw_inline(&h, b, depth, out);
            }
        }
        if pos < b {
            let p = plain(sl(t, pos, b));
            if !p.is_empty() {
                out.push(Inline::Text(p));
            }
        }
    }

    fn mw_inline(&mut self, h: &Hit, b: usize, depth: usize, out: &mut Vec<Inline>) -> usize {
        let (t, m) = (self.cx.text, self.cx.mask);
        let (s, e) = (h.s, h.e);
        match &h.k {
            Kind::Wl => {
                let end = match self.cx.links.get(&s) {
                    Some(&end) if end < b => end,
                    _ => {
                        self.point("[[", out);
                        return e;
                    }
                };
                let close_end = end + 2;
                let pipes = top_pipes(m, s + 2, end, self.cx.links);
                let target = sl(m, s + 2, pipes.first().copied().unwrap_or(end));
                // a target that is a template call (`{{Localized media|x|svg}}`) is a file when an option follows it
                let templated_file = strip(sl(t, s + 2, pipes.first().copied().unwrap_or(end))).starts_with("{{")
                    && pipes.len() >= 2
                    && pipes.windows(2).any(|w| file_option(sl(m, w[0] + 1, w[1])));
                if !pipes.is_empty() && (file_ns(target) || templated_file) && !strip(target).starts_with(':') {
                    // MediaWiki takes the last part that is not an option as the caption; options may follow it
                    for k in (0..pipes.len()).rev() {
                        let (ls, le) = (pipes[k] + 1, pipes.get(k + 1).copied().unwrap_or(end));
                        let cap = sl(m, ls, le);
                        if file_option(cap) {
                            continue;
                        }
                        if has_alpha(cap) {
                            self.pair_trim(s, ls, le, close_end, depth, out);
                            return close_end;
                        }
                        break;
                    }
                } else if !pipes.is_empty() && has_alpha(sl(m, pipes[0] + 1, end)) {
                    self.pair_trim(s, pipes[0] + 1, end, close_end, depth, out);
                    return close_end;
                }
                self.point(sl(t, s, close_end), out);
                close_end
            }
            Kind::El(elt) => {
                match elt {
                    Some(es) if has_alpha(sl(m, *es, e - 1)) => self.pair_trim(s, *es, e - 1, e, depth, out),
                    _ => self.point(sl(t, s, e), out),
                }
                e
            }
            Kind::Quote(ia, ib) => {
                self.pair(s, *ia, *ib, e, depth, out);
                e
            }
            _ => {
                self.point(sl(t, s, e), out);
                e
            }
        }
    }

    fn dw_inline(&mut self, h: &Hit, depth: usize, out: &mut Vec<Inline>) {
        let (t, m) = (self.cx.text, self.cx.mask);
        let (s, e) = (h.s, h.e);
        match &h.k {
            Kind::DLink(ia, ib) => match sl(m, *ia, *ib).find('|') {
                Some(off) if has_alpha(sl(m, ia + off + 1, *ib)) => self.pair_trim(s, ia + off + 1, *ib, e, depth, out),
                _ => self.point(sl(t, s, e), out),
            },
            Kind::Media(ia, ib) => match sl(m, *ia, *ib).find('|') {
                Some(off) if !sl(m, *ia, ia + off).contains('>') && has_alpha(sl(m, ia + off + 1, *ib)) => {
                    self.pair_trim(s, ia + off + 1, *ib, e, depth, out)
                }
                _ => self.point(sl(t, s, e), out),
            },
            Kind::Foot(ia, ib) => self.pair_trim(s, *ia, *ib, e, depth, out),
            Kind::Bold(ia, ib) | Kind::Ital(ia, ib) | Kind::Under(ia, ib) => self.pair(s, *ia, *ib, e, depth, out),
            _ => self.point(sl(t, s, e), out),
        }
    }
}

/// Positions of `|` in `[a, b)` outside nested `[[ ]]`.
fn top_pipes(m: &str, a: usize, b: usize, links: &HashMap<usize, usize>) -> Vec<usize> {
    let by = m.as_bytes();
    let mut out = Vec::new();
    let mut i = a;
    while i < b {
        if by[i] == b'[' && by.get(i + 1) == Some(&b'[') {
            if let Some(&end) = links.get(&i).filter(|end| **end < b) {
                i = end + 2;
                continue;
            }
        }
        if by[i] == b'|' {
            out.push(i);
        }
        i += 1;
    }
    out
}

/// `[[` start to `]]` start over the whole mask, nested (file captions hold links).
pub fn link_match(mask: &str) -> HashMap<usize, usize> {
    let b = mask.as_bytes();
    let mut out = HashMap::new();
    let mut stack = Vec::new();
    // external links (`[http://x label]`) opened inside the current link and not closed yet
    let mut ext = 0usize;
    let mut i = 0;
    while i + 1 < b.len() {
        if b[i] == b'[' && b[i + 1] == b'[' {
            stack.push(i);
            ext = 0;
            i += 2;
        } else if b[i] == b'[' && !stack.is_empty() && ["http:", "https:", "ftp:", "ftps:", "mailto:", "//"].iter().any(|p| b[i + 1..].starts_with(p.as_bytes())) {
            ext += 1;
            i += 1;
        } else if b[i] == b']' && ext > 0 && b.get(i + 1) == Some(&b']') && b.get(i + 2) == Some(&b']') {
            // `]]]` after an open external link: the first `]` ends that link, the last two end the file link
            ext -= 1;
            i += 1;
        } else if b[i] == b']' && ext > 0 && b[i + 1] != b']' {
            ext -= 1;
            i += 1;
        } else if b[i] == b']' && b[i + 1] == b']' {
            if let Some(o) = stack.pop() {
                out.insert(o, i);
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    out
}

/// `XX[^\r\n]{1,500}?XX` for a two-character mark at `i`.
fn dd(m: &str, i: usize, limit: usize, mark: &[u8]) -> Option<(usize, usize)> {
    if !at(m.as_bytes(), i, limit, mark) {
        return None;
    }
    lazy_close(m, i + 2, limit, 1, 500, mark)
}

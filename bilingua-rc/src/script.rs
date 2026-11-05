//! The walk over a lexed resource script: which string literals are translatable, and where the LANGUAGE
//! statements and the VERSIONINFO language fields are. Iterative, no recursion: deep nesting costs nothing.

use crate::lex::{lex, Kind, Tok};
use crate::lit::{self, Piece};
use bilingua_model::{guard, Error, Inline, Limits};

pub struct Found {
    pub start: usize,
    pub end: usize,
    pub style: &'static str,
    /// Text and codes; each code is the raw source (`\t`, `%s`).
    pub pieces: Vec<Piece>,
    /// No letter or digit outside the codes: shown locked, or skipped.
    pub locked: bool,
}

pub struct Script<'a> {
    text: &'a str,
    toks: Vec<Tok>,
    nl: Vec<usize>,
    pub units: Vec<Found>,
    /// LANGUAGE argument spans (first argument start to second argument end).
    pub langs: Vec<(usize, usize)>,
    /// The 4 leading hex digits of a StringFileInfo block id.
    pub ver_blocks: Vec<(usize, usize)>,
    /// The language number of `VALUE "Translation"`.
    pub ver_trans: Vec<(usize, usize)>,
    collect: bool,
    include_locked: bool,
    limits: Limits,
}

#[derive(Clone, Copy)]
enum H {
    Strings,
    Menu,
    Dialog,
    Version,
    Skip,
}

const TEXT_CONTROLS: &[&str] = &[
    "LTEXT", "RTEXT", "CTEXT", "PUSHBUTTON", "DEFPUSHBUTTON", "PUSHBOX", "CHECKBOX", "AUTOCHECKBOX", "RADIOBUTTON", "AUTORADIOBUTTON",
    "AUTO3STATE", "STATE3", "GROUPBOX",
];
const OTHER_CONTROLS: &[&str] = &["CONTROL", "ICON", "EDITTEXT", "COMBOBOX", "LISTBOX", "SCROLLBAR"];
const IMAGE_STYLES: &[&str] = &["SS_ICON", "SS_BITMAP", "SS_ENHMETAFILE", "SS_OWNERDRAW"];
const VERSION_KEYS: &[&str] = &["FileDescription", "ProductName", "Comments", "LegalCopyright", "LegalTrademarks"];

fn in_list(list: &[&str], s: &str) -> bool {
    list.iter().any(|x| x.eq_ignore_ascii_case(s))
}

impl<'a> Script<'a> {
    /// Lexes and walks `text`. With `collect` false only the language spans are recorded.
    pub fn new(text: &'a str, collect: bool, include_locked: bool, limits: &Limits) -> Result<Script<'a>, Error> {
        let toks = lex(text);
        let nl = text.bytes().enumerate().filter(|(_, b)| *b == b'\n').map(|(i, _)| i).collect();
        let mut s = Script {
            text,
            toks,
            nl,
            units: Vec::new(),
            langs: Vec::new(),
            ver_blocks: Vec::new(),
            ver_trans: Vec::new(),
            collect,
            include_locked,
            limits: *limits,
        };
        s.walk()?;
        Ok(s)
    }

    /// The 1-based line of a byte offset.
    pub fn line(&self, pos: usize) -> usize {
        self.nl.partition_point(|&p| p < pos) + 1
    }

    fn val(&self, i: usize) -> &'a str {
        &self.text[self.toks[i].start..self.toks[i].end]
    }
    fn is_id(&self, i: usize) -> bool {
        self.toks[i].kind == Kind::Id
    }
    fn up_is(&self, i: usize, word: &str) -> bool {
        self.is_id(i) && self.val(i).eq_ignore_ascii_case(word)
    }
    fn is_p(&self, i: usize, c: &str) -> bool {
        self.toks[i].kind == Kind::P && self.val(i) == c
    }
    fn is_open(&self, i: usize) -> bool {
        self.up_is(i, "BEGIN") || self.is_p(i, "{")
    }
    fn is_close(&self, i: usize) -> bool {
        self.up_is(i, "END") || self.is_p(i, "}")
    }
    fn lineno(&self, pos: usize) -> usize {
        self.line(pos)
    }

    fn unit(&mut self, tok: usize, style: &'static str) -> Result<(), Error> {
        if !self.collect {
            return Ok(());
        }
        let t = self.toks[tok];
        let (pieces, _) = lit::pieces(&self.text[t.start..t.end]);
        let alnum = pieces.iter().any(|p| matches!(p, Piece::Text(s) if s.chars().any(char::is_alphanumeric)));
        if !alnum && (!self.include_locked || pieces.is_empty()) {
            return Ok(());
        }
        self.units.push(Found { start: t.start, end: t.end, style, pieces, locked: !alnum });
        guard::check_unit_count(self.units.len(), &self.limits)
    }

    /// `LANGUAGE a, b` at `i`; the index after it.
    fn language(&mut self, i: usize) -> usize {
        let n = self.toks.len();
        if i + 3 < n
            && matches!(self.toks[i + 1].kind, Kind::Id | Kind::Num)
            && self.is_p(i + 2, ",")
            && matches!(self.toks[i + 3].kind, Kind::Id | Kind::Num)
        {
            self.langs.push((self.toks[i + 1].start, self.toks[i + 3].end));
            return i + 4;
        }
        i + 1
    }

    /// Optional statements up to the body's BEGIN; the index of BEGIN (or the end).
    fn header(&mut self, mut i: usize, caption: bool) -> Result<usize, Error> {
        let n = self.toks.len();
        while i < n && !self.is_open(i) {
            if self.up_is(i, "LANGUAGE") {
                i = self.language(i);
                continue;
            }
            if caption && self.up_is(i, "CAPTION") && i + 1 < n && self.toks[i + 1].kind == Kind::Str {
                self.unit(i + 1, "rc:caption")?;
                i += 2;
                continue;
            }
            if self.is_close(i) {
                return Ok(i);
            }
            i += 1;
        }
        Ok(i)
    }

    /// A BEGIN..END body starting at `i` (the BEGIN); the index after its END.
    fn body(&mut self, mut i: usize, h: H) -> Result<usize, Error> {
        let n = self.toks.len();
        if i >= n || !self.is_open(i) {
            return Ok(i);
        }
        let mut depth = 1usize;
        i += 1;
        while i < n && depth > 0 {
            if self.is_open(i) {
                depth += 1;
                i += 1;
            } else if self.is_close(i) {
                depth -= 1;
                i += 1;
            } else {
                i = self.handle(h, i)?;
            }
        }
        Ok(i)
    }

    fn handle(&mut self, h: H, i: usize) -> Result<usize, Error> {
        let n = self.toks.len();
        match h {
            H::Skip => Ok(i + 1),
            H::Strings => {
                if self.toks[i].kind == Kind::Str {
                    self.unit(i, "rc:string")?;
                }
                Ok(i + 1)
            }
            H::Menu => {
                if (self.up_is(i, "MENUITEM") || self.up_is(i, "POPUP")) && i + 1 < n && self.toks[i + 1].kind == Kind::Str {
                    self.unit(i + 1, "rc:menu")?;
                    return Ok(i + 2);
                }
                Ok(i + 1)
            }
            H::Dialog => {
                if !self.is_id(i) {
                    return Ok(i + 1);
                }
                let v = self.val(i);
                let text_ctl = in_list(TEXT_CONTROLS, v);
                if !text_ctl && !in_list(OTHER_CONTROLS, v) {
                    return Ok(i + 1);
                }
                if text_ctl {
                    if i + 1 < n && self.toks[i + 1].kind == Kind::Str {
                        self.unit(i + 1, "rc:control")?;
                        return Ok(i + 2);
                    }
                    return Ok(i + 1);
                }
                if v.eq_ignore_ascii_case("CONTROL") && i + 1 < n && self.toks[i + 1].kind == Kind::Str {
                    let mut j = i + 2;
                    let mut image = false;
                    while j < n {
                        if self.is_open(j) || self.is_close(j) {
                            break;
                        }
                        if self.is_id(j) {
                            let u = self.val(j);
                            if in_list(TEXT_CONTROLS, u) || in_list(OTHER_CONTROLS, u) {
                                break;
                            }
                            if in_list(IMAGE_STYLES, u) {
                                image = true;
                            }
                        }
                        j += 1;
                    }
                    if !image {
                        self.unit(i + 1, "rc:control")?;
                    }
                    return Ok(i + 2);
                }
                Ok(i + 1)
            }
            H::Version => {
                if self.up_is(i, "BLOCK") && i + 1 < n && self.toks[i + 1].kind == Kind::Str {
                    let s = self.toks[i + 1];
                    let v = &self.text[s.start..s.end];
                    let inner = v.strip_prefix('L').unwrap_or(v);
                    if inner.len() == 10 && inner.starts_with('"') && inner.ends_with('"') && inner[1..9].bytes().all(|b| b.is_ascii_hexdigit()) {
                        let q = v.find('"').unwrap_or(0) + 1;
                        self.ver_blocks.push((s.start + q, s.start + q + 4));
                    }
                    return Ok(i + 2);
                }
                if self.up_is(i, "VALUE") && i + 1 < n && self.toks[i + 1].kind == Kind::Str {
                    let raw = self.val(i + 1);
                    let key = raw.trim_start_matches('L').trim_matches('"');
                    let key = key.strip_suffix("\\0").unwrap_or(key).trim();
                    let mut j = i + 2;
                    if j < n && self.is_p(j, ",") {
                        j += 1;
                    }
                    if key == "Translation" && j < n && self.toks[j].kind == Kind::Num {
                        self.ver_trans.push((self.toks[j].start, self.toks[j].end));
                        return Ok(j + 1);
                    }
                    if in_list_exact(VERSION_KEYS, key) && j < n && self.toks[j].kind == Kind::Str {
                        self.unit(j, "rc:version")?;
                        return Ok(j + 1);
                    }
                    return Ok(j);
                }
                Ok(i + 1)
            }
        }
    }

    /// An unknown or skipped resource: the rest of its line (and lines continued by a trailing comma), then a
    /// BEGIN..END body if one follows.
    fn skip_unknown(&mut self, type_idx: usize) -> Result<usize, Error> {
        let n = self.toks.len();
        let mut line = self.lineno(self.toks[type_idx].start);
        let mut i = type_idx + 1;
        while i < n && !self.is_open(i) {
            let l = self.lineno(self.toks[i].start);
            if l != line {
                if self.is_p(i - 1, ",") {
                    line = l;
                } else {
                    break;
                }
            }
            i += 1;
        }
        if i < n && self.is_open(i) {
            i = self.body(i, H::Skip)?;
        }
        Ok(i)
    }

    fn walk(&mut self) -> Result<(), Error> {
        let n = self.toks.len();
        let mut i = 0;
        while i < n {
            let kind = self.toks[i].kind;
            if self.up_is(i, "LANGUAGE") {
                i = self.language(i);
            } else if self.up_is(i, "STRINGTABLE") {
                let h = self.header(i + 1, false)?;
                i = self.body(h, H::Strings)?;
            } else if self.up_is(i, "VERSION") || self.up_is(i, "CHARACTERISTICS") || self.up_is(i, "RCINCLUDE") {
                i = self.skip_unknown(i)?;
            } else if matches!(kind, Kind::Id | Kind::Num | Kind::Str) && i + 1 < n && self.is_id(i + 1) {
                let typ = self.val(i + 1).to_ascii_uppercase();
                match typ.as_str() {
                    "DIALOG" | "DIALOGEX" => {
                        let h = self.header(i + 2, true)?;
                        i = self.body(h, H::Dialog)?;
                    }
                    "MENU" | "MENUEX" => {
                        let h = self.header(i + 2, false)?;
                        i = self.body(h, H::Menu)?;
                    }
                    "VERSIONINFO" => {
                        let h = self.header(i + 2, false)?;
                        i = self.body(h, H::Version)?;
                    }
                    _ => i = self.skip_unknown(i + 1)?,
                }
            } else if self.is_open(i) {
                i = self.body(i, H::Skip)?;
            } else {
                i += 1;
            }
        }
        Ok(())
    }
}

fn in_list_exact(list: &[&str], s: &str) -> bool {
    list.contains(&s)
}

/// The text and codes of a found literal as inline content. Codes get ids "1", "2", ..., in order.
pub fn inlines(pieces: &[Piece]) -> (Vec<Inline>, Vec<(String, String)>) {
    let mut out = Vec::with_capacity(pieces.len());
    let mut codes = Vec::new();
    for p in pieces {
        match p {
            Piece::Text(t) => out.push(Inline::text(t.clone())),
            Piece::Code(c) => {
                let id = (codes.len() + 1).to_string();
                out.push(if lit::is_free(c) { Inline::free_tag(id.clone(), c.clone()) } else { Inline::tag(id.clone(), bilingua_model::TagKind::Standalone, c.clone()) });
                codes.push((id, c.clone()));
            }
        }
    }
    (out, codes)
}

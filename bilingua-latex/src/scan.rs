//! The LaTeX source scanner: low-level helpers (control sequences, balanced groups, arguments) and the block scan
//! that finds the paragraph-level text slots of a document. Nothing is executed or expanded, and every loop is
//! iterative: a hostile file costs time linear in its size.

use crate::chars::{char_at, py_alpha, py_space, py_strip};
use bilingua_model::guard::{self, Limits};
use bilingua_model::Error;
use regex::Regex;
use std::cell::OnceCell;
use std::sync::OnceLock;

// ---------------------------------------------------------------------------------------------- name tables

/// Block commands whose arguments are units of their own.
pub const TEXT_BLOCK_COMMANDS: &[&str] = &[
    "part", "chapter", "section", "subsection", "subsubsection", "paragraph", "subparagraph", "addpart", "addchap", "addsec", "minisec",
    "caption", "captionof", "title", "subtitle", "frametitle", "framesubtitle",
];

/// Inline commands whose last mandatory argument is text: name and the number of leading mandatory arguments that
/// stay in the markup.
pub const TEXT_INLINE_COMMANDS: &[(&str, usize)] = &[
    ("emph", 0),
    ("textbf", 0),
    ("textit", 0),
    ("textsl", 0),
    ("textsc", 0),
    ("textup", 0),
    ("textmd", 0),
    ("textrm", 0),
    ("textsf", 0),
    ("textnormal", 0),
    ("underline", 0),
    ("uline", 0),
    ("footnote", 0),
    ("footnotetext", 0),
    ("marginpar", 0),
    ("mbox", 0),
    ("fbox", 0),
    ("enquote", 0),
    ("textquote", 0),
    ("intertext", 0),
    ("text", 0),
    ("href", 1),
    ("textcolor", 1),
    ("colorbox", 1),
    ("foreignlanguage", 1),
    ("hyperref", 0),
];

const BOUNDARY_COMMANDS: &[&str] = &[
    "par", "item", "bibitem", "hline", "cline", "toprule", "midrule", "bottomrule", "cmidrule", "centering", "raggedright", "raggedleft",
    "maketitle", "tableofcontents", "listoffigures", "listoftables", "newpage", "clearpage", "cleardoublepage", "pagebreak", "vspace",
    "vfill", "bigskip", "medskip", "smallskip", "printbibliography", "bibliography", "bibliographystyle", "input", "include", "includeonly",
    "subfile", "appendix", "frontmatter", "mainmatter", "backmatter", "usepackage", "documentclass", "newcommand", "renewcommand",
    "providecommand", "newenvironment", "renewenvironment", "setlength", "addtolength", "setcounter", "addtocounter", "includegraphics",
    "label", "printindex", "makeindex",
];

/// Arguments read verbatim by the command: `%` and `#` there are not comments.
pub const VERBATIM_ARGS: &[&str] = &["url", "href", "path", "nolinkurl", "hyperlink", "hypertarget"];
const DEF_COMMANDS: &[&str] = &["def", "gdef", "edef", "xdef", "let"];

const VERBATIM_ENVS: &[&str] = &[
    "verbatim", "verbatim*", "Verbatim", "Verbatim*", "BVerbatim", "LVerbatim", "lstlisting", "minted", "comment", "filecontents",
    "filecontents*", "alltt", "spverbatim", "tcblisting", "luacode", "luacode*", "pycode",
];
const SKIP_ENVS: &[&str] = &[
    "equation", "equation*", "align", "align*", "alignat", "alignat*", "flalign", "flalign*", "gather", "gather*", "multline", "multline*",
    "eqnarray", "eqnarray*", "math", "displaymath", "array", "split", "dmath", "dmath*", "tikzpicture", "pgfpicture", "picture", "forest",
    "axis", "circuitikz",
];
const TABULAR_ENVS: &[(&str, usize)] = &[
    ("tabular", 1),
    ("tabular*", 2),
    ("tabularx", 2),
    ("tabulary", 2),
    ("longtable", 1),
    ("supertabular", 1),
    ("xtabular", 1),
    ("tblr", 1),
];
const ENV_ARGS: &[(&str, usize)] = &[
    ("minipage", 1),
    ("multicols", 1),
    ("thebibliography", 1),
    ("wrapfigure", 2),
    ("list", 2),
    ("otherlanguage", 1),
    ("otherlanguage*", 1),
    ("subfigure", 1),
    ("adjustbox", 1),
    ("tcolorbox", 0),
];

pub fn is_verbatim_arg(name: &str) -> bool {
    VERBATIM_ARGS.contains(&name)
}

fn is_tabular(env: &str) -> bool {
    TABULAR_ENVS.iter().any(|(n, _)| *n == env)
}

// ---------------------------------------------------------------------------------------------- group maps

/// Positions found by one linear pass over a text (an unclosed `{`, `[` or `$` repeated many times must not cost a
/// rescan each). Sparse: the size follows the number of delimiters, not the size of the text.
///   * `brace`: for the n-th `{` the index after its matching `}`
///   * `bracket`: for the n-th `[` the index after the first `]` at the same brace level
///   * `dollars`: positions of unescaped `$`
///   * `blanks`: positions of newlines that start a blank line
///
/// Escapes skip the next character; `%` starts a comment unless `verbatim` (`\url`); a blank line ends every open
/// group (LaTeX arguments never cross a paragraph).
pub struct Maps {
    open_pos: Vec<u32>,
    open_end: Vec<u32>,
    sq_pos: Vec<u32>,
    sq_end: Vec<u32>,
    dollars: Vec<u32>,
    blanks: Vec<u32>,
}

/// `text[k]` is a newline and the next line is blank (or the text ends).
pub fn blank_line_at(b: &[u8], k: usize, limit: usize) -> bool {
    let mut j = k + 1;
    while j < limit && matches!(b[j], b' ' | b'\t' | b'\r') {
        j += 1;
    }
    j >= limit || b[j] == b'\n'
}

fn find_byte(b: &[u8], x: u8, from: usize) -> Option<usize> {
    if from >= b.len() {
        return None;
    }
    b[from..].iter().position(|&c| c == x).map(|p| p + from)
}

fn compute_maps(t: &str, verbatim: bool) -> Maps {
    let b = t.as_bytes();
    let n = b.len();
    let mut m = Maps { open_pos: vec![], open_end: vec![], sq_pos: vec![], sq_end: vec![], dollars: vec![], blanks: vec![] };
    let mut stack: Vec<u32> = Vec::new(); // ordinal of each open `{`
    let mut pend: Vec<u32> = Vec::new(); // pending `[` ordinals, flat
    let mut lvl: Vec<u32> = vec![0]; // start of each brace level in `pend`
    let mut k = 0usize;
    while k < n {
        match b[k] {
            b'\\' => {
                k += 2;
                continue;
            }
            b'%' if !verbatim => {
                k = find_byte(b, b'\n', k).map_or(n, |p| p + 1);
                continue;
            }
            b'\n' if blank_line_at(b, k, n) => {
                m.blanks.push(k as u32);
                stack.clear();
                pend.clear();
                lvl.clear();
                lvl.push(0);
            }
            b'{' => {
                stack.push(m.open_pos.len() as u32);
                m.open_pos.push(k as u32);
                m.open_end.push(0);
                lvl.push(pend.len() as u32);
            }
            b'}' => {
                if let Some(o) = stack.pop() {
                    m.open_end[o as usize] = (k + 1) as u32;
                    let s = lvl.pop().unwrap_or(0);
                    pend.truncate(s as usize);
                } else {
                    pend.truncate(lvl.last().copied().unwrap_or(0) as usize);
                }
            }
            b'[' => {
                pend.push(m.sq_pos.len() as u32);
                m.sq_pos.push(k as u32);
                m.sq_end.push(0);
            }
            b']' => {
                let s = lvl.last().copied().unwrap_or(0) as usize;
                for &p in &pend[s..] {
                    m.sq_end[p as usize] = (k + 1) as u32;
                }
                pend.truncate(s);
            }
            b'$' => m.dollars.push(k as u32),
            _ => {}
        }
        k += 1;
    }
    m
}

/// A text with its (lazily computed) group maps.
pub struct Txt<'a> {
    pub s: &'a str,
    maps: [OnceCell<Maps>; 2],
}

impl<'a> Txt<'a> {
    pub fn new(s: &'a str) -> Txt<'a> {
        Txt { s, maps: [OnceCell::new(), OnceCell::new()] }
    }

    fn maps(&self, verbatim: bool) -> &Maps {
        self.maps[verbatim as usize].get_or_init(|| compute_maps(self.s, verbatim))
    }

    pub fn bytes(&self) -> &'a [u8] {
        self.s.as_bytes()
    }

    /// `text[i] == '{'`: the index after the matching `}` (never across a blank line), if it is within `limit`.
    pub fn match_brace(&self, i: usize, limit: usize, verbatim: bool) -> Option<usize> {
        let m = self.maps(verbatim);
        let o = m.open_pos.binary_search(&(i as u32)).ok()?;
        let e = m.open_end[o] as usize;
        (e != 0 && e <= limit).then_some(e)
    }

    /// `text[i] == '['`: the index after the first `]` at the same brace level, if it is within `limit`.
    pub fn match_bracket(&self, i: usize, limit: usize) -> Option<usize> {
        let m = self.maps(false);
        let o = m.sq_pos.binary_search(&(i as u32)).ok()?;
        let e = m.sq_end[o] as usize;
        (e != 0 && e <= limit).then_some(e)
    }

    /// `text[i] == '$'` (single): the index after the next unescaped `$`, never across a blank line.
    pub fn inline_math_end(&self, i: usize, limit: usize) -> Option<usize> {
        let m = self.maps(false);
        let d = m.dollars.partition_point(|&x| x as usize <= i);
        if d >= m.dollars.len() || m.dollars[d] as usize >= limit {
            return None;
        }
        let j = m.dollars[d] as usize;
        let bl = m.blanks.partition_point(|&x| x as usize <= i);
        if bl < m.blanks.len() && (m.blanks[bl] as usize) < j {
            return None;
        }
        Some(j + 1)
    }
}

// ---------------------------------------------------------------------------------------------- helpers

/// A control sequence at `t[i] == '\\'`: its name and the end index. A control word takes every letter (Python
/// `isalpha`, so Unicode letters too); anything else is a one-character control symbol.
pub fn cs(t: &str, i: usize) -> (&str, usize) {
    let j = i + 1;
    if j >= t.len() {
        return ("", j);
    }
    let (c, l) = char_at(t, j);
    if py_alpha(c) {
        let mut k = j + l;
        while k < t.len() {
            let (c2, l2) = char_at(t, k);
            if !py_alpha(c2) {
                break;
            }
            k += l2;
        }
        (&t[j..k], k)
    } else {
        (&t[j..j + l], j + l)
    }
}

/// `text.find(needle, i, limit)` returning the index after the needle, or `limit` when there is none.
pub fn find_end(t: &str, needle: &str, i: usize, limit: usize) -> usize {
    if i > limit {
        return limit;
    }
    t[i..limit].find(needle).map_or(limit, |k| i + k + needle.len())
}

/// Skip spaces and at most one newline (arguments may follow on the next line).
fn skip_ws(b: &[u8], mut i: usize, limit: usize) -> usize {
    let mut nl = false;
    while i < limit && matches!(b[i], b' ' | b'\t' | b'\r' | b'\n') {
        if b[i] == b'\n' {
            if nl {
                break;
            }
            nl = true;
        }
        i += 1;
    }
    i
}

/// One `[opt]` or `{arg}` group: the opening character, start and end (delimiters included).
pub type Arg = (u8, usize, usize);

/// Adjacent `[opt]` and `{arg}` groups after position `i`. `spaced` allows whitespace before each group.
pub fn args(tx: &Txt, mut i: usize, limit: usize, spaced: bool, verbatim: bool) -> Vec<Arg> {
    let b = tx.bytes();
    let mut out = Vec::new();
    while i < limit {
        let j = if spaced { skip_ws(b, i, limit) } else { i };
        let e = if j < limit && b[j] == b'{' {
            tx.match_brace(j, limit, verbatim)
        } else if j < limit && b[j] == b'[' {
            tx.match_bracket(j, limit)
        } else {
            break;
        };
        let Some(e) = e else { break };
        out.push((b[j], j, e));
        i = e;
    }
    out
}

pub fn end_of_args(a: &[Arg], i: usize) -> usize {
    a.last().map_or(i, |x| x.2)
}

/// `i` is just after `\verb` or `\verb*`: the delimiter is the character at `i`. The end of the verbatim text.
pub fn verb_end(tx: &Txt, i: usize, limit: usize) -> usize {
    let t = tx.s;
    if i >= limit {
        return limit;
    }
    if t.as_bytes()[i] == b'{' {
        return tx.match_brace(i, limit, false).unwrap_or(i + 1);
    }
    let (d, dl) = char_at(t, i);
    let start = i + dl;
    if start > limit {
        return i + dl;
    }
    let nl = t[start..limit].find('\n').map(|p| p + start);
    let stop = nl.unwrap_or(limit);
    let mut buf = [0u8; 4];
    match t[start..stop].find(d.encode_utf8(&mut buf) as &str) {
        Some(k) => start + k + dl,
        None => i + dl,
    }
}

/// `\end{env}` (with optional blanks after `\end`) searched in `[from, limit)`: the index after it.
fn find_env_end(t: &str, env: &str, from: usize, limit: usize) -> Option<usize> {
    let mut pos = from;
    while pos < limit {
        let k = t[pos..limit].find("\\end")? + pos;
        let mut j = k + 4;
        while j < limit {
            let (c, l) = char_at(t, j);
            if !crate::chars::py_space(c) {
                break;
            }
            j += l;
        }
        let rest = &t[j..limit];
        if let Some(r) = rest.strip_prefix('{') {
            if let Some(r) = r.strip_prefix(env) {
                if r.starts_with('}') {
                    return Some(j + 1 + env.len() + 1);
                }
            }
        }
        pos = k + 4;
    }
    None
}

fn in_comment(t: &str, pos: usize) -> bool {
    let b = t.as_bytes();
    let ls = b[..pos].iter().rposition(|&c| c == b'\n').map_or(0, |p| p + 1);
    let mut k = ls;
    while k < pos {
        match b[k] {
            b'\\' => {
                k += 2;
                continue;
            }
            b'%' => return true,
            _ => {}
        }
        k += 1;
    }
    false
}

fn doc_re(begin: bool) -> &'static Regex {
    static B: OnceLock<Regex> = OnceLock::new();
    static E: OnceLock<Regex> = OnceLock::new();
    if begin {
        B.get_or_init(|| Regex::new(r"\\begin\s*\{document\}").expect("begin pattern"))
    } else {
        E.get_or_init(|| Regex::new(r"\\end\s*\{document\}").expect("end pattern"))
    }
}

// ---------------------------------------------------------------------------------------------- block scan

/// One paragraph-level slot of text in the source.
#[derive(Debug, Clone, Copy)]
pub struct Slot {
    pub a: usize,
    pub b: usize,
    pub kind: &'static str,
}

pub struct Scan<'a> {
    tx: &'a Txt<'a>,
    pub slots: Vec<Slot>,
    limits: Limits,
}

fn block_kind(name: &str) -> &'static str {
    match name {
        "caption" | "captionof" => "caption",
        "title" | "subtitle" | "frametitle" | "framesubtitle" => "title",
        _ => "heading",
    }
}

impl<'a> Scan<'a> {
    pub fn new(tx: &'a Txt<'a>, limits: Limits) -> Scan<'a> {
        Scan { tx, slots: Vec::new(), limits }
    }

    fn unit(&mut self, a: usize, b: usize, kind: &'static str) -> Result<(), Error> {
        let t = self.tx.s;
        let s = &t[a..b];
        let lead = s.len() - s.trim_start_matches(py_space).len();
        let body = py_strip(s);
        if !body.is_empty() {
            self.slots.push(Slot { a: a + lead, b: a + lead + body.len(), kind });
            guard::check_unit_count(self.slots.len(), &self.limits)?;
        }
        Ok(())
    }

    pub fn run(&mut self) -> Result<(), Error> {
        let t = self.tx.s;
        let mut doc = doc_re(true).find(t);
        while let Some(d) = doc {
            if !in_comment(t, d.start()) {
                break;
            }
            doc = doc_re(true).find_at(t, d.end());
        }
        match doc {
            Some(d) => {
                self.preamble(0, d.start())?;
                let mut end = doc_re(false).find_at(t, d.end());
                while let Some(e) = end {
                    if !in_comment(t, e.start()) {
                        break;
                    }
                    end = doc_re(false).find_at(t, e.end());
                }
                self.body(d.end(), end.map_or(t.len(), |e| e.start()))
            }
            None => self.body(0, t.len()),
        }
    }

    fn preamble(&mut self, a: usize, b: usize) -> Result<(), Error> {
        let t = self.tx.s;
        let by = t.as_bytes();
        let mut i = a;
        while i < b {
            match by[i] {
                b'%' => {
                    i = find_end(t, "\n", i, b);
                    continue;
                }
                b'\\' => {
                    let (name, mut e) = cs(t, i);
                    if name == "title" || name == "subtitle" {
                        for (_k, s, en) in args(self.tx, e, b, true, false) {
                            self.unit(s + 1, en - 1, "title")?;
                            e = en;
                        }
                    }
                    i = e;
                    continue;
                }
                _ => {}
            }
            i += 1;
        }
        Ok(())
    }

    fn body(&mut self, a: usize, b: usize) -> Result<(), Error> {
        let tx = self.tx;
        let t = tx.s;
        let by = t.as_bytes();
        let mut stack: Vec<&'a str> = Vec::new();
        let mut run: Option<usize> = None;
        let mut i = a;

        macro_rules! flush {
            ($at:expr) => {
                if let Some(r) = run.take() {
                    let tab = stack.last().is_some_and(|s| is_tabular(s));
                    self.unit(r, $at, if tab { "cell" } else { "paragraph" })?;
                }
            };
        }
        macro_rules! start {
            ($at:expr) => {
                if run.is_none() {
                    run = Some($at);
                }
            };
        }
        let in_tab = |stack: &Vec<&str>| stack.last().is_some_and(|s| is_tabular(s));

        while i < b {
            let c = by[i];
            if c == b'%' {
                flush!(i);
                i = find_end(t, "\n", i, b);
                continue;
            }
            if c == b'\n' && blank_line_at(by, i, b) {
                flush!(i);
                i += 1;
                continue;
            }
            if c == b'$' {
                if by.get(i + 1) == Some(&b'$') {
                    flush!(i);
                    i = if i + 2 <= b { t[i + 2..b].find("$$").map_or(b, |k| i + 2 + k + 2) } else { b };
                    continue;
                }
                let e = tx.inline_math_end(i, b);
                start!(i);
                i = e.unwrap_or(i + 1);
                continue;
            }
            if c == b'&' && in_tab(&stack) {
                flush!(i);
                i += 1;
                continue;
            }
            if c != b'\\' {
                let (ch, l) = char_at(t, i);
                if !py_space(ch) {
                    start!(i);
                }
                i += l;
                continue;
            }
            let (name, mut e) = cs(t, i);
            match name {
                "[" => {
                    flush!(i);
                    i = if e <= b { t[e..b].find("\\]").map_or(b, |k| e + k + 2) } else { b };
                    continue;
                }
                "(" => {
                    start!(i);
                    i = if e <= b { t[e..b].find("\\)").map_or(b, |k| e + k + 2) } else { b };
                    continue;
                }
                "\\" => {
                    if in_tab(&stack) {
                        flush!(i);
                        if e < b && by[e] == b'*' {
                            e += 1;
                        }
                        let a2 = args(tx, e, b, false, false);
                        i = end_of_args(&a2, e);
                    } else {
                        start!(i);
                        i = e;
                    }
                    continue;
                }
                "verb" | "lstinline" | "mintinline" => {
                    start!(i);
                    if e < b && by[e] == b'*' {
                        e += 1;
                    }
                    if name == "mintinline" {
                        let a2 = args(tx, e, b, false, false);
                        e = end_of_args(&a2[..a2.len().min(1)], e);
                        i = if a2.len() > 1 { end_of_args(&a2[1..2], e) } else { verb_end(tx, e, b) };
                        continue;
                    }
                    if name == "lstinline" && e < b && by[e] == b'[' {
                        if let Some(k) = tx.match_bracket(e, b) {
                            e = k;
                        }
                    }
                    i = verb_end(tx, e, b);
                    continue;
                }
                "begin" => {
                    flush!(i);
                    let envs = args(tx, e, b, true, false);
                    if envs.is_empty() || envs[0].0 != b'{' {
                        i = e;
                        continue;
                    }
                    let env = py_strip(&t[envs[0].1 + 1..envs[0].2 - 1]);
                    e = envs[0].2;
                    if VERBATIM_ENVS.contains(&env) || SKIP_ENVS.contains(&env) {
                        i = find_env_end(t, env, e, b).unwrap_or(b);
                        continue;
                    }
                    let need = TABULAR_ENVS.iter().chain(ENV_ARGS.iter()).find(|(n, _)| *n == env).map_or(0, |x| x.1);
                    let mut k = e;
                    let mut taken = 0;
                    for (kind, _s, en) in args(tx, e, b, false, false) {
                        if kind == b'{' {
                            if taken >= need {
                                break;
                            }
                            taken += 1;
                        }
                        k = en;
                    }
                    stack.push(env);
                    i = k;
                    continue;
                }
                "end" => {
                    flush!(i);
                    let envs = args(tx, e, b, true, false);
                    if !envs.is_empty() && envs[0].0 == b'{' {
                        let env = py_strip(&t[envs[0].1 + 1..envs[0].2 - 1]);
                        if stack.contains(&env) {
                            while let Some(top) = stack.pop() {
                                if top == env {
                                    break;
                                }
                            }
                        }
                        e = envs[0].2;
                    }
                    i = e;
                    continue;
                }
                _ => {}
            }
            if DEF_COMMANDS.contains(&name) {
                flush!(i);
                let mut k = e;
                while k < b && by[k] != b'{' && by[k] != b'\n' {
                    k += 1;
                }
                if k < b && by[k] == b'{' {
                    k = tx.match_brace(k, b, false).unwrap_or(k + 1);
                }
                i = k;
                continue;
            }
            if TEXT_BLOCK_COMMANDS.contains(&name) {
                flush!(i);
                if e < b && by[e] == b'*' {
                    e += 1;
                }
                let mut a2 = args(tx, e, b, true, false);
                if name == "captionof" && !a2.is_empty() && a2[0].0 == b'{' {
                    a2.remove(0); // the float type
                }
                for &(_k, s, en) in &a2 {
                    self.unit(s + 1, en - 1, block_kind(name))?;
                }
                i = end_of_args(&a2, e);
                continue;
            }
            if name == "item" {
                flush!(i);
                let a2 = args(tx, e, b, false, false);
                if !a2.is_empty() && a2[0].0 == b'[' {
                    self.unit(a2[0].1 + 1, a2[0].2 - 1, "label")?;
                    e = a2[0].2;
                }
                i = e;
                continue;
            }
            if BOUNDARY_COMMANDS.contains(&name) {
                flush!(i);
                if e < b && by[e] == b'*' {
                    e += 1;
                }
                let a2 = args(tx, e, b, !matches!(name, "item" | "par" | "centering"), false);
                i = end_of_args(&a2, e);
                continue;
            }
            // any other command: part of the current run, arguments included
            start!(i);
            if e < b && by[e] == b'*' {
                e += 1;
            }
            let a2 = args(tx, e, b, false, is_verbatim_arg(name));
            i = end_of_args(&a2, e);
        }
        flush!(b);
        Ok(())
    }
}

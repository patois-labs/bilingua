//! The MIF statement scanner: an iterative, allocation-light lexer and nester. No recursion anywhere, so a file
//! of a million open brackets costs memory proportional to its size and nothing else.
//!
//! Grammar (as the LocOS filter reads it): `<Name args... <Sub ...> >`; a string is `` `text' `` with backslash
//! escapes; `#` starts a comment to the end of the line; a token that starts with `=` or `&` at the start of a line
//! is an inset facet line (opaque data, skipped to the end of the line); anything else without white space, angle
//! brackets, back quotes, `#` or `'` is an atom.

/// No node (a missing child, sibling or parent).
pub const NONE: u32 = u32::MAX;

/// A leaf argument of a statement.
#[derive(Debug, Clone, Copy)]
pub struct Arg {
    /// True for a `` `string' ``, false for an atom.
    pub is_str: bool,
    pub start: usize,
    pub end: usize,
}

/// One statement: `<Name ...>`. Child statements are linked (first child, next sibling).
#[derive(Debug)]
pub struct Node {
    pub name_start: usize,
    pub name_end: usize,
    /// Byte offset of the `<`.
    pub start: usize,
    /// Byte offset just after the closing `>`.
    pub end: usize,
    pub args: Vec<Arg>,
    pub first_child: u32,
    pub last_child: u32,
    pub next: u32,
}

/// All statements of a file. Node 0 is a nameless root that holds the top-level statements.
#[derive(Debug)]
pub struct Tree {
    pub nodes: Vec<Node>,
}

/// Why a file could not be nested.
#[derive(Debug)]
pub enum ScanError {
    Syntax(String),
    TooDeep,
}

fn syntax(msg: String) -> ScanError {
    ScanError::Syntax(format!(
        "This file could not be read as FrameMaker MIF: {msg}"
    ))
}

/// White space as Python's `\s` sees it on text (Unicode white space plus the four ASCII separators).
pub fn is_ws(c: char) -> bool {
    bilingua_model::py_isspace_char(c)
}

fn is_ascii_ws(b: u8) -> bool {
    matches!(b, b'\t' | b'\n' | 0x0b | 0x0c | b'\r' | b' ' | 0x1c..=0x1f)
}

impl Tree {
    pub fn name<'a>(&self, text: &'a str, n: u32) -> &'a str {
        let nd = &self.nodes[n as usize];
        &text[nd.name_start..nd.name_end]
    }

    pub fn raw<'a>(&self, text: &'a str, n: u32) -> &'a str {
        let nd = &self.nodes[n as usize];
        &text[nd.start..nd.end]
    }

    pub fn children(&self, n: u32) -> Children<'_> {
        Children {
            tree: self,
            cur: self.nodes[n as usize].first_child,
        }
    }

    pub fn has_children(&self, n: u32) -> bool {
        self.nodes[n as usize].first_child != NONE
    }

    /// The decoded text of a statement whose only argument is one string (`<PgfTag `Body'>`).
    pub fn string_arg(&self, text: &str, n: u32) -> Option<String> {
        let nd = &self.nodes[n as usize];
        if nd.args.len() != 1 || nd.first_child != NONE || !nd.args[0].is_str {
            return None;
        }
        let a = nd.args[0];
        Some(crate::esc::decode_string(&text[a.start + 1..a.end - 1]))
    }

    /// The word of a statement whose only argument is one atom (`<Char Tab>`).
    pub fn atom_arg<'a>(&self, text: &'a str, n: u32) -> Option<&'a str> {
        let nd = &self.nodes[n as usize];
        if nd.args.len() != 1 || nd.first_child != NONE || nd.args[0].is_str {
            return None;
        }
        let a = nd.args[0];
        Some(&text[a.start..a.end])
    }
}

pub struct Children<'a> {
    tree: &'a Tree,
    cur: u32,
}

impl Iterator for Children<'_> {
    type Item = u32;
    fn next(&mut self) -> Option<u32> {
        if self.cur == NONE {
            return None;
        }
        let n = self.cur;
        self.cur = self.tree.nodes[n as usize].next;
        Some(n)
    }
}

/// Lexes and nests the whole text. `max_depth` is the deepest statement nesting allowed.
pub fn scan(text: &str, max_depth: usize) -> Result<Tree, ScanError> {
    let b = text.as_bytes();
    let n = b.len();
    let mut nodes: Vec<Node> = Vec::with_capacity(n / 24 + 4);
    nodes.push(Node {
        name_start: 0,
        name_end: 0,
        start: 0,
        end: 0,
        args: Vec::new(),
        first_child: NONE,
        last_child: NONE,
        next: NONE,
    });
    let mut stack: Vec<u32> = vec![0];
    let mut pos = 0usize;
    // true when the current line already holds something other than white space before `pos`
    let mut content_on_line = false;
    while pos < n {
        let c = b[pos];
        match c {
            b'`' => {
                let mut i = pos + 1;
                loop {
                    if i >= n {
                        return Err(syntax(format!("unexpected text at position {pos}.")));
                    }
                    match b[i] {
                        b'\\' => {
                            if i + 1 >= n {
                                return Err(syntax(format!("unexpected text at position {pos}.")));
                            }
                            i += 2;
                        }
                        b'\'' => break,
                        _ => i += 1,
                    }
                }
                let top = *stack.last().expect("root") as usize;
                nodes[top].args.push(Arg {
                    is_str: true,
                    start: pos,
                    end: i + 1,
                });
                pos = i + 1;
                content_on_line = true;
            }
            b'<' => {
                let mut i = pos + 1;
                while i < n && (b[i] == b' ' || b[i] == b'\t') {
                    i += 1;
                }
                let ns = i;
                if i >= n || !b[i].is_ascii_alphabetic() {
                    return Err(syntax(format!("unexpected text at position {pos}.")));
                }
                while i < n && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                if stack.len() > max_depth {
                    return Err(ScanError::TooDeep);
                }
                let idx = nodes.len() as u32;
                nodes.push(Node {
                    name_start: ns,
                    name_end: i,
                    start: pos,
                    end: pos,
                    args: Vec::new(),
                    first_child: NONE,
                    last_child: NONE,
                    next: NONE,
                });
                let parent = *stack.last().expect("root") as usize;
                let last = nodes[parent].last_child;
                if last == NONE {
                    nodes[parent].first_child = idx;
                } else {
                    nodes[last as usize].next = idx;
                }
                nodes[parent].last_child = idx;
                stack.push(idx);
                pos = i;
                content_on_line = true;
            }
            b'>' => {
                if stack.len() == 1 {
                    return Err(syntax(format!("unbalanced '>' at position {pos}.")));
                }
                let nd = stack.pop().expect("checked") as usize;
                nodes[nd].end = pos + 1;
                pos += 1;
                content_on_line = true;
            }
            b'#' => {
                while pos < n && b[pos] != b'\n' && b[pos] != b'\r' {
                    pos += 1;
                }
                content_on_line = true;
            }
            b'=' | b'&' if !content_on_line => {
                // an inset facet line: opaque, to the end of the line
                while pos < n && b[pos] != b'\n' && b[pos] != b'\r' {
                    pos += 1;
                }
            }
            b'\'' => return Err(syntax(format!("unexpected text at position {pos}."))),
            _ => {
                // white space or an atom
                let ch_len = if c < 0x80 {
                    1
                } else {
                    text[pos..].chars().next().map_or(1, char::len_utf8)
                };
                let ws = if c < 0x80 {
                    is_ascii_ws(c)
                } else {
                    text[pos..].chars().next().is_some_and(is_ws)
                };
                if ws {
                    let mut i = pos + ch_len;
                    let mut nl = c == b'\n';
                    while i < n {
                        let d = b[i];
                        if d < 0x80 {
                            if !is_ascii_ws(d) {
                                break;
                            }
                            nl |= d == b'\n';
                            i += 1;
                        } else {
                            let ch = text[i..].chars().next().expect("char");
                            if !is_ws(ch) {
                                break;
                            }
                            i += ch.len_utf8();
                        }
                    }
                    if nl {
                        content_on_line = false;
                    }
                    pos = i;
                } else {
                    let mut i = pos;
                    while i < n {
                        let d = b[i];
                        if d < 0x80 {
                            if is_ascii_ws(d) || matches!(d, b'<' | b'>' | b'`' | b'#' | b'\'') {
                                break;
                            }
                            i += 1;
                        } else {
                            let ch = text[i..].chars().next().expect("char");
                            if is_ws(ch) {
                                break;
                            }
                            i += ch.len_utf8();
                        }
                    }
                    let top = *stack.last().expect("root") as usize;
                    nodes[top].args.push(Arg {
                        is_str: false,
                        start: pos,
                        end: i,
                    });
                    pos = i;
                    content_on_line = true;
                }
            }
        }
    }
    if stack.len() != 1 {
        let top = *stack.last().expect("root");
        let t = Tree { nodes };
        let name = t.name(text, top).to_string();
        return Err(syntax(format!("statement <{name}> is never closed.")));
    }
    Ok(Tree { nodes })
}

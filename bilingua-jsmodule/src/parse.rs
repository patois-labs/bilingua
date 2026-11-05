//! The parser: accepts only object and array literals with literal keys, string / template / number / true / false /
//! null values, static `'a' + 'b'` concatenation and references to a const declared earlier in the file. TypeScript
//! types are skipped (they never hold a message value). Everything else refuses the file with a reason and a line.

use crate::lex::{lex, refuse, short, Kind, Tok};
use crate::{Limits, ModuleError};
use std::collections::HashSet;

type R<T> = Result<T, ModuleError>;

const MAX_DEPTH: usize = 32;

/// A statement keyword at the start of a new line ends a TypeScript type that had no `;`.
const STMT_WORDS: &[&str] = &[
    "export",
    "const",
    "let",
    "var",
    "type",
    "interface",
    "import",
    "module",
    "exports",
    "declare",
];

fn code_word(w: &str) -> Option<&'static str> {
    Some(match w {
        "function" => "a function",
        "class" => "a class",
        "new" => "a `new` expression",
        "async" => "an async function",
        "await" => "an await expression",
        "require" => "a require() call",
        "import" => "an import",
        "this" => "`this`",
        "typeof" => "a typeof expression",
        "void" => "a void expression",
        "delete" => "a delete expression",
        "yield" => "a yield expression",
        _ => return None,
    })
}

/// One string value found in the file.
pub struct RawUnit {
    pub start: usize,
    pub end: usize,
    pub quote: u8,
    pub multiline: bool,
    pub value: String,
    /// Key path joined with dots (the root name included for a `const`).
    pub path: String,
    pub root: String,
}

pub struct Parsed {
    pub units: Vec<RawUnit>,
    pub default_ref: Option<String>,
    /// Export / declaration statements seen (a file with none is not a locale module worth claiming by sniffing).
    pub statements: usize,
}

struct P<'a, 'l> {
    text: &'a str,
    toks: Vec<Tok>,
    i: usize,
    limits: &'l Limits,
    units: Vec<RawUnit>,
    declared: HashSet<String>,
    default_ref: Option<String>,
    statements: usize,
    path: Vec<String>,
}

pub fn parse(text: &str, limits: &Limits) -> R<Parsed> {
    let toks = lex(text, limits)?;
    let mut p = P {
        text,
        toks,
        i: 0,
        limits,
        units: Vec::new(),
        declared: HashSet::new(),
        default_ref: None,
        statements: 0,
        path: Vec::new(),
    };
    p.walk()?;
    Ok(Parsed {
        units: p.units,
        default_ref: p.default_ref,
        statements: p.statements,
    })
}

impl<'a> P<'a, '_> {
    fn raw(&self, j: usize) -> &'a str {
        let t = &self.toks[j];
        &self.text[t.start..t.end]
    }

    fn has(&self, j: usize) -> bool {
        j < self.toks.len()
    }

    fn is(&self, j: usize, kind: Kind, raw: &str) -> bool {
        self.has(j) && self.toks[j].kind == kind && self.raw(j) == raw
    }

    fn is_kind(&self, j: usize, kind: Kind) -> bool {
        self.has(j) && self.toks[j].kind == kind
    }

    fn is_p(&self, j: usize, raw: &str) -> bool {
        self.is(j, Kind::Punct, raw)
    }

    fn is_id(&self, j: usize, raw: &str) -> bool {
        self.is(j, Kind::Ident, raw)
    }

    fn fail<T>(&self, pos: usize, why: &str) -> R<T> {
        refuse(self.text, pos, why)
    }

    fn next(&mut self) -> R<usize> {
        if self.i >= self.toks.len() {
            return self.fail(
                self.text.len(),
                "the file ends in the middle of a statement",
            );
        }
        self.i += 1;
        Ok(self.i - 1)
    }

    fn expect(&mut self, kind: Kind, raw: &str, what: &str) -> R<usize> {
        let t = self.next()?;
        if !self.is(t, kind, raw) {
            return self.fail(
                self.toks[t].start,
                &format!("expected {what}, found {}", short(self.raw(t))),
            );
        }
        Ok(t)
    }

    fn end_statement(&mut self) -> R<()> {
        if self.i >= self.toks.len() {
            return Ok(());
        }
        if self.is_p(self.i, ";") {
            self.i += 1;
            return Ok(());
        }
        if self.toks[self.i].nl {
            return Ok(());
        }
        self.fail(
            self.toks[self.i].start,
            &format!(
                "expected the end of the statement, found {}",
                short(self.raw(self.i))
            ),
        )
    }

    fn name(&mut self) -> R<String> {
        let t = self.next()?;
        if self.toks[t].kind != Kind::Ident || code_word(self.raw(t)).is_some() {
            return self.fail(
                self.toks[t].start,
                &format!("expected a name, found {}", short(self.raw(t))),
            );
        }
        Ok(self.raw(t).to_string())
    }

    // ---- statements
    fn walk(&mut self) -> R<()> {
        let mut first = true;
        while self.i < self.toks.len() {
            let t = self.i;
            if self.is_p(t, ";") {
                self.i += 1;
            } else if self.toks[t].kind == Kind::Str
                && first
                && self.toks[t].cooked == "use strict"
                && self.toks[t].quote != b'`'
            {
                self.i += 1;
                self.end_statement()?;
            } else if self.is_id(t, "export") {
                self.statements += 1;
                self.export()?;
            } else if self.is_kind(t, Kind::Ident) && matches!(self.raw(t), "const" | "let" | "var")
            {
                self.statements += 1;
                self.decl()?;
            } else if self.is_id(t, "import") {
                self.import_type()?;
            } else if self.is_id(t, "type") && self.is_kind(t + 1, Kind::Ident) {
                self.i += 1;
                self.type_alias()?;
            } else if self.is_id(t, "interface") {
                self.i += 1;
                self.interface()?;
            } else if self.is_id(t, "module") && self.is_p(t + 1, ".") {
                self.statements += 1;
                self.i += 2;
                let t2 = self.next()?;
                if !self.is_id(t2, "exports") {
                    return self.fail(
                        self.toks[t2].start,
                        "only `module.exports = {...}` is allowed",
                    );
                }
                self.cjs_assign(false)?;
            } else if self.is_id(t, "exports") && self.is_p(t + 1, ".") {
                self.statements += 1;
                self.i += 1;
                self.cjs_assign(true)?;
            } else {
                let why = if self.toks[t].kind == Kind::Ident {
                    code_word(self.raw(t)).map(str::to_string)
                } else {
                    None
                };
                let why = why.unwrap_or_else(|| {
                    format!("{} is not a static message export", short(self.raw(t)))
                });
                return self.fail(self.toks[t].start, &why);
            }
            first = false;
        }
        Ok(())
    }

    fn import_type(&mut self) -> R<()> {
        let start = self.next()?;
        if !self.is_id(self.i, "type") {
            return self.fail(
                self.toks[start].start,
                "an import (only `import type ... from '...'` is allowed)",
            );
        }
        self.i += 1;
        loop {
            let t = self.next()?;
            if self.is_id(t, "from") {
                break;
            }
            let ok = self.toks[t].kind == Kind::Ident
                || (self.toks[t].kind == Kind::Punct
                    && matches!(self.raw(t), "{" | "}" | "," | "*"));
            if !ok {
                return self.fail(
                    self.toks[t].start,
                    &format!("unexpected {} in an import type", short(self.raw(t))),
                );
            }
        }
        let spec = self.next()?;
        if self.toks[spec].kind != Kind::Str || self.toks[spec].quote == b'`' {
            return self.fail(self.toks[spec].start, "expected a module name");
        }
        self.end_statement()
    }

    fn export(&mut self) -> R<()> {
        let start = self.next()?;
        let t = self.i;
        if self.is_id(t, "default") {
            self.i += 1;
            let r = self.value(0, "")?;
            if let Some(r) = r {
                self.default_ref = Some(r);
            }
            self.end_statement()
        } else if self.is_kind(t, Kind::Ident) && matches!(self.raw(t), "const" | "let" | "var") {
            self.decl()
        } else if self.is_id(t, "type") && self.is_kind(t + 1, Kind::Ident) {
            self.i += 1;
            self.type_alias()
        } else if self.is_id(t, "interface") {
            self.i += 1;
            self.interface()
        } else if self.is_p(t, "{") {
            self.i += 1;
            self.export_list()
        } else {
            let why = if self.is_kind(t, Kind::Ident) {
                code_word(self.raw(t))
            } else {
                None
            };
            self.fail(
                self.toks[start].start,
                why.unwrap_or("an export that is not a static object"),
            )
        }
    }

    fn export_list(&mut self) -> R<()> {
        loop {
            let t = self.next()?;
            if self.is_p(t, "}") {
                break;
            }
            if self.toks[t].kind != Kind::Ident || !self.declared.contains(self.raw(t)) {
                return self.fail(
                    self.toks[t].start,
                    &format!(
                        "export of {}, which is not declared here",
                        short(self.raw(t))
                    ),
                );
            }
            if self.is_id(self.i, "as") {
                self.i += 1;
                let alias = self.next()?;
                if self.is_id(alias, "default") {
                    self.default_ref = Some(self.raw(t).to_string());
                } else if self.toks[alias].kind != Kind::Ident {
                    return self.fail(self.toks[alias].start, "expected a name");
                }
            }
            let t = self.next()?;
            if self.is_p(t, "}") {
                break;
            }
            if !self.is_p(t, ",") {
                return self.fail(
                    self.toks[t].start,
                    &format!("expected ',' or '}}', found {}", short(self.raw(t))),
                );
            }
        }
        if self.is_id(self.i, "from") {
            return self.fail(self.toks[self.i].start, "a re-export from another module");
        }
        self.end_statement()
    }

    fn decl(&mut self) -> R<()> {
        self.i += 1; // const / let / var
        loop {
            let t = self.i;
            let name = self.name()?;
            if self.is_p(self.i, ":") {
                self.i += 1;
                self.skip_type("=")?;
            }
            if !self.is_p(self.i, "=") {
                return self.fail(
                    self.toks[t.min(self.toks.len() - 1)].start,
                    "a declaration without a literal value",
                );
            }
            self.i += 1;
            self.path.push(name.clone());
            let r = self.value(0, &name);
            self.path.pop();
            r?;
            self.declared.insert(name);
            if self.is_p(self.i, ",") {
                self.i += 1;
                continue;
            }
            break;
        }
        self.end_statement()
    }

    fn cjs_assign(&mut self, prefix_required: bool) -> R<()> {
        let base = self.path.len();
        while self.is_p(self.i, ".") {
            self.i += 1;
            let n = match self.name() {
                Ok(n) => n,
                Err(e) => {
                    self.path.truncate(base);
                    return Err(e);
                }
            };
            self.path.push(n);
        }
        let has_path = self.path.len() > base;
        let res = (|| {
            if prefix_required && !has_path {
                let pos = if self.i < self.toks.len() {
                    self.toks[self.i].start
                } else {
                    0
                };
                return self.fail(pos, "expected exports.name");
            }
            self.expect(Kind::Punct, "=", "'='")?;
            let r = self.value(0, "")?;
            if let (Some(r), false) = (r, has_path) {
                self.default_ref = Some(r);
            }
            self.end_statement()
        })();
        self.path.truncate(base);
        res
    }

    fn type_alias(&mut self) -> R<()> {
        self.name()?;
        if self.is_p(self.i, "<") {
            self.skip_type("=")?;
        }
        self.expect(Kind::Punct, "=", "'='")?;
        self.skip_type(";")?;
        self.end_statement()
    }

    fn interface(&mut self) -> R<()> {
        self.name()?;
        while !self.is_p(self.i, "{") {
            let t = self.next()?;
            let ok = matches!(self.toks[t].kind, Kind::Ident | Kind::Punct)
                && !matches!(self.raw(t), "=" | ";");
            if !ok {
                return self.fail(
                    self.toks[t].start,
                    &format!("unexpected {} in an interface", short(self.raw(t))),
                );
            }
        }
        self.skip_type("")
    }

    /// Skips a TypeScript type: balanced brackets; stops at depth 0 before a stop punctuator, a closing bracket
    /// of the enclosing literal, or a statement keyword on a new line.
    fn skip_type(&mut self, stops: &str) -> R<()> {
        let mut depth = 0i32;
        let mut moved = false;
        while self.i < self.toks.len() {
            let t = self.i;
            let punct = self.toks[t].kind == Kind::Punct;
            let raw = self.raw(t);
            if depth == 0 {
                if punct && (raw == stops || matches!(raw, "," | ";" | ")" | "]" | "}" | ">")) {
                    if !moved && raw != stops {
                        return self.fail(self.toks[t].start, "a missing type");
                    }
                    return Ok(());
                }
                if moved
                    && self.toks[t].nl
                    && self.toks[t].kind == Kind::Ident
                    && STMT_WORDS.contains(&raw)
                {
                    return Ok(());
                }
            }
            if punct && matches!(raw, "(" | "[" | "{" | "<") {
                depth += 1;
            } else if punct && matches!(raw, ")" | "]" | "}" | ">") {
                depth -= 1;
            }
            self.i += 1;
            moved = true;
        }
        Ok(())
    }

    // ---- values
    /// Reads one value; returns the referenced name when the value is a bare reference to a declared const.
    fn value(&mut self, depth: usize, root: &str) -> R<Option<String>> {
        if depth > MAX_DEPTH {
            return Err(ModuleError::Limit("This file is nested too deeply.".into()));
        }
        let t = self.next()?;
        let mut r = None;
        let tk = self.toks[t].kind;
        let raw = self.raw(t);
        if tk == Kind::Punct && raw == "(" {
            if self.is_p(self.i, ")") {
                return self.fail(self.toks[t].start, "an arrow function");
            }
            r = self.value(depth + 1, root)?;
            self.expect(Kind::Punct, ")", "')'")?;
        } else if tk == Kind::Str {
            let mut last = t;
            while self.is_p(self.i, "+") {
                self.i += 1;
                let nxt = self.next()?;
                if self.toks[nxt].kind != Kind::Str {
                    return self.fail(
                        self.toks[nxt].start,
                        "only string literals may be concatenated",
                    );
                }
                last = nxt;
            }
            self.unit(t, last, root)?;
        } else if tk == Kind::Punct && raw == "{" {
            self.object(depth + 1, root)?;
        } else if tk == Kind::Punct && raw == "[" {
            self.array(depth + 1, root)?;
        } else if tk == Kind::Punct && (raw == "-" || raw == "+") && self.is_kind(self.i, Kind::Num)
        {
            self.i += 1;
        } else if tk == Kind::Num || (tk == Kind::Ident && matches!(raw, "true" | "false" | "null"))
        {
        } else if tk == Kind::Ident {
            let nxt = self.i;
            if self.is_p(nxt, "(") || (self.is_kind(nxt, Kind::Str) && self.toks[nxt].quote == b'`')
            {
                return self.fail(
                    self.toks[t].start,
                    &format!(
                        "a function call ({}(...))",
                        raw.chars().take(20).collect::<String>()
                    ),
                );
            }
            if self.is_p(nxt, "=>") {
                return self.fail(self.toks[t].start, "an arrow function");
            }
            if let Some(w) = code_word(raw) {
                return self.fail(self.toks[t].start, w);
            }
            if !self.declared.contains(raw) {
                return self.fail(
                    self.toks[t].start,
                    &format!("{} is a variable, not a literal value", short(raw)),
                );
            }
            r = Some(raw.to_string());
        } else {
            return self.fail(
                self.toks[t].start,
                &format!("{} is not a literal value", short(raw)),
            );
        }
        // postfix: TypeScript `as const` / `as T` / `satisfies T`; anything else is code
        loop {
            let nxt = self.i;
            if self.is_kind(nxt, Kind::Ident) && matches!(self.raw(nxt), "as" | "satisfies") {
                self.i += 1;
                self.skip_type("")?;
                continue;
            }
            if self.is_kind(nxt, Kind::Punct) && !self.toks[nxt].nl {
                match self.raw(nxt) {
                    "(" | "?." => return self.fail(self.toks[nxt].start, "a function call"),
                    "." | "[" => return self.fail(self.toks[nxt].start, "a property access"),
                    _ => {}
                }
            }
            break;
        }
        Ok(r)
    }

    fn unit(&mut self, first: usize, last: usize, root: &str) -> R<()> {
        let mut value = String::new();
        let mut j = first;
        while j <= last {
            if self.toks[j].kind == Kind::Str {
                value.push_str(&self.toks[j].cooked);
            }
            j += 1;
        }
        let f = &self.toks[first];
        let multiline = f.quote == b'`' && self.text[f.start..f.end].contains('\n');
        self.units.push(RawUnit {
            start: f.start,
            end: self.toks[last].end,
            quote: f.quote,
            multiline,
            value,
            path: self.path.join("."),
            root: root.to_string(),
        });
        if self.units.len() > self.limits.max_units {
            return Err(ModuleError::Limit(format!(
                "This file has more than {} segments. Split it into smaller files.",
                self.limits.max_units
            )));
        }
        Ok(())
    }

    fn key(&mut self) -> R<String> {
        let t = self.next()?;
        let raw = self.raw(t);
        let start = self.toks[t].start;
        match self.toks[t].kind {
            Kind::Punct if raw == "..." => self.fail(start, "a spread (...)"),
            Kind::Punct if raw == "[" => self.fail(start, "a computed key [...]"),
            Kind::Str => {
                if self.toks[t].quote == b'`' {
                    return self.fail(start, "a template literal used as a key");
                }
                Ok(self.toks[t].cooked.clone())
            }
            Kind::Ident | Kind::Num => {
                if self.toks[t].kind == Kind::Ident
                    && matches!(raw, "get" | "set" | "async")
                    && self.is_kind(self.i, Kind::Ident)
                {
                    return self.fail(start, "a getter, setter or method");
                }
                Ok(raw.to_string())
            }
            Kind::Punct if raw == "*" => self.fail(start, "a generator method"),
            _ => self.fail(start, &format!("{} is not a literal key", short(raw))),
        }
    }

    fn object(&mut self, depth: usize, root: &str) -> R<()> {
        loop {
            if self.is_p(self.i, "}") {
                self.i += 1;
                return Ok(());
            }
            let kt = self.i;
            let key = self.key()?;
            let t = self.i;
            if self.is_p(t, ":") {
                self.i += 1;
                self.path.push(key);
                let r = self.value(depth, root);
                self.path.pop();
                r?;
            } else if self.is_kind(t, Kind::Punct) && matches!(self.raw(t), "(" | "<") {
                return self.fail(
                    self.toks[kt].start,
                    &format!(
                        "a method ({}() {{...}})",
                        key.chars().take(20).collect::<String>()
                    ),
                );
            } else if self.is_kind(t, Kind::Punct) && matches!(self.raw(t), "," | "}") {
                if self.toks[kt].kind != Kind::Ident || !self.declared.contains(&key) {
                    return self.fail(
                        self.toks[kt].start,
                        &format!("shorthand property {} refers to a variable", short(&key)),
                    );
                }
            } else {
                return self.fail(
                    self.toks[kt].start,
                    &format!("expected ':' after key {}", short(&key)),
                );
            }
            let t = self.next()?;
            if self.is_p(t, "}") {
                return Ok(());
            }
            if !self.is_p(t, ",") {
                return self.fail(
                    self.toks[t].start,
                    &format!("expected ',' or '}}', found {}", short(self.raw(t))),
                );
            }
        }
    }

    fn array(&mut self, depth: usize, root: &str) -> R<()> {
        let mut idx = 0usize;
        loop {
            let t = self.i;
            if self.is_p(t, "]") {
                self.i += 1;
                return Ok(());
            }
            if self.is_p(t, "...") {
                return self.fail(self.toks[t].start, "a spread (...)");
            }
            if self.is_p(t, ",") {
                return self.fail(self.toks[t].start, "an empty array slot");
            }
            self.path.push(idx.to_string());
            let r = self.value(depth, root);
            self.path.pop();
            r?;
            idx += 1;
            let t = self.next()?;
            if self.is_p(t, "]") {
                return Ok(());
            }
            if !self.is_p(t, ",") {
                return self.fail(
                    self.toks[t].start,
                    &format!("expected ',' or ']', found {}", short(self.raw(t))),
                );
            }
        }
    }
}

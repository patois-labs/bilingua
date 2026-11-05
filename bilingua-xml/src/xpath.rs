//! A small XPath 1.0 engine over [`Tree`]: location paths with every axis, predicates, unions, the core function
//! library, comparisons and arithmetic. It is what ITS selectors, `locNotePointer` and the multilingual XPath
//! rules run on. Name tests without a prefix match no-namespace names (XPath 1.0), prefixed ones use the bindings
//! given at compile time. Work is bounded by the caller's own cost checks ([`check_cost`]) plus a step budget.

use crate::tree::{decode_refs, Kind, Tree, NONE};
use std::cmp::Ordering;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Node {
    Doc,
    El(u32),
    /// (element, index into `Tree::attrs`)
    At(u32, u32),
    /// (element or NONE for document level, item index, in prolog 0 / epilog 1 when NONE)
    Tx(u32, u32),
}

#[derive(Debug, Clone)]
enum Axis {
    Child,
    Descendant,
    DescendantOrSelf,
    Parent,
    Ancestor,
    AncestorOrSelf,
    SelfAx,
    Attribute,
    FollowingSibling,
    PrecedingSibling,
    Following,
    Preceding,
}

#[derive(Debug, Clone)]
enum Test {
    /// namespace uri ("" = none), local name (None = any)
    Name(String, Option<String>),
    /// `prefix:*` is Name(uri, None); plain `*` is AnyName
    AnyName,
    Text,
    Comment,
    Pi,
    AnyNode,
}

#[derive(Debug, Clone)]
struct Step {
    axis: Axis,
    test: Test,
    preds: Vec<E>,
}

#[derive(Debug, Clone)]
enum Op {
    Or,
    And,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
}

#[derive(Debug, Clone)]
enum E {
    Num(f64),
    Str(String),
    Func(String, Vec<E>),
    Path { abs: bool, start: Option<Box<E>>, preds: Vec<E>, steps: Vec<Step> },
    Bin(Op, Box<E>, Box<E>),
    Neg(Box<E>),
    Union(Vec<E>),
}

/// A compiled expression.
#[derive(Debug, Clone)]
pub(crate) struct XPath(E);

#[derive(Debug, Clone)]
pub(crate) enum Val {
    Nodes(Vec<Node>),
    Num(f64),
    Str(String),
    Bool(bool),
}

// ---------------------------------------------------------------- lexer

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Str(String),
    Name(String), // NCName or QName or prefix:*
    Axis(String), // name followed by ::
    Func(String), // name followed by (
    NodeType(String),
    Var,
    Op(&'static str),
    Star,
    Eof,
}

fn lex(src: &str) -> Result<Vec<Tok>, String> {
    let c: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut out: Vec<Tok> = Vec::new();
    let prev_is_operand = |out: &Vec<Tok>| -> bool {
        match out.last() {
            None => false,
            Some(Tok::Op(o)) => matches!(*o, ")" | "]" | "." | ".."),
            Some(Tok::Axis(_)) | Some(Tok::Func(_)) | Some(Tok::Var) => false,
            Some(_) => true,
        }
    };
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
            continue;
        }
        if ch == '"' || ch == '\'' {
            let Some(j) = c[i + 1..].iter().position(|x| *x == ch) else { return Err("unterminated string".into()) };
            out.push(Tok::Str(c[i + 1..i + 1 + j].iter().collect()));
            i += j + 2;
            continue;
        }
        if ch.is_ascii_digit() || (ch == '.' && c.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let s = i;
            while i < c.len() && c[i].is_ascii_digit() {
                i += 1;
            }
            if i < c.len() && c[i] == '.' {
                i += 1;
                while i < c.len() && c[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let t: String = c[s..i].iter().collect();
            out.push(Tok::Num(t.parse().map_err(|_| "bad number".to_string())?));
            continue;
        }
        let two: String = c[i..(i + 2).min(c.len())].iter().collect();
        match two.as_str() {
            "//" => {
                out.push(Tok::Op("//"));
                i += 2;
                continue;
            }
            ".." => {
                out.push(Tok::Op(".."));
                i += 2;
                continue;
            }
            "!=" => {
                out.push(Tok::Op("!="));
                i += 2;
                continue;
            }
            "<=" => {
                out.push(Tok::Op("<="));
                i += 2;
                continue;
            }
            ">=" => {
                out.push(Tok::Op(">="));
                i += 2;
                continue;
            }
            _ => {}
        }
        match ch {
            '/' => out.push(Tok::Op("/")),
            '(' => out.push(Tok::Op("(")),
            ')' => out.push(Tok::Op(")")),
            '[' => out.push(Tok::Op("[")),
            ']' => out.push(Tok::Op("]")),
            '.' => out.push(Tok::Op(".")),
            '@' => out.push(Tok::Op("@")),
            ',' => out.push(Tok::Op(",")),
            '|' => out.push(Tok::Op("|")),
            '+' => out.push(Tok::Op("+")),
            '-' => out.push(Tok::Op("-")),
            '=' => out.push(Tok::Op("=")),
            '<' => out.push(Tok::Op("<")),
            '>' => out.push(Tok::Op(">")),
            '$' => out.push(Tok::Var),
            '*' => {
                if prev_is_operand(&out) {
                    out.push(Tok::Op("*"));
                } else {
                    out.push(Tok::Star);
                }
            }
            _ if crate::tree::is_name_start(ch) => {
                let s = i;
                while i < c.len() && (crate::tree::is_name_char(c[i]) && c[i] != ':') {
                    i += 1;
                }
                // prefix:local or prefix:*
                if i + 1 < c.len() && c[i] == ':' && c[i + 1] != ':' {
                    if c[i + 1] == '*' {
                        i += 2;
                    } else if crate::tree::is_name_start(c[i + 1]) {
                        i += 1;
                        while i < c.len() && (crate::tree::is_name_char(c[i]) && c[i] != ':') {
                            i += 1;
                        }
                    }
                }
                let name: String = c[s..i].iter().collect();
                if prev_is_operand(&out) && matches!(name.as_str(), "and" | "or" | "div" | "mod") {
                    out.push(Tok::Op(match name.as_str() {
                        "and" => "and",
                        "or" => "or",
                        "div" => "div",
                        _ => "mod",
                    }));
                    continue;
                }
                let mut j = i;
                while j < c.len() && c[j].is_whitespace() {
                    j += 1;
                }
                if j + 1 < c.len() && c[j] == ':' && c[j + 1] == ':' {
                    out.push(Tok::Axis(name));
                    i = j + 2;
                } else if j < c.len() && c[j] == '(' {
                    if matches!(name.as_str(), "text" | "comment" | "node" | "processing-instruction") {
                        out.push(Tok::NodeType(name));
                    } else {
                        out.push(Tok::Func(name));
                    }
                } else {
                    out.push(Tok::Name(name));
                }
                continue;
            }
            _ => return Err(format!("unexpected character {ch:?}")),
        }
        i += 1;
    }
    out.push(Tok::Eof);
    Ok(out)
}

// ---------------------------------------------------------------- parser

struct P<'a> {
    t: Vec<Tok>,
    i: usize,
    ns: &'a [(String, String)],
    depth: usize,
}

impl P<'_> {
    fn peek(&self) -> &Tok {
        &self.t[self.i]
    }
    fn next(&mut self) -> Tok {
        let t = self.t[self.i].clone();
        if self.i < self.t.len() - 1 {
            self.i += 1;
        }
        t
    }
    fn is_op(&self, o: &str) -> bool {
        matches!(self.peek(), Tok::Op(x) if *x == o)
    }
    fn eat(&mut self, o: &str) -> bool {
        if self.is_op(o) {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, o: &str) -> Result<(), String> {
        if self.eat(o) {
            Ok(())
        } else {
            Err(format!("expected {o:?}"))
        }
    }

    fn enter(&mut self) -> Result<(), String> {
        self.depth += 1;
        if self.depth > 64 {
            return Err("expression nests too deeply".into());
        }
        Ok(())
    }

    fn or(&mut self) -> Result<E, String> {
        self.enter()?;
        let mut l = self.and()?;
        while self.eat("or") {
            let r = self.and()?;
            l = E::Bin(Op::Or, Box::new(l), Box::new(r));
        }
        self.depth -= 1;
        Ok(l)
    }
    fn and(&mut self) -> Result<E, String> {
        let mut l = self.equality()?;
        while self.eat("and") {
            let r = self.equality()?;
            l = E::Bin(Op::And, Box::new(l), Box::new(r));
        }
        Ok(l)
    }
    fn equality(&mut self) -> Result<E, String> {
        let mut l = self.relational()?;
        loop {
            let op = if self.eat("=") {
                Op::Eq
            } else if self.eat("!=") {
                Op::Ne
            } else {
                return Ok(l);
            };
            let r = self.relational()?;
            l = E::Bin(op, Box::new(l), Box::new(r));
        }
    }
    fn relational(&mut self) -> Result<E, String> {
        let mut l = self.additive()?;
        loop {
            let op = if self.eat("<=") {
                Op::Le
            } else if self.eat(">=") {
                Op::Ge
            } else if self.eat("<") {
                Op::Lt
            } else if self.eat(">") {
                Op::Gt
            } else {
                return Ok(l);
            };
            let r = self.additive()?;
            l = E::Bin(op, Box::new(l), Box::new(r));
        }
    }
    fn additive(&mut self) -> Result<E, String> {
        let mut l = self.multiplicative()?;
        loop {
            let op = if self.eat("+") {
                Op::Add
            } else if self.eat("-") {
                Op::Sub
            } else {
                return Ok(l);
            };
            let r = self.multiplicative()?;
            l = E::Bin(op, Box::new(l), Box::new(r));
        }
    }
    fn multiplicative(&mut self) -> Result<E, String> {
        let mut l = self.unary()?;
        loop {
            let op = if self.eat("*") {
                Op::Mul
            } else if self.eat("div") {
                Op::Div
            } else if self.eat("mod") {
                Op::Mod
            } else {
                return Ok(l);
            };
            let r = self.unary()?;
            l = E::Bin(op, Box::new(l), Box::new(r));
        }
    }
    fn unary(&mut self) -> Result<E, String> {
        if self.eat("-") {
            self.enter()?;
            let e = self.unary()?;
            self.depth -= 1;
            return Ok(E::Neg(Box::new(e)));
        }
        self.union()
    }
    fn union(&mut self) -> Result<E, String> {
        let first = self.path()?;
        if !self.is_op("|") {
            return Ok(first);
        }
        let mut v = vec![first];
        while self.eat("|") {
            v.push(self.path()?);
        }
        Ok(E::Union(v))
    }

    fn path(&mut self) -> Result<E, String> {
        // filter expression or location path
        let starts_primary = matches!(self.peek(), Tok::Num(_) | Tok::Str(_) | Tok::Func(_) | Tok::Var) || self.is_op("(");
        if starts_primary {
            let prim = self.primary()?;
            let mut preds = Vec::new();
            while self.is_op("[") {
                preds.push(self.predicate()?);
            }
            let mut steps = Vec::new();
            if self.is_op("/") || self.is_op("//") {
                self.rel_steps(&mut steps)?;
            }
            if preds.is_empty() && steps.is_empty() {
                return Ok(prim);
            }
            return Ok(E::Path { abs: false, start: Some(Box::new(prim)), preds, steps });
        }
        let mut steps = Vec::new();
        let abs = if self.eat("/") {
            // a lone "/" is the document node
            if !self.step_starts() {
                return Ok(E::Path { abs: true, start: None, preds: vec![], steps });
            }
            true
        } else if self.eat("//") {
            steps.push(Step { axis: Axis::DescendantOrSelf, test: Test::AnyNode, preds: vec![] });
            true
        } else {
            false
        };
        steps.push(self.step()?);
        while self.is_op("/") || self.is_op("//") {
            self.rel_steps(&mut steps)?;
        }
        Ok(E::Path { abs, start: None, preds: vec![], steps })
    }

    fn rel_steps(&mut self, steps: &mut Vec<Step>) -> Result<(), String> {
        while self.is_op("/") || self.is_op("//") {
            if self.eat("//") {
                steps.push(Step { axis: Axis::DescendantOrSelf, test: Test::AnyNode, preds: vec![] });
            } else {
                self.eat("/");
            }
            steps.push(self.step()?);
        }
        Ok(())
    }

    fn step_starts(&self) -> bool {
        matches!(self.peek(), Tok::Name(_) | Tok::Axis(_) | Tok::NodeType(_) | Tok::Star) || self.is_op("@") || self.is_op(".") || self.is_op("..")
    }

    fn predicate(&mut self) -> Result<E, String> {
        self.expect("[")?;
        let e = self.or()?;
        self.expect("]")?;
        Ok(e)
    }

    fn step(&mut self) -> Result<Step, String> {
        if self.eat(".") {
            return Ok(Step { axis: Axis::SelfAx, test: Test::AnyNode, preds: vec![] });
        }
        if self.eat("..") {
            return Ok(Step { axis: Axis::Parent, test: Test::AnyNode, preds: vec![] });
        }
        let mut axis = Axis::Child;
        if self.eat("@") {
            axis = Axis::Attribute;
        } else if let Tok::Axis(a) = self.peek().clone() {
            self.i += 1;
            axis = match a.as_str() {
                "child" => Axis::Child,
                "descendant" => Axis::Descendant,
                "descendant-or-self" => Axis::DescendantOrSelf,
                "parent" => Axis::Parent,
                "ancestor" => Axis::Ancestor,
                "ancestor-or-self" => Axis::AncestorOrSelf,
                "self" => Axis::SelfAx,
                "attribute" => Axis::Attribute,
                "following-sibling" => Axis::FollowingSibling,
                "preceding-sibling" => Axis::PrecedingSibling,
                "following" => Axis::Following,
                "preceding" => Axis::Preceding,
                other => return Err(format!("unsupported axis {other}")),
            };
        }
        let test = match self.next() {
            Tok::Star => Test::AnyName,
            Tok::NodeType(n) => {
                self.expect("(")?;
                if n == "processing-instruction" {
                    if let Tok::Str(_) = self.peek() {
                        self.i += 1;
                    }
                }
                self.expect(")")?;
                match n.as_str() {
                    "text" => Test::Text,
                    "comment" => Test::Comment,
                    "processing-instruction" => Test::Pi,
                    _ => Test::AnyNode,
                }
            }
            Tok::Name(n) => match n.split_once(':') {
                None => Test::Name(String::new(), Some(n)),
                Some((p, l)) => {
                    let uri = self.ns.iter().find(|(q, _)| q == p).map(|(_, u)| u.clone()).ok_or_else(|| format!("undefined namespace prefix {p}"))?;
                    Test::Name(uri, if l == "*" { None } else { Some(l.to_string()) })
                }
            },
            t => return Err(format!("unexpected token {t:?}")),
        };
        let mut preds = Vec::new();
        while self.is_op("[") {
            preds.push(self.predicate()?);
        }
        Ok(Step { axis, test, preds })
    }

    fn primary(&mut self) -> Result<E, String> {
        match self.next() {
            Tok::Num(n) => Ok(E::Num(n)),
            Tok::Str(s) => Ok(E::Str(s)),
            Tok::Var => Err("variables are not supported".into()),
            Tok::Func(name) => {
                self.expect("(")?;
                let mut args = Vec::new();
                if !self.is_op(")") {
                    args.push(self.or()?);
                    while self.eat(",") {
                        args.push(self.or()?);
                    }
                }
                self.expect(")")?;
                Ok(E::Func(name, args))
            }
            Tok::Op("(") => {
                let e = self.or()?;
                self.expect(")")?;
                Ok(e)
            }
            t => Err(format!("unexpected token {t:?}")),
        }
    }
}

impl XPath {
    /// Compiles `src`; `ns` binds the prefixes the expression uses.
    pub(crate) fn compile(src: &str, ns: &[(String, String)]) -> Result<XPath, String> {
        let toks = lex(src)?;
        let mut p = P { t: toks, i: 0, ns, depth: 0 };
        let e = p.or()?;
        if *p.peek() != Tok::Eof {
            return Err(format!("unexpected {:?}", p.peek()));
        }
        Ok(XPath(e))
    }

    /// Evaluates with `ctx` as the context node; `budget` bounds the nodes visited.
    pub(crate) fn eval(&self, t: &Tree, ctx: Node, budget: &mut u64) -> Result<Val, String> {
        let ev = Ev { t };
        ev.expr(&self.0, ctx, 1, 1, budget)
    }

    pub(crate) fn select(&self, t: &Tree, ctx: Node, budget: &mut u64) -> Result<Vec<Node>, String> {
        match self.eval(t, ctx, budget)? {
            Val::Nodes(n) => Ok(n),
            _ => Ok(Vec::new()),
        }
    }
}

// ---------------------------------------------------------------- evaluator

struct Ev<'a, 'b> {
    t: &'b Tree<'a>,
}

fn order_key(t: &Tree, n: Node) -> (usize, u8, u32) {
    match n {
        Node::Doc => (0, 0, 0),
        Node::El(e) => (t.els[e as usize].s0, 1, 0),
        Node::At(e, a) => (t.els[e as usize].s0, 2, a),
        Node::Tx(e, i) => {
            let items = if e == NONE { &t.prolog } else { &t.els[e as usize].items };
            let s = items.get(i as usize).map_or(0, |x| x.s);
            (s, 3, i)
        }
    }
}

fn tick(budget: &mut u64, n: u64) -> Result<(), String> {
    if *budget < n {
        return Err("expression is too expensive for this document".into());
    }
    *budget -= n;
    Ok(())
}

impl<'a, 'b> Ev<'a, 'b> {
    fn to_bool(&self, v: &Val) -> bool {
        match v {
            Val::Nodes(n) => !n.is_empty(),
            Val::Num(n) => *n != 0.0 && !n.is_nan(),
            Val::Str(s) => !s.is_empty(),
            Val::Bool(b) => *b,
        }
    }
    fn to_str(&self, v: &Val) -> String {
        match v {
            Val::Nodes(n) => n.first().map(|x| self.node_str(*x)).unwrap_or_default(),
            Val::Num(n) => num_str(*n),
            Val::Str(s) => s.clone(),
            Val::Bool(b) => (if *b { "true" } else { "false" }).into(),
        }
    }
    fn to_num(&self, v: &Val) -> f64 {
        match v {
            Val::Num(n) => *n,
            Val::Bool(b) => f64::from(*b),
            other => str_num(&self.to_str(other)),
        }
    }

    fn node_str(&self, n: Node) -> String {
        let t = self.t;
        match n {
            Node::Doc => {
                let mut s = String::new();
                t.string_value(t.root, &mut s, 1 << 20);
                s
            }
            Node::El(e) => {
                let mut s = String::new();
                t.string_value(e, &mut s, 1 << 20);
                s
            }
            Node::At(_, a) => decode_refs(t.attr_raw(&t.attrs[a as usize]), |_| None),
            Node::Tx(e, i) => {
                let items = if e == NONE { &t.prolog } else { &t.els[e as usize].items };
                match items.get(i as usize) {
                    Some(it) => match it.kind {
                        Kind::Text => decode_refs(&t.text[it.s..it.e], |_| None),
                        Kind::Cdata => t.text[it.s + 9..it.e - 3].to_string(),
                        Kind::Comment => t.text[it.s + 4..it.e - 3].to_string(),
                        _ => t.text[it.s..it.e].to_string(),
                    },
                    None => String::new(),
                }
            }
        }
    }

    fn expr(&self, e: &E, ctx: Node, pos: usize, size: usize, budget: &mut u64) -> Result<Val, String> {
        tick(budget, 1)?;
        match e {
            E::Num(n) => Ok(Val::Num(*n)),
            E::Str(s) => Ok(Val::Str(s.clone())),
            E::Neg(x) => {
                let v = self.expr(x, ctx, pos, size, budget)?;
                Ok(Val::Num(-self.to_num(&v)))
            }
            E::Union(parts) => {
                let mut all: Vec<Node> = Vec::new();
                for p in parts {
                    match self.expr(p, ctx, pos, size, budget)? {
                        Val::Nodes(n) => all.extend(n),
                        _ => return Err("union of non-node-sets".into()),
                    }
                }
                Ok(Val::Nodes(self.normalize(all)))
            }
            E::Bin(op, l, r) => self.binary(op, l, r, ctx, pos, size, budget),
            E::Func(name, args) => self.func(name, args, ctx, pos, size, budget),
            E::Path { abs, start, preds, steps } => {
                let mut cur: Vec<Node> = if let Some(s) = start {
                    match self.expr(s, ctx, pos, size, budget)? {
                        Val::Nodes(n) => n,
                        _ => return Err("not a node-set".into()),
                    }
                } else if *abs {
                    vec![Node::Doc]
                } else {
                    vec![ctx]
                };
                for p in preds {
                    cur = self.filter(cur, p, budget)?;
                }
                for st in steps {
                    let mut next: Vec<Node> = Vec::new();
                    for n in &cur {
                        let mut cand = self.axis(&st.axis, *n, &st.test, budget)?;
                        for p in &st.preds {
                            cand = self.filter(cand, p, budget)?;
                        }
                        next.extend(cand);
                    }
                    cur = if cur.len() > 1 || matches!(st.axis, Axis::Parent | Axis::Ancestor | Axis::AncestorOrSelf) { self.normalize(next) } else { next };
                }
                Ok(Val::Nodes(cur))
            }
        }
    }

    fn normalize(&self, mut v: Vec<Node>) -> Vec<Node> {
        v.sort_by_key(|n| order_key(self.t, *n));
        v.dedup();
        v
    }

    fn filter(&self, nodes: Vec<Node>, pred: &E, budget: &mut u64) -> Result<Vec<Node>, String> {
        let size = nodes.len();
        let mut out = Vec::new();
        for (i, n) in nodes.into_iter().enumerate() {
            let v = self.expr(pred, n, i + 1, size, budget)?;
            let keep = match v {
                Val::Num(x) => (x - (i as f64 + 1.0)).abs() < f64::EPSILON,
                other => self.to_bool(&other),
            };
            if keep {
                out.push(n);
            }
        }
        Ok(out)
    }

    fn matches(&self, n: Node, test: &Test, principal_attr: bool) -> bool {
        let t = self.t;
        match (test, n) {
            (Test::AnyNode, _) => true,
            (Test::AnyName, Node::El(_)) => !principal_attr,
            (Test::AnyName, Node::At(..)) => principal_attr,
            (Test::Name(uri, local), Node::El(e)) if !principal_attr => {
                t.el_ns(e) == uri && local.as_deref().is_none_or(|l| t.local(e) == l)
            }
            (Test::Name(uri, local), Node::At(_, a)) if principal_attr => {
                let at = &t.attrs[a as usize];
                t.uri(at.ns) == uri && local.as_deref().is_none_or(|l| t.attr_local(at) == l)
            }
            (Test::Text, Node::Tx(e, i)) | (Test::Comment, Node::Tx(e, i)) | (Test::Pi, Node::Tx(e, i)) => {
                let items = if e == NONE { &t.prolog } else { &t.els[e as usize].items };
                matches!(
                    (test, items.get(i as usize).map(|x| x.kind)),
                    (Test::Text, Some(Kind::Text | Kind::Cdata)) | (Test::Comment, Some(Kind::Comment)) | (Test::Pi, Some(Kind::Pi))
                )
            }
            _ => false,
        }
    }

    fn children(&self, n: Node) -> Vec<Node> {
        let t = self.t;
        match n {
            Node::Doc => {
                let mut v: Vec<Node> = (0..t.prolog.len()).map(|i| Node::Tx(NONE, i as u32)).collect();
                v.push(Node::El(t.root));
                v
            }
            Node::El(e) => t.els[e as usize]
                .items
                .iter()
                .enumerate()
                .filter_map(|(i, it)| match it.kind {
                    Kind::Elem => Some(Node::El(it.el)),
                    Kind::Doctype => None,
                    _ => Some(Node::Tx(e, i as u32)),
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    fn parent(&self, n: Node) -> Option<Node> {
        match n {
            Node::Doc => None,
            Node::El(e) => {
                let p = self.t.els[e as usize].parent;
                Some(if p == NONE { Node::Doc } else { Node::El(p) })
            }
            Node::At(e, _) | Node::Tx(e, _) => Some(if e == NONE { Node::Doc } else { Node::El(e) }),
        }
    }

    fn descendants(&self, n: Node, out: &mut Vec<Node>, budget: &mut u64) -> Result<(), String> {
        // iterative preorder
        let mut stack: Vec<Node> = self.children(n).into_iter().rev().collect();
        while let Some(x) = stack.pop() {
            tick(budget, 1)?;
            out.push(x);
            if let Node::El(_) = x {
                stack.extend(self.children(x).into_iter().rev());
            }
        }
        Ok(())
    }

    fn axis(&self, axis: &Axis, n: Node, test: &Test, budget: &mut u64) -> Result<Vec<Node>, String> {
        let t = self.t;
        let attr_axis = matches!(axis, Axis::Attribute);
        let mut raw: Vec<Node> = Vec::new();
        match axis {
            Axis::Child => raw = self.children(n),
            Axis::Descendant => self.descendants(n, &mut raw, budget)?,
            Axis::DescendantOrSelf => {
                raw.push(n);
                self.descendants(n, &mut raw, budget)?;
            }
            Axis::SelfAx => raw.push(n),
            Axis::Parent => raw.extend(self.parent(n)),
            Axis::Ancestor | Axis::AncestorOrSelf => {
                if matches!(axis, Axis::AncestorOrSelf) {
                    raw.push(n);
                }
                let mut c = n;
                while let Some(p) = self.parent(c) {
                    raw.push(p);
                    c = p;
                }
                raw.reverse();
            }
            Axis::Attribute => {
                if let Node::El(e) = n {
                    let el = &t.els[e as usize];
                    for a in el.attr_lo..el.attr_hi {
                        if !t.is_ns_decl(&t.attrs[a as usize]) {
                            raw.push(Node::At(e, a));
                        }
                    }
                }
            }
            Axis::FollowingSibling | Axis::PrecedingSibling => {
                if let Some(p) = self.parent(n) {
                    if !matches!(n, Node::At(..)) {
                        let sibs = self.children(p);
                        if let Some(i) = sibs.iter().position(|x| *x == n) {
                            raw = if matches!(axis, Axis::FollowingSibling) { sibs[i + 1..].to_vec() } else { sibs[..i].iter().rev().copied().collect() };
                        }
                    }
                }
            }
            Axis::Following | Axis::Preceding => {
                let mut all = vec![Node::Doc];
                self.descendants(Node::Doc, &mut all, budget)?;
                let k = order_key(t, n);
                let anc: Vec<Node> = {
                    let mut v = Vec::new();
                    let mut c = n;
                    while let Some(p) = self.parent(c) {
                        v.push(p);
                        c = p;
                    }
                    v
                };
                if matches!(axis, Axis::Following) {
                    let end = match n {
                        Node::El(e) => t.els[e as usize].e1,
                        _ => 0,
                    };
                    raw = all.into_iter().filter(|x| !matches!(x, Node::Doc) && order_key(t, *x).0 >= end.max(k.0 + 1) && order_key(t, *x) > k).collect();
                } else {
                    raw = all.into_iter().filter(|x| order_key(t, *x) < k && !anc.contains(x) && !matches!(x, Node::Doc)).collect();
                    raw.reverse();
                }
            }
        }
        Ok(raw.into_iter().filter(|x| self.matches(*x, test, attr_axis)).collect())
    }

    fn compare(&self, op: &Op, a: &Val, b: &Val) -> bool {
        use Val::*;
        let cmp_num = |x: f64, y: f64| match op {
            Op::Eq => x == y,
            Op::Ne => x != y,
            Op::Lt => x < y,
            Op::Le => x <= y,
            Op::Gt => x > y,
            _ => x >= y,
        };
        let eq_like = matches!(op, Op::Eq | Op::Ne);
        match (a, b) {
            (Nodes(x), Nodes(y)) => {
                let ys: Vec<String> = y.iter().map(|n| self.node_str(*n)).collect();
                x.iter().any(|n| {
                    let s = self.node_str(*n);
                    ys.iter().any(|o| if eq_like { if matches!(op, Op::Eq) { &s == o } else { &s != o } } else { cmp_num(str_num(&s), str_num(o)) })
                })
            }
            (Nodes(x), other) | (other, Nodes(x)) => {
                let flipped = matches!(b, Nodes(_)) && !matches!(a, Nodes(_));
                x.iter().any(|n| {
                    let s = self.node_str(*n);
                    let (l, r): (Val, Val) = match other {
                        Num(_) => (Num(str_num(&s)), other.clone()),
                        Bool(_) => (Bool(!x.is_empty()), other.clone()),
                        _ => (Str(s), other.clone()),
                    };
                    let (l, r) = if flipped { (r, l) } else { (l, r) };
                    self.compare_atoms(op, &l, &r)
                })
            }
            _ => self.compare_atoms(op, a, b),
        }
    }

    fn compare_atoms(&self, op: &Op, a: &Val, b: &Val) -> bool {
        let eq_like = matches!(op, Op::Eq | Op::Ne);
        if eq_like {
            let r = if matches!(a, Val::Bool(_)) || matches!(b, Val::Bool(_)) {
                self.to_bool(a) == self.to_bool(b)
            } else if matches!(a, Val::Num(_)) || matches!(b, Val::Num(_)) {
                self.to_num(a) == self.to_num(b)
            } else {
                self.to_str(a) == self.to_str(b)
            };
            return if matches!(op, Op::Eq) { r } else { !r };
        }
        let (x, y) = (self.to_num(a), self.to_num(b));
        match op {
            Op::Lt => x < y,
            Op::Le => x <= y,
            Op::Gt => x > y,
            _ => x >= y,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn binary(&self, op: &Op, l: &E, r: &E, ctx: Node, pos: usize, size: usize, budget: &mut u64) -> Result<Val, String> {
        match op {
            Op::Or => {
                let a = self.expr(l, ctx, pos, size, budget)?;
                if self.to_bool(&a) {
                    return Ok(Val::Bool(true));
                }
                let b = self.expr(r, ctx, pos, size, budget)?;
                Ok(Val::Bool(self.to_bool(&b)))
            }
            Op::And => {
                let a = self.expr(l, ctx, pos, size, budget)?;
                if !self.to_bool(&a) {
                    return Ok(Val::Bool(false));
                }
                let b = self.expr(r, ctx, pos, size, budget)?;
                Ok(Val::Bool(self.to_bool(&b)))
            }
            Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge => {
                let a = self.expr(l, ctx, pos, size, budget)?;
                let b = self.expr(r, ctx, pos, size, budget)?;
                Ok(Val::Bool(self.compare(op, &a, &b)))
            }
            _ => {
                let a = self.expr(l, ctx, pos, size, budget)?;
                let b = self.expr(r, ctx, pos, size, budget)?;
                let (x, y) = (self.to_num(&a), self.to_num(&b));
                Ok(Val::Num(match op {
                    Op::Add => x + y,
                    Op::Sub => x - y,
                    Op::Mul => x * y,
                    Op::Div => x / y,
                    _ => x % y,
                }))
            }
        }
    }

    fn func(&self, name: &str, args: &[E], ctx: Node, pos: usize, size: usize, budget: &mut u64) -> Result<Val, String> {
        let mut ev = |i: usize| -> Result<Val, String> { self.expr(&args[i], ctx, pos, size, budget) };
        let need = |n: usize| -> Result<(), String> {
            if args.len() == n {
                Ok(())
            } else {
                Err(format!("{name}() takes {n} argument(s)"))
            }
        };
        let ctx_str = |me: &Self| me.node_str(ctx);
        Ok(match name {
            "last" => Val::Num(size as f64),
            "position" => Val::Num(pos as f64),
            "count" => {
                need(1)?;
                match ev(0)? {
                    Val::Nodes(n) => Val::Num(n.len() as f64),
                    _ => return Err("count() needs a node-set".into()),
                }
            }
            "true" => Val::Bool(true),
            "false" => Val::Bool(false),
            "not" => {
                need(1)?;
                let v = ev(0)?;
                Val::Bool(!self.to_bool(&v))
            }
            "boolean" => {
                need(1)?;
                let v = ev(0)?;
                Val::Bool(self.to_bool(&v))
            }
            "string" => {
                if args.is_empty() {
                    Val::Str(ctx_str(self))
                } else {
                    let v = ev(0)?;
                    Val::Str(self.to_str(&v))
                }
            }
            "number" => {
                if args.is_empty() {
                    Val::Num(str_num(&ctx_str(self)))
                } else {
                    let v = ev(0)?;
                    Val::Num(self.to_num(&v))
                }
            }
            "string-length" => {
                let s = if args.is_empty() {
                    ctx_str(self)
                } else {
                    let v = ev(0)?;
                    self.to_str(&v)
                };
                Val::Num(s.chars().count() as f64)
            }
            "normalize-space" => {
                let s = if args.is_empty() {
                    ctx_str(self)
                } else {
                    let v = ev(0)?;
                    self.to_str(&v)
                };
                Val::Str(s.split_whitespace().collect::<Vec<_>>().join(" "))
            }
            "concat" => {
                let mut s = String::new();
                for i in 0..args.len() {
                    let v = ev(i)?;
                    s.push_str(&self.to_str(&v));
                }
                Val::Str(s)
            }
            "contains" | "starts-with" | "substring-before" | "substring-after" => {
                need(2)?;
                let (a, b) = (ev(0)?, ev(1)?);
                let (a, b) = (self.to_str(&a), self.to_str(&b));
                match name {
                    "contains" => Val::Bool(a.contains(&b)),
                    "starts-with" => Val::Bool(a.starts_with(&b)),
                    "substring-before" => Val::Str(a.find(&b).map(|i| a[..i].to_string()).unwrap_or_default()),
                    _ => Val::Str(a.find(&b).map(|i| a[i + b.len()..].to_string()).unwrap_or_default()),
                }
            }
            "substring" => {
                if args.len() < 2 || args.len() > 3 {
                    return Err("substring() takes 2 or 3 arguments".into());
                }
                let s = ev(0)?;
                let s = self.to_str(&s);
                let st = ev(1)?;
                let st = round(self.to_num(&st));
                let len = if args.len() == 3 {
                    let l = ev(2)?;
                    round(self.to_num(&l))
                } else {
                    f64::INFINITY
                };
                let end = st + len;
                Val::Str(s.chars().enumerate().filter(|(i, _)| {
                    let p = *i as f64 + 1.0;
                    p >= st && p < end
                }).map(|(_, c)| c).collect())
            }
            "translate" => {
                need(3)?;
                let (a, b, c) = (ev(0)?, ev(1)?, ev(2)?);
                let (a, b, c): (String, Vec<char>, Vec<char>) = (self.to_str(&a), self.to_str(&b).chars().collect(), self.to_str(&c).chars().collect());
                Val::Str(a.chars().filter_map(|ch| match b.iter().position(|x| *x == ch) {
                    None => Some(ch),
                    Some(i) => c.get(i).copied(),
                }).collect())
            }
            "local-name" | "name" | "namespace-uri" => {
                let n = if args.is_empty() {
                    Some(ctx)
                } else {
                    match ev(0)? {
                        Val::Nodes(v) => v.first().copied(),
                        _ => return Err(format!("{name}() needs a node-set")),
                    }
                };
                let t = self.t;
                Val::Str(match n {
                    Some(Node::El(e)) => match name {
                        "local-name" => t.local(e).to_string(),
                        "name" => t.qname(e).to_string(),
                        _ => t.el_ns(e).to_string(),
                    },
                    Some(Node::At(_, a)) => {
                        let at = &t.attrs[a as usize];
                        match name {
                            "local-name" => t.attr_local(at).to_string(),
                            "name" => t.attr_qname(at).to_string(),
                            _ => t.uri(at.ns).to_string(),
                        }
                    }
                    _ => String::new(),
                })
            }
            "sum" => {
                need(1)?;
                match ev(0)? {
                    Val::Nodes(n) => Val::Num(n.iter().map(|x| str_num(&self.node_str(*x))).sum()),
                    _ => return Err("sum() needs a node-set".into()),
                }
            }
            "floor" | "ceiling" | "round" => {
                need(1)?;
                let v = ev(0)?;
                let x = self.to_num(&v);
                Val::Num(match name {
                    "floor" => x.floor(),
                    "ceiling" => x.ceil(),
                    _ => round(x),
                })
            }
            "lang" => {
                need(1)?;
                let want = ev(0)?;
                let want = self.to_str(&want).to_lowercase();
                let t = self.t;
                let mut cur = match ctx {
                    Node::El(e) => e,
                    Node::At(e, _) | Node::Tx(e, _) => e,
                    Node::Doc => NONE,
                };
                let mut found = false;
                while cur != NONE {
                    if let Some(v) = t.attr_value(cur, crate::tree::XML_NS, "lang") {
                        let v = v.to_lowercase();
                        found = v == want || v.starts_with(&format!("{want}-"));
                        break;
                    }
                    cur = t.els[cur as usize].parent;
                }
                Val::Bool(found)
            }
            other => return Err(format!("unsupported function {other}()")),
        })
    }
}

fn round(x: f64) -> f64 {
    if x.is_nan() || x.is_infinite() {
        x
    } else {
        (x + 0.5).floor()
    }
}

fn num_str(n: f64) -> String {
    if n.is_nan() {
        "NaN".into()
    } else if n.is_infinite() {
        (if n > 0.0 { "Infinity" } else { "-Infinity" }).into()
    } else if n == n.trunc() && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

fn str_num(s: &str) -> f64 {
    let t = s.trim();
    if t.is_empty() || t.starts_with('+') || t.contains(['e', 'E']) || t.eq_ignore_ascii_case("inf") || t.eq_ignore_ascii_case("infinity") || t.eq_ignore_ascii_case("nan") {
        return f64::NAN;
    }
    t.parse().unwrap_or(f64::NAN)
}

#[allow(dead_code)]
fn _ordering_unused(_: Ordering) {}

// ---------------------------------------------------------------- cost checks (the LocOS rules)

fn has_word(code: &str, words: &[&str]) -> bool {
    words.iter().any(|w| code.contains(w))
}

/// Refuses selector shapes that make evaluation quadratic or reach outside the selected subtree. The rules and the
/// messages are Localization OS's (`xml_its._check_xpath`).
pub(crate) fn check_cost(expr: &str, what: &str, pointer: bool, max_len: usize) -> Result<(), String> {
    if expr.is_empty() || expr.chars().count() > max_len {
        return Err(format!("{what}: XPath must be 1 to {max_len} characters."));
    }
    let mut depth = 0usize;
    let mut inner = String::new();
    let mut outer = String::new();
    let mut quote = '\0';
    for ch in expr.chars() {
        if quote != '\0' {
            if ch == quote {
                quote = '\0';
            }
            outer.push(' ');
            if depth > 0 {
                inner.push(' ');
            }
            continue;
        }
        outer.push(ch);
        if ch == '"' || ch == '\'' {
            quote = ch;
        } else if ch == '[' {
            depth += 1;
            if depth > 2 {
                return Err(format!("{what}: XPath predicates may nest at most two deep."));
            }
        } else if ch == ']' {
            depth = depth.saturating_sub(1);
        }
        if depth > 0 {
            inner.push(ch);
        } else {
            inner.push('\n');
        }
    }
    if has_word(&outer, &["following", "preceding"]) {
        return Err(format!("{what}: following and preceding axes are not allowed."));
    }
    if pointer && (outer.trim_start().starts_with('/') || has_word(&outer, &["//", "descendant"])) {
        return Err(format!("{what}: a pointer must be relative to the selected element, without '//' or a descendant axis."));
    }
    let deep = has_word(&inner, &["//", "descendant", "ancestor", "following", "preceding", ".."]) || parent_axis(&inner);
    if deep || absolute_in_predicate(&inner) {
        return Err(format!(
            "{what}: '//', '..', an absolute path and descendant/ancestor/parent/following/preceding axes are not allowed inside a predicate."
        ));
    }
    Ok(())
}

/// `parent\s*::`
fn parent_axis(s: &str) -> bool {
    let mut rest = s;
    while let Some(i) = rest.find("parent") {
        let after = rest[i + 6..].trim_start();
        if after.starts_with("::") {
            return true;
        }
        rest = &rest[i + 6..];
    }
    false
}

fn absolute_in_predicate(s: &str) -> bool {
    let cs: Vec<char> = s.chars().collect();
    for (i, c) in cs.iter().enumerate() {
        if *c == '/' {
            if i == 0 {
                return true;
            }
            let p = cs[i - 1];
            if p.is_whitespace() || "[(,=<>|+".contains(p) {
                return true;
            }
        }
    }
    false
}

//! Structure fuzz (LocOS trackers #788 and #790): random translations drawn from each format's special characters,
//! merged and read again. For every case:
//!
//! * S1: the rebuilt file has the same units, with the same kind, style and translate flag, in order;
//! * S2: the skeleton outside the units is unchanged: for a text skeleton the text between the units, else the
//!   rebuilt file with its units set back to the source text, compared with the file a plain translation and back
//!   gives;
//! * S3: each unit reads back as the translation that was set (the escape the writer adds undone, codes expanded to
//!   their markup or compared by kind, blanks removed);
//!
//! or the merge refuses with an error (a clean refusal). proptest with a fixed seed, so a run is repeatable; a
//! failure is shrunk to a small translation and printed.
//!
//! Native Markdown keeps unit text as Markdown source, so a line break, an image or a link (`](`) in a translation
//! is new markup by design: its alphabet has none of them. The alphabets hold multi-character atoms (`import `,
//! ```` ``` ````, `::`) so the block starts the trackers named come up.

use bilingua_model::*;
use proptest::prelude::*;
use proptest::test_runner::{Config, RngAlgorithm, TestCaseError, TestError, TestRng, TestRunner};
use std::cell::Cell;

const LETTERS: &[&str] = &["a", "b", "x", "z", " ", "A", "w"];

#[derive(Clone, Copy, PartialEq)]
enum Norm {
    /// MDX and the LocOS Markdown profile: backslash escapes and character references undone.
    Md,
    /// Native Markdown: unit text is Markdown source on both sides; only the backslashes the writer adds before a
    /// block marker or a cell `|` are dropped (all backslashes, after the codes are expanded).
    MdRaw,
    /// AsciiDoc: `{empty}`, `{two-colons}`, `{two-semicolons}` and cell escapes undone.
    Adoc,
    /// Markup and code formats whose units hold decoded text.
    Plain,
    /// MediaWiki: `<nowiki>x</nowiki>` read as `x`.
    Wiki,
    /// Server pages: an expression start quoted with one backslash (`\${`, `\#{`) or `&#92;` read as written.
    Jsp,
}

impl Norm {
    /// Markdown, MDX and AsciiDoc codes hold the markup a translation may also write as text (`` `x` ``, `*b*`), so
    /// codes are compared by their markup; elsewhere a pair's markup can hold another unit (an `alt` or `title`
    /// attribute), so pairs are compared by kind only.
    fn by_markup(self) -> bool {
        matches!(self, Norm::Md | Norm::MdRaw | Norm::Adoc)
    }
}

struct Case {
    name: &'static str,
    profile: Option<&'static str>,
    src: &'static str,
    atoms: &'static [&'static str],
    norm: Norm,
    /// S2 compares with blanks removed (writers that fold or trim blanks around a unit).
    loose: bool,
    /// S2 on the skeleton text between the units (the format keeps its skeleton as text parts around the units);
    /// otherwise S2 rebuilds with the source text and compares with the canonical file.
    raw_s2: bool,
    cases: u32,
}

fn opts(c: &Case) -> ExtractOptions {
    let o = ExtractOptions::default();
    match c.profile {
        Some(p) => o.with_profile(p),
        None => o,
    }
}

fn filter(c: &Case) -> Box<dyn Filter> {
    bilingua::detect_with(c.name, c.src.as_bytes(), c.profile).unwrap_or_else(|| panic!("no filter for {}", c.name))
}

/// The text of `inl` for comparing: each text run with the writer's escapes undone, every code written as its
/// markup (the unit's own codes), or a pair by kind (see `Norm::by_markup`); then blanks removed.
fn expand(n: Norm, u: &Unit, inl: &[Inline]) -> String {
    let mut s = String::new();
    for i in inl {
        match i {
            Inline::Text(t) => s.push_str(&norm_text(n, t)),
            // a pair by kind; a standalone code by its markup (a translated `{a}` is read as a placeholder code)
            Inline::Tag { kind: TagKind::Open, .. } if !n.by_markup() => s.push_str("\u{27E6}o\u{27E7}"),
            Inline::Tag { kind: TagKind::Close, .. } if !n.by_markup() => s.push_str("\u{27E6}c\u{27E7}"),
            Inline::Tag { id, kind, .. } => {
                if let Some(c) = u.codes.get(id) {
                    let piece = match kind {
                        TagKind::Open => c.open.as_deref(),
                        TagKind::Close => c.close.as_deref(),
                        TagKind::Standalone => c.standalone.as_deref(),
                    };
                    let piece = piece.unwrap_or_default();
                    // the AsciiDoc writer may double a constrained mark next to punctuation (`}*p*` to `}**p**`)
                    if n == Norm::Adoc && piece.len() == 2 && piece.as_bytes()[0] == piece.as_bytes()[1] {
                        s.push_str(&piece[..1]);
                    } else {
                        s.push_str(piece);
                    }
                }
            }
        }
    }
    let s = no_marks(&s);
    let s = match n {
        // a code's markup can be the escape itself (`<nowiki>|</nowiki>`)
        Norm::Wiki => s.replace("<nowiki>", "").replace("</nowiki>", ""),
        // an attribute reference or escape the writer adds can be read back as a code
        Norm::Adoc => unref(&s.replace("{empty}", "").replace("{two-colons}", "::").replace("{two-semicolons}", ";;").replace("\\|", "|").replace("\\!", "!")),
        // the emphasis repair may write `_` as `*`, move edge punctuation over a mark, or write the pair as HTML
        Norm::MdRaw => s
            .replace('\\', "")
            .replace("<strong>", "**")
            .replace("</strong>", "**")
            .replace("<em>", "*")
            .replace("</em>", "*")
            .replace('_', "*"),
        _ => s,
    };
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if n == Norm::MdRaw { sort_punct_runs(&s) } else { s }
}

/// Each run of non-alphanumeric characters sorted: punctuation moved across an emphasis mark compares equal, while
/// a mark or punctuation added, lost or moved past a letter still differs.
fn sort_punct_runs(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut run: Vec<char> = Vec::new();
    for c in s.chars() {
        if c.is_alphanumeric() {
            run.sort_unstable();
            out.extend(run.drain(..));
            out.push(c);
        } else {
            run.push(c);
        }
    }
    run.sort_unstable();
    out.extend(run);
    out
}

/// Well-formed numeric and the five XML character references.
fn unref(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(a) = rest.find('&') {
        out.push_str(&rest[..a]);
        let tail = &rest[a..];
        let semi = tail[1..].find(';').map(|k| k + 1).filter(|&k| k <= 10);
        let decoded = semi.and_then(|k| {
            let name = &tail[1..k];
            let num = |n: &str, radix| u32::from_str_radix(n, radix).ok().and_then(char::from_u32);
            match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ if name.starts_with("#x") || name.starts_with("#X") => num(&name[2..], 16),
                _ if name.starts_with('#') => num(&name[1..], 10),
                _ => None,
            }
            .map(|c| (c, k))
        });
        match decoded {
            Some((c, k)) => {
                out.push(c);
                rest = &tail[k + 1..];
            }
            None => {
                out.push('&');
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn md_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\\' && it.peek().is_some_and(|n| n.is_ascii_punctuation()) {
            out.push(it.next().unwrap_or_default());
        } else {
            out.push(c);
        }
    }
    out
}

/// Private-use markers (Markdown alt and title slots) and what sits between them.
fn no_marks(s: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for c in s.chars() {
        match c {
            '\u{E000}' => inside = true,
            '\u{E001}' => inside = false,
            _ if !inside => out.push(c),
            _ => {}
        }
    }
    out
}

/// One text run with the escape its writer adds undone.
fn norm_text(n: Norm, s: &str) -> String {
    match n {
        Norm::Md => unref(&md_unescape(s)),
        Norm::MdRaw => s.to_string(),
        Norm::Adoc => {
            let s = s.replace("{empty}", "").replace("{two-colons}", "::").replace("{two-semicolons}", ";;");
            unref(&s.replace("\\|", "|").replace("\\!", "!"))
        }
        Norm::Plain => unref(s),
        Norm::Wiki => unref(&s.replace("<nowiki>", "").replace("</nowiki>", "")),
        Norm::Jsp => unref(&s.replace("\\${", "${").replace("\\#{", "#{")),
    }
}

fn plain_of(u: &Unit) -> String {
    plain_text(&u.source())
}

/// One item of a unit for the structure check: a character, or a code of a kind.
#[derive(Clone, Copy, PartialEq)]
enum Tok {
    Ch(char),
    Code(TagKind),
}

fn chars_of(n: Norm, t: &str) -> Vec<Tok> {
    norm_text(n, t).chars().filter(|c| !c.is_whitespace()).map(Tok::Ch).collect()
}

/// S4 (AsciiDoc): the codes of the translation are still codes after the file is read again, and the text around
/// them is the text that was set. A mark the reader pairs into a code of its own (a lone `*` in a translation with
/// a code's `**` further on) is a code that was not set, so the line formats differently than the translator typed.
/// A read code may stand for its own markup as text only when the translation had that text, so typed markup such
/// as `_a_` (text on one side, a pair on the other) passes.
fn same_structure(n: Norm, u: &Unit, set_t: &[Inline], read: &[Inline]) -> bool {
    let mut want: Vec<Tok> = Vec::new();
    for i in set_t {
        match i {
            Inline::Text(t) => want.extend(chars_of(n, t)),
            Inline::Tag { kind, .. } => want.push(Tok::Code(*kind)),
        }
    }
    // reach[j]: the items read so far can stand for the first j wanted tokens
    let mut reach = vec![false; want.len() + 1];
    reach[0] = true;
    for i in read {
        let alts: Vec<Vec<Tok>> = match i {
            Inline::Text(t) => vec![chars_of(n, t)],
            Inline::Tag { kind, .. } => {
                let m = markup(u, i).unwrap_or_default();
                let m = if m.len() == 2 && m.as_bytes()[0] == m.as_bytes()[1] { &m[..1] } else { m };
                vec![vec![Tok::Code(*kind)], chars_of(n, m)]
            }
        };
        let mut next = vec![false; want.len() + 1];
        for j in (0..reach.len()).filter(|&j| reach[j]) {
            for a in &alts {
                if want.len() - j >= a.len() && want[j..j + a.len()] == a[..] {
                    next[j + a.len()] = true;
                }
            }
        }
        reach = next;
    }
    reach[want.len()]
}

/// A Markdown table row unit (MDX: one unit per row, cells split by `|`).
fn is_row(u: &Unit) -> bool {
    let t = plain_of(u);
    let t = t.trim();
    t.len() > 1 && t.starts_with('|') && t.ends_with('|')
}

fn translatable(u: &Unit) -> bool {
    u.translate && u.segments.len() == 1
}

/// The target for `u`: random pieces between the unit's own codes, in order, plus one plain word (a symbol-only
/// string is not a unit by design). A table row keeps its cells and gets no codes.
fn target_for(u: &Unit, front: bool, pieces: &[String]) -> Vec<Inline> {
    if is_row(u) {
        let cells: Vec<String> = pieces.iter().map(|p| format!(" w{} ", p.replace('|', ""))).collect();
        return vec![Inline::text(format!("|{}|", cells.join("|")))];
    }
    let tags: Vec<Inline> = u.source().into_iter().filter(|i| matches!(i, Inline::Tag { .. })).collect();
    let n = tags.len();
    let mut texts: Vec<String> = (0..=n).map(|k| pieces.get(k).cloned().unwrap_or_default()).collect();
    if front {
        texts[0] = format!("w {}", texts[0]);
    } else {
        texts[n].push_str(" w");
    }
    let mut out: Vec<Inline> = Vec::new();
    for (k, t) in texts.into_iter().enumerate() {
        if !t.is_empty() {
            out.push(Inline::text(t));
        }
        if let Some(tag) = tags.get(k) {
            out.push(tag.clone());
            // a pair around nothing (`****`) is text in most formats: keep one letter inside
            if matches!(tag, Inline::Tag { kind: TagKind::Open, .. }) {
                out.push(Inline::text("p"));
            }
        }
    }
    out
}

fn set(doc: &mut Document, i: usize, t: Vec<Inline>) {
    let seg = &mut doc.units[i].segments[0];
    seg.target = t;
    seg.state = SegState::Draft;
}

/// The markup of a tag in its unit.
fn markup<'a>(u: &'a Unit, i: &Inline) -> Option<&'a str> {
    let Inline::Tag { id, kind, .. } = i else { return None };
    let c = u.codes.get(id)?;
    match kind {
        TagKind::Open => c.open.as_deref(),
        TagKind::Close => c.close.as_deref(),
        TagKind::Standalone => c.standalone.as_deref(),
    }
}

fn kind_of(i: &Inline) -> Option<TagKind> {
    match i {
        Inline::Tag { kind, .. } => Some(*kind),
        Inline::Text(_) => None,
    }
}

/// `doc2` (a re-read build) with every unit set back to the text of the same unit of `orig`. Each code of `orig` is
/// the next code of the re-read unit with the same kind and markup (the writer may have added codes of its own, an
/// escape the reader keeps as a code), else the next one of the same kind (its markup can hold a translated
/// attribute).
fn back_to_source(orig: &Document, doc2: &mut Document) {
    for i in 0..orig.units.len() {
        if !translatable(&orig.units[i]) || !translatable(&doc2.units[i]) {
            continue;
        }
        let (ou, nu) = (&orig.units[i], &doc2.units[i]);
        let theirs: Vec<Inline> = nu.source().into_iter().filter(|x| kind_of(x).is_some()).collect();
        let mut k = 0;
        let mut t = Vec::new();
        for inl in ou.source() {
            if kind_of(&inl).is_none() {
                t.push(inl);
                continue;
            }
            let m = markup(ou, &inl);
            let same = |x: &Inline| kind_of(x) == kind_of(&inl);
            let hit = theirs[k..].iter().position(|x| same(x) && markup(nu, x) == m).or_else(|| theirs[k..].iter().position(same));
            match hit {
                Some(j) => {
                    t.push(theirs[k + j].clone());
                    k += j + 1;
                }
                None => t.push(Inline::text(format!("[lost code {}]", m.unwrap_or_default()))),
            }
        }
        set(doc2, i, t);
    }
}

/// The skeleton text between units (Raw parts joined), for a text skeleton.
fn raw_parts(doc: &Document) -> Option<Vec<String>> {
    let Skeleton::Text { parts, .. } = &doc.skeleton else { return None };
    let mut out = vec![String::new()];
    for p in parts {
        match p {
            SkelPart::Raw(r) => {
                if let Some(s) = out.last_mut() {
                    s.push_str(r);
                }
            }
            SkelPart::Unit(_) => out.push(String::new()),
        }
    }
    Some(out)
}

/// Blanks removed, and backslashes: a reader may leave an escape the writer added before a unit in the skeleton.
fn squash(b: &[u8]) -> Vec<u8> {
    b.iter().copied().filter(|c| !c.is_ascii_whitespace() && *c != b'\\').collect()
}

/// The source after one plain translation and back: the form every rebuild must reach.
fn canon(c: &Case, f: &dyn Filter, doc: &Document) -> Vec<u8> {
    let mut d = doc.clone();
    for i in 0..d.units.len() {
        if !translatable(&d.units[i]) {
            continue;
        }
        let tags = d.units[i].source().into_iter().filter(|x| matches!(x, Inline::Tag { .. }));
        let mut t = vec![Inline::text("Plain ")];
        for tag in tags {
            let open = matches!(tag, Inline::Tag { kind: TagKind::Open, .. });
            t.push(tag);
            t.push(Inline::text(if open { "p" } else { " " }));
        }
        t.push(Inline::text(" words"));
        set(&mut d, i, t);
    }
    let plain = f.merge(&d).expect("plain translation merges");
    let mut d2 = f.extract(&plain, &opts(c)).expect("plain translation reads");
    assert_eq!(d2.units.len(), doc.units.len(), "{}: a plain translation keeps the units", c.name);
    back_to_source(doc, &mut d2);
    f.merge(&d2).expect("plain translation and back merges")
}

fn check(c: &Case, f: &dyn Filter, doc: &Document, want: &[u8], plan: &[(bool, Vec<String>)], refused: &Cell<u32>) -> Result<(), String> {
    let mut d = doc.clone();
    for (i, (front, pieces)) in plan.iter().enumerate() {
        if translatable(&doc.units[i]) {
            let t = target_for(&doc.units[i], *front, pieces);
            set(&mut d, i, t);
        }
    }
    let out = match f.merge(&d) {
        Ok(o) => o,
        Err(_) => {
            refused.set(refused.get() + 1);
            return Ok(());
        }
    };
    let shown = || String::from_utf8_lossy(&out).into_owned();
    let mut d2 = f.extract(&out, &opts(c)).map_err(|e| format!("export not readable: {e}\n{}", shown()))?;
    if d2.units.len() != doc.units.len() {
        let got: Vec<String> = d2.units.iter().map(plain_of).collect();
        return Err(format!("S1 unit count {} -> {}: {got:?}\n{}", doc.units.len(), d2.units.len(), shown()));
    }
    for (i, (a, b)) in doc.units.iter().zip(&d2.units).enumerate() {
        if a.context.kind != b.context.kind || a.context.style != b.context.style || a.translate != b.translate {
            return Err(format!("S1 unit {i} kind {:?}/{:?} -> {:?}/{:?}\n{}", a.context.kind, a.context.style, b.context.kind, b.context.style, shown()));
        }
    }
    for i in 0..doc.units.len() {
        if !translatable(&doc.units[i]) {
            continue;
        }
        let set_t = expand(c.norm, &doc.units[i], &d.units[i].segments[0].target);
        let got = expand(c.norm, &d2.units[i], &d2.units[i].source());
        if set_t != got {
            return Err(format!("S3 unit {i}: set {set_t:?} read {got:?}\n{}", shown()));
        }
        if c.norm == Norm::Adoc && !same_structure(c.norm, &d2.units[i], &d.units[i].segments[0].target, &d2.units[i].source()) {
            return Err(format!("S4 unit {i}: the codes set are not the codes read: set {set_t:?} read {got:?}\n{}", shown()));
        }
    }
    // a text skeleton: the bytes between the units are compared as they are
    if let (true, Some(a), Some(b)) = (c.raw_s2, raw_parts(doc), raw_parts(&d2)) {
        let same = if c.loose { a.iter().map(|x| squash(x.as_bytes())).eq(b.iter().map(|x| squash(x.as_bytes()))) } else { a == b };
        if !same {
            let diff: Vec<(String, String)> = a.into_iter().zip(b).filter(|(x, y)| x != y).collect();
            return Err(format!("S2 skeleton changed between the units: {diff:?}\n{}", shown()));
        }
        return Ok(());
    }
    back_to_source(doc, &mut d2);
    let back = f.merge(&d2).map_err(|e| format!("rebuild with the source refused: {e}"))?;
    let same = if c.loose { squash(&back) == squash(want) } else { back == want };
    if !same {
        return Err(format!("S2 skeleton changed: rebuilt with the source\n{}\nwant\n{}", String::from_utf8_lossy(&back), String::from_utf8_lossy(want)));
    }
    Ok(())
}

fn run(c: &Case) {
    let f = filter(c);
    let doc = f.extract(c.src.as_bytes(), &opts(c)).unwrap_or_else(|e| panic!("{}: {e}", c.name));
    assert!(doc.units.iter().filter(|u| translatable(u)).count() >= 3, "{}: the source needs units", c.name);
    let want = canon(c, f.as_ref(), &doc);
    let mut alphabet: Vec<&'static str> = c.atoms.to_vec();
    alphabet.extend_from_slice(LETTERS);
    let piece = prop::collection::vec(prop::sample::select(alphabet), 0..8).prop_map(|v| v.concat());
    let per_unit: Vec<BoxedStrategy<(bool, Vec<String>)>> = doc
        .units
        .iter()
        .map(|u| {
            let n = if is_row(u) { plain_of(u).trim().matches('|').count() - 1 } else { u.source().iter().filter(|i| matches!(i, Inline::Tag { .. })).count() + 1 };
            (any::<bool>(), prop::collection::vec(piece.clone(), n..=n)).boxed()
        })
        .collect();
    let config = Config { cases: c.cases, failure_persistence: None, max_shrink_iters: 4000, ..Config::default() };
    let mut runner = TestRunner::new_with_rng(config, TestRng::from_seed(RngAlgorithm::ChaCha, &[0x79; 32]));
    let refused = Cell::new(0u32);
    let res = runner.run(&per_unit, |plan| check(c, f.as_ref(), &doc, &want, &plan, &refused).map_err(TestCaseError::fail));
    match res {
        Ok(()) => {}
        Err(TestError::Fail(why, plan)) => {
            let shown: Vec<String> = plan.iter().enumerate().filter(|(i, (_, p))| translatable(&doc.units[*i]) && p.iter().any(|s| !s.is_empty())).map(|(i, (front, p))| format!("unit {i} front={front} {p:?}")).collect();
            panic!("{} {:?}: {why}\nminimal plan: {shown:#?}", c.name, c.profile);
        }
        Err(e) => panic!("{}: {e}", c.name),
    }
    assert!(refused.get() * 2 < c.cases, "{}: {} of {} cases refused", c.name, refused.get(), c.cases);
}

const MDX_SRC: &str = "import { Badge } from './badge'

# Getting started

Hello <Badge>new</Badge> world and {count} items.

<Note>

Inside a component with `code`.

</Note>

| Col A | Col B |
|---|---|
| cell one | cell two |

- item one
- item two

> quoted text here

Last paragraph.
";

const MDX_ATOMS: &[&str] = &["{", "}", "<", ">", "\\", "`", "``", "```", "~~~", "\n", "|", "import ", "export ", "&", "#", "- ", "1. "];

#[test]
fn mdx() {
    run(&Case { name: "x.mdx", profile: None, src: MDX_SRC, atoms: MDX_ATOMS, norm: Norm::Md, loose: true, raw_s2: true, cases: 5000 });
}

const MD_SRC: &str = "# Getting started

Hello *world* and `code` here.

Second paragraph with a [link](https://example.org/a) in it.

| Col A | Col B |
|---|---|
| cell one | cell two |

- item one
- item **two**

> quoted text here

Last paragraph.
";

const MD_ATOMS: &[&str] = &["`", "``", "```", "~~~", "*", "_", "#", "# ", "- ", "+ ", "1. ", "2) ", "> ", "[", "]", "\\", "|", "<", ">", "&", "=", "---"];
const MD_LOCOS_ATOMS: &[&str] = &["`", "``", "```", "~~~", "*", "_", "#", "# ", "- ", "+ ", "1. ", "> ", "[", "]", "\\", "&", "\n"];

#[test]
fn markdown_native() {
    run(&Case { name: "x.md", profile: None, src: MD_SRC, atoms: MD_ATOMS, norm: Norm::MdRaw, loose: true, raw_s2: true, cases: 5000 });
}

#[test]
fn markdown_locos() {
    run(&Case { name: "x.md", profile: Some("locos"), src: MD_SRC, atoms: MD_LOCOS_ATOMS, norm: Norm::Md, loose: true, raw_s2: false, cases: 5000 });
}

const ADOC_SRC: &str = "= Document Title
:attr: value

Intro paragraph text.

== Section heading

A paragraph with *strong* words.

[cols=\"1,1\"]
|===
|Cell one |Cell two
|Cell three |Cell four
|===

* List item one
* List item two

Term one:: Its description.

NOTE: Admonition text.
";

// lone inline marks (`*`, `_`, backtick, `#`, `^`, `~`) are in the alphabet: one in a translation must not pair with a
// code's mark, next to it or further away (`*}` + `**p**`; check S4)
const ADOC_ATOMS: &[&str] = &["|", "!", "{", "}", ":", "::", ":::", ";;", "/", "//", "\n", "* ", "[", "]", "=", "== ", ". ", "- ", "*", "_", "`", "#", "^", "~"];

#[test]
fn adoc() {
    run(&Case { name: "x.adoc", profile: None, src: ADOC_SRC, atoms: ADOC_ATOMS, norm: Norm::Adoc, loose: true, raw_s2: true, cases: 5000 });
}

const HTML_SRC: &str = "<!doctype html>
<html><head><title>Page title</title></head><body>
<h1>Welcome here</h1>
<p>Hello <b>bold</b> world and more.</p>
<p>Plain &amp; paragraph</p>
<ul><li>Item one</li><li>Item two</li></ul>
<script>var a = 1;</script>
<p><a href=\"x\" title=\"Tip text\">Link text</a></p>
</body></html>
";

const HTML_ATOMS: &[&str] = &["<", ">", "&", ";", "\"", "'", "&amp;", "<b>", "</p>", "<!--", "-->", "</script>", "\n", "=", "/"];

#[test]
fn html() {
    run(&Case { name: "x.html", profile: None, src: HTML_SRC, atoms: HTML_ATOMS, norm: Norm::Plain, loose: true, raw_s2: false, cases: 3000 });
}

const WIKI_SRC: &str = "== Section heading ==

Hello '''bold''' world and more text.

* Item one
* Item two

A paragraph with a [[Target|link]] inside.

Last paragraph here.
";

const WIKI_ATOMS: &[&str] = &["[[", "]]", "{{", "}}", "'''", "''", "<", ">", "|", "=", "==", "\n", "*", "#", "&", "~~~~", "----", ":", ";"];

#[test]
fn wiki() {
    run(&Case { name: "x.wiki", profile: None, src: WIKI_SRC, atoms: WIKI_ATOMS, norm: Norm::Wiki, loose: true, raw_s2: false, cases: 3000 });
}

const JS_SRC: &str = "// header
export default {
  a: 'Hello world',
  b: \"Double text\",
  c: `Template text`,
  nested: { d: 'Nested one' },
  list: ['One', 'Two'],
}
";

const JS_ATOMS: &[&str] = &["/*", "*/", "`", "${", "}", "'", "\"", "\\", "\n", "//", "\u{2028}", "</script>"];

#[test]
fn jsmodule() {
    run(&Case { name: "x.js", profile: None, src: JS_SRC, atoms: JS_ATOMS, norm: Norm::Plain, loose: false, raw_s2: false, cases: 3000 });
}

const PHP_SRC: &str = "<?php
// comment
return [
    'a' => 'Hello world',
    'b' => \"Double quoted text\",
    'nested' => ['c' => 'Nested one', 'list' => ['One', 'Two']],
];
";

const PHP_ATOMS: &[&str] = &["$", "{", "}", "${", "{$", "\\", "'", "\"", "\r", "\n", "?>", "<?php", "\\n"];

#[test]
fn php() {
    run(&Case { name: "x.php", profile: None, src: PHP_SRC, atoms: PHP_ATOMS, norm: Norm::Plain, loose: false, raw_s2: false, cases: 3000 });
}

const JSP_SRC: &str = "<%@ page contentType=\"text/html; charset=UTF-8\" %>
<html><body>
<h1>Welcome here</h1>
<p>Hello ${user.name} and welcome</p>
<p>#{greeting} friend of ours</p>
<% out.println(\"x\"); %>
<p>Plain &amp; paragraph</p>
</body></html>
";

const JSP_ATOMS: &[&str] = &["$", "#", "{", "}", "${", "#{", "\\", "'", "\"", "<", ">", "&", "<%", "%>", "<%=", "<jsp:"];

#[test]
fn serverpages() {
    run(&Case { name: "x.jsp", profile: None, src: JSP_SRC, atoms: JSP_ATOMS, norm: Norm::Jsp, loose: true, raw_s2: false, cases: 3000 });
}

const SVG_SRC: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"50\">
<title>Chart title</title>
<desc>A description</desc>
<text x=\"1\" y=\"2\">Hello world</text>
<text><tspan>Span text</tspan> tail text</text>
<g><text>Group &amp; text</text></g>
</svg>
";

const SVG_ATOMS: &[&str] = &["<", ">", "&", ";", ":", "'", "\"", "]]>", "<![CDATA[", "&#", "\n"];

#[test]
fn svg() {
    run(&Case { name: "x.svg", profile: None, src: SVG_SRC, atoms: SVG_ATOMS, norm: Norm::Plain, loose: true, raw_s2: false, cases: 3000 });
}

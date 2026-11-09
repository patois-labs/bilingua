//! Writing the target language into a package on a target-locale merge, as Localization OS does:
//! `fo:language` / `fo:country` / `fo:script` (or the `-asian` / `-complex` slot for East Asian and complex-script
//! targets) in the default styles and in every style that already declares a language, `dc:language` in meta.xml,
//! and for a right-to-left target `style:writing-mode="rl-tb"` on the default paragraph and graphic styles, on
//! every explicit horizontal left-to-right style, and on every sheet of a spreadsheet.

use crate::edit::Edits;
use crate::lang::{self, Slot};
use crate::xml::*;

pub struct Lang {
    names: [(u16, &'static str); 3],
    values: [Option<String>; 3],
    pub rtl: bool,
}

impl Lang {
    pub fn new(tag: &str) -> Lang {
        let (l, c, s) = lang::parts(tag);
        let names = match lang::slot(tag) {
            Slot::EastAsia => [(NS_STYLE, "language-asian"), (NS_STYLE, "country-asian"), (NS_STYLE, "script-asian")],
            Slot::Complex => [(NS_STYLE, "language-complex"), (NS_STYLE, "country-complex"), (NS_STYLE, "script-complex")],
            Slot::Latin => [(NS_FO, "language"), (NS_FO, "country"), (NS_FO, "script")],
        };
        Lang { names, values: [Some(l), c, s], rtl: lang::is_rtl(tag) }
    }
}

fn default_prefix(ns: u16) -> &'static str {
    match ns {
        NS_TEXT => "text",
        NS_TABLE => "table",
        NS_OFFICE => "office",
        NS_STYLE => "style",
        NS_FO => "fo",
        NS_META => "meta",
        NS_DC => "dc",
        _ => "ns",
    }
}

/// The first `nsN` (N from 0) that is not bound at `node` and not in `taken`: the prefix lxml gives a namespace the
/// document has no prefix for.
fn lxml_prefix(t: &Tree, node: u32, taken: impl Fn(&str) -> bool) -> String {
    (0..).map(|k| format!("ns{k}")).find(|p| !t.prefix_bound(node, p) && !taken(p)).expect("endless range")
}

/// A new element as markup, with prefixes taken from the scope of `ctx` and a local `xmlns` declaration for any
/// namespace the document does not bind there.
pub fn make_el(t: &Tree, ctx: u32, ns: u16, local: &str, attrs: &[(u16, &str, String)], inner: &str, lxml: bool) -> String {
    let mut decls: Vec<(String, u16)> = Vec::new();
    let mut pf = |ns: u16| -> String {
        if let Some((p, _)) = decls.iter().find(|(_, id)| *id == ns) {
            return p.clone();
        }
        match t.find_prefix(ctx, ns) {
            Some(p) => p,
            None => {
                // the usual prefix, unless the document binds it to another namespace here (then the new name would
                // change what its attributes mean) or the output is to read like lxml's
                let p = if lxml || t.prefix_bound(ctx, default_prefix(ns)) {
                    lxml_prefix(t, ctx, |q| decls.iter().any(|(d, _)| d == q))
                } else {
                    default_prefix(ns).to_string()
                };
                if !decls.iter().any(|(q, _)| *q == p) {
                    decls.push((p.clone(), ns));
                }
                p
            }
        }
    };
    let name = format!("{}:{local}", pf(ns));
    let mut a = String::new();
    for (ans, al, v) in attrs {
        a.push_str(&format!(" {}:{al}=\"{}\"", pf(*ans), escape_attr(v, b'"')));
    }
    let d: String = decls.iter().map(|(p, id)| format!(" xmlns:{p}=\"{}\"", t.ns_uri(*id))).collect();
    if inner.is_empty() {
        format!("<{name}{d}{a}/>")
    } else {
        format!("<{name}{d}{a}>{inner}</{name}>")
    }
}

/// The qualified name for an attribute on `node`, declaring its namespace on the element when it has no prefix.
fn attr_q(t: &Tree, ed: &mut Edits, node: u32, ns: u16, local: &str) -> String {
    if let Some(q) = t.attr_qname(node, ns, local) {
        return q.to_string();
    }
    // a declaration this edit already put on the start tag comes first (nearest scope)
    let tag = ed.start_of(t, node).to_string();
    let uri = t.ns_uri(ns);
    for a in scan_attrs(&tag) {
        let q = &tag[a.name_start..a.name_end];
        if let Some(p) = q.strip_prefix("xmlns:") {
            if unescape(&tag[a.val_start..a.val_end]) == uri {
                return format!("{p}:{local}");
            }
        }
    }
    match t.find_prefix(node, ns) {
        Some(p) => format!("{p}:{local}"),
        None => {
            let p = if ed.lxml_prefixes || t.prefix_bound(node, default_prefix(ns)) {
                lxml_prefix(t, node, |q| scan_attrs(&tag).iter().any(|a| &tag[a.name_start..a.name_end] == format!("xmlns:{q}")))
            } else {
                default_prefix(ns).to_string()
            };
            ed.set_attr(t, node, &format!("xmlns:{p}"), Some(uri));
            format!("{p}:{local}")
        }
    }
}

fn set_attr(t: &Tree, ed: &mut Edits, node: u32, ns: u16, local: &str, value: Option<&str>) {
    if value.is_none() && t.attr_qname(node, ns, local).is_none() {
        return;
    }
    let q = attr_q(t, ed, node, ns, local);
    ed.set_attr(t, node, &q, value);
}

fn set_lang(t: &Tree, ed: &mut Edits, props: u32, l: &Lang) {
    for (i, (ns, local)) in l.names.iter().enumerate() {
        set_attr(t, ed, props, *ns, local, l.values[i].as_deref());
    }
}

fn child_el(t: &Tree, n: u32, ns: u16, local: &str) -> Option<u32> {
    t.first_elem_child(n, ns, local)
}

/// Index of `child` among the non-text children of `parent`.
fn node_index(t: &Tree, parent: u32, child: u32) -> usize {
    t.children(parent).filter(|c| !matches!(t.node(*c).kind, Kind::Text | Kind::Cdata)).position(|c| c == child).unwrap_or(0)
}

/// Stamps one part of the package. `content_part` is true for content.xml (spreadsheet sheets are turned there).
pub fn stamp_part(t: &Tree, ed: &mut Edits, styles_part: bool, content_part: bool, l: &Lang) {
    let n = t.nodes.len() as u32;
    let (ns0, local0) = l.names[0];
    for i in 0..n {
        if t.is(i, NS_STYLE, "text-properties") {
            // "zxx" / "none" is LibreOffice's "[None]" language (code, URLs): proofing off on purpose
            let cur = t.attr(i, ns0, local0);
            if !matches!(cur.as_deref(), None | Some("zxx") | Some("none")) {
                set_lang(t, ed, i, l);
            }
        }
    }
    if styles_part {
        for i in 0..n {
            if !t.is(i, NS_STYLE, "default-style") || !matches!(t.attr(i, NS_STYLE, "family").as_deref(), Some("paragraph") | Some("graphic")) {
                continue;
            }
            match child_el(t, i, NS_STYLE, "text-properties") {
                Some(tp) => set_lang(t, ed, tp, l),
                None => {
                    let attrs: Vec<(u16, &str, String)> =
                        l.names.iter().enumerate().filter_map(|(k, (ns, local))| l.values[k].clone().map(|v| (*ns, *local, v))).collect();
                    ed.append_child(t, i, make_el(t, i, NS_STYLE, "text-properties", &attrs, "", ed.lxml_prefixes));
                }
            }
            // Text in drawing frames (every ODP / ODG text, ODT text boxes) takes its direction from the graphic
            // default style, so both defaults turn right to left.
            if l.rtl {
                match child_el(t, i, NS_STYLE, "paragraph-properties") {
                    Some(pp) => set_attr(t, ed, pp, NS_STYLE, "writing-mode", Some("rl-tb")),
                    None => {
                        let at = child_el(t, i, NS_STYLE, "graphic-properties").map_or(0, |g| node_index(t, i, g) + 1);
                        let raw = make_el(t, i, NS_STYLE, "paragraph-properties", &[(NS_STYLE, "writing-mode", "rl-tb".to_string())], "", ed.lxml_prefixes);
                        ed.insert_child(t, i, at, raw);
                    }
                }
            }
        }
    }
    if l.rtl {
        flip_frame_styles(t, ed, n);
        if content_part {
            flip_sheets(t, ed);
        }
    }
}

/// An explicit horizontal left-to-right mode on a paragraph, graphic or presentation style turns right to left
/// (a modern ODP pins `lr-tb` on every shape). Vertical modes and "page" are kept.
fn flip_frame_styles(t: &Tree, ed: &mut Edits, n: u32) {
    for i in 0..n {
        if !t.is(i, NS_STYLE, "style") || !matches!(t.attr(i, NS_STYLE, "family").as_deref(), Some("paragraph") | Some("graphic") | Some("presentation")) {
            continue;
        }
        for pp in t.children(i).filter(|c| t.is(*c, NS_STYLE, "paragraph-properties")) {
            let new = match t.attr(pp, NS_STYLE, "writing-mode").as_deref() {
                Some("lr-tb") => "rl-tb",
                Some("lr") => "rl",
                _ => continue,
            };
            set_attr(t, ed, pp, NS_STYLE, "writing-mode", Some(new));
        }
    }
}

/// Every sheet of a spreadsheet turns right to left (column A on the right): `style:writing-mode` on the table
/// properties of the sheet's automatic table style, a new style for a sheet without one.
fn flip_sheets(t: &Tree, ed: &mut Edits) {
    let root = t.root();
    let Some(body) = child_el(t, root, NS_OFFICE, "body") else { return };
    let Some(sheets) = child_el(t, body, NS_OFFICE, "spreadsheet") else { return };
    let tables: Vec<u32> = t.children(sheets).filter(|c| t.is(*c, NS_TABLE, "table")).collect();
    if tables.is_empty() {
        return;
    }
    let lx = ed.lxml_prefixes;
    let autos = child_el(t, root, NS_OFFICE, "automatic-styles");
    let mut by_name: Vec<(String, u32)> = Vec::new();
    let mut taken: Vec<String> = Vec::new();
    if let Some(a) = autos {
        for st in t.children(a).filter(|c| t.is(*c, NS_STYLE, "style")) {
            let name = t.attr(st, NS_STYLE, "name").unwrap_or_default();
            if t.attr(st, NS_STYLE, "family").as_deref() == Some("table") {
                by_name.push((name.clone(), st));
            }
            taken.push(name);
        }
    }
    let mut made: Vec<(Option<String>, String)> = Vec::new(); // (old name, new name)
    let mut new_styles = String::new();
    let mut edited: Vec<u32> = Vec::new();
    for table in tables {
        let name = t.attr(table, NS_TABLE, "style-name");
        let found = name.as_ref().and_then(|nm| by_name.iter().find(|(k, _)| k == nm)).map(|(_, n)| *n);
        match found {
            Some(st) => {
                if !edited.contains(&st) {
                    edited.push(st);
                    match child_el(t, st, NS_STYLE, "table-properties") {
                        Some(tp) => set_attr(t, ed, tp, NS_STYLE, "writing-mode", Some("rl-tb")),
                        None => {
                            let raw = make_el(t, st, NS_STYLE, "table-properties", &[(NS_STYLE, "writing-mode", "rl-tb".to_string())], "", ed.lxml_prefixes);
                            ed.insert_child(t, st, 0, raw);
                        }
                    }
                }
            }
            None => {
                let new_name = match made.iter().find(|(old, _)| *old == name) {
                    Some((_, nn)) => nn.clone(),
                    None => {
                        let mut k = 1;
                        while taken.contains(&format!("taRtl{k}")) {
                            k += 1;
                        }
                        let nn = format!("taRtl{k}");
                        taken.push(nn.clone());
                        let mut attrs: Vec<(u16, &str, String)> = vec![(NS_STYLE, "name", nn.clone()), (NS_STYLE, "family", "table".to_string())];
                        if let Some(p) = &name {
                            attrs.push((NS_STYLE, "parent-style-name", p.clone()));
                        }
                        let tp = make_el(t, autos.unwrap_or(root), NS_STYLE, "table-properties", &[(NS_STYLE, "writing-mode", "rl-tb".to_string())], "", lx);
                        new_styles.push_str(&make_el(t, autos.unwrap_or(root), NS_STYLE, "style", &attrs, &tp, lx));
                        made.push((name.clone(), nn.clone()));
                        nn
                    }
                };
                set_attr(t, ed, table, NS_TABLE, "style-name", Some(&new_name));
            }
        }
    }
    if new_styles.is_empty() {
        return;
    }
    match autos {
        Some(a) => ed.append_child(t, a, new_styles),
        None => {
            let at = node_index(t, root, body);
            let raw = make_el(t, root, NS_OFFICE, "automatic-styles", &[], &new_styles, lx);
            ed.insert_child(t, root, at, raw);
        }
    }
}

/// meta.xml: `dc:language` becomes the target tag. Returns false when there is no `office:meta` to put it in.
pub fn stamp_meta(t: &Tree, ed: &mut Edits, tag: &str) -> bool {
    let Some(meta) = child_el(t, t.root(), NS_OFFICE, "meta") else { return false };
    let mut text = String::new();
    escape_text(tag, &mut text);
    match child_el(t, meta, NS_DC, "language") {
        Some(l) => ed.set_content(t, l, vec![crate::edit::Piece::Raw(text)]),
        None => ed.append_child(t, meta, make_el(t, meta, NS_DC, "language", &[], &text, ed.lxml_prefixes)),
    }
    true
}

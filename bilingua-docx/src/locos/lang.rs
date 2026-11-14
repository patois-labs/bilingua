//! The target language as LocOS's `build_localized` declares it (`_declare_language`, `_keep_source_language`):
//! the styles' document default and the styles that carried the old default, explicit `w:lang` on the translated
//! runs, `w:bidi` on translated paragraphs for a right-to-left target, the complex-script twins of bold, italic and
//! size for a complex-script target, `dc:language` in the core properties, and the source language written onto
//! every untranslated run that inherited it from the old default.

use super::para::{w6_paragraph_of_run, w6_runs};
use super::pkg::{rt, Pkg, Target, RT_CORE};
use super::tree::*;
use super::walk::P;
use bilingua_xmltree::{Dom, Kind};
use std::collections::{HashMap, HashSet};

/// LocOS `a11y.COMPLEX_SCRIPT_LANGS`.
const COMPLEX_SCRIPT: [&str; 25] = ["th", "lo", "km", "my", "hi", "mr", "ne", "sa", "bn", "as", "pa", "gu", "or", "ta", "te", "kn", "ml", "si", "bo", "dz", "kok", "mai", "bho", "doi", "sd"];
const EAST_ASIAN: [&str; 8] = ["zh", "ja", "ko", "yue", "wuu", "hak", "nan", "cmn"];
/// `CT_PPrBase` elements that follow `w:bidi`.
const BIDI_SUCCESSORS: [&str; 17] = [
    "adjustRightInd",
    "snapToGrid",
    "spacing",
    "ind",
    "contextualSpacing",
    "mirrorIndents",
    "suppressOverlap",
    "jc",
    "textDirection",
    "textAlignment",
    "textboxTightWrap",
    "outlineLvl",
    "divId",
    "cnfStyle",
    "rPr",
    "sectPr",
    "pPrChange",
];
const LANG_SUCCESSORS: [&str; 3] = ["eastAsianLayout", "specVanish", "oMath"];
const CS_TWINS: [(&str, &str); 3] = [("b", "bCs"), ("i", "iCs"), ("sz", "szCs")];

/// `a11y.script_class` as the `w:lang` attribute it picks: `val`, `eastAsia` or `bidi`.
pub fn lang_slot(tag: &str, rtl: bool) -> &'static str {
    if tag.is_empty() {
        return "val";
    }
    if rtl {
        return "bidi";
    }
    let subtags: Vec<String> = tag.split('-').map(|s| s.to_lowercase()).collect();
    if COMPLEX_SCRIPT.contains(&subtags[0].as_str()) && !subtags[1..].iter().any(|s| s == "latn") {
        return "bidi";
    }
    if EAST_ASIAN.contains(&subtags[0].as_str()) {
        return "eastAsia";
    }
    "val"
}

/// `insert_element_before`: before the first child found for the successors, taken in their order.
fn insert_before(d: &mut Dom, parent: usize, el: usize, successors: &[&str]) {
    for s in successors {
        if let Some(c) = find_w(d, parent, s) {
            d.add_previous(c, el);
            return;
        }
    }
    d.append(parent, el);
}

fn get_or_add_first(d: &mut Dom, parent: usize, local: &str) -> usize {
    if let Some(x) = find_w(d, parent, local) {
        return x;
    }
    let x = new_w(d, parent, local);
    // lxml's `insert(0, ..)`: the first element slot, after any text that leads the parent (a run holding bare
    // text keeps that text before its new rPr, as LocOS writes it)
    let at = d.first_slot(parent);
    d.insert(parent, at, x);
    x
}

/// `_set_run_lang`.
fn set_run_lang(d: &mut Dom, r: usize, slot: &str, tag: &str) {
    let rpr = get_or_add_first(d, r, "rPr");
    let lang = match find_w(d, rpr, "lang") {
        Some(l) => l,
        None => {
            let l = new_w(d, rpr, "lang");
            insert_before(d, rpr, l, &LANG_SUCCESSORS);
            l
        }
    };
    set_w(d, lang, slot, tag);
}

/// `_mirror_cs_props`.
fn mirror_cs_props(d: &mut Dom, r: usize) {
    let Some(rpr) = find_w(d, r, "rPr") else { return };
    for (name, twin) in CS_TWINS {
        let Some(el) = find_w(d, rpr, name) else { continue };
        if find_w(d, rpr, twin).is_some() {
            continue;
        }
        let new = d.deep_copy(el);
        let qname = match d.nodes[el].name.split_once(':') {
            Some((p, _)) => format!("{p}:{twin}"),
            None => twin.to_string(),
        };
        d.nodes[new].name = qname;
        d.add_next(el, new);
    }
}

type StyleLangs = (HashMap<String, (Option<String>, Option<String>)>, Option<String>);

/// `_style_langs`: (styleId -> (w:lang in `slot`, basedOn), default paragraph style), as the source had them.
fn style_langs(d: &Dom, slot: &str) -> StyleLangs {
    let mut info = HashMap::new();
    let mut default_para = None;
    for st in d.iter(d.root).into_iter().filter(|&s| is_w(d, s, "style")) {
        let sid = get_w(d, st, "styleId").map(str::to_string);
        let lang = find_w(d, st, "rPr").and_then(|r| find_w(d, r, "lang")).and_then(|l| get_w(d, l, slot)).map(str::to_string);
        let based = find_w(d, st, "basedOn").and_then(|b| get_w(d, b, "val")).map(str::to_string);
        if let Some(s) = &sid {
            info.insert(s.clone(), (lang, based));
        }
        if get_w(d, st, "type") == Some("paragraph") && matches!(get_w(d, st, "default"), Some("1") | Some("true")) {
            default_para = sid;
        }
    }
    (info, default_para)
}

/// Sets lxml's `el.text`: the text before the first child.
fn set_lead_text(d: &mut Dom, el: usize, text: &str) {
    while let Some(&c) = d.nodes[el].children.first() {
        if d.nodes[c].kind != Kind::Text {
            break;
        }
        d.detach(c);
    }
    if !text.is_empty() {
        let t = d.new_text(text);
        d.insert(el, 0, t);
    }
}

/// `_declare_language`.
pub fn declare(pkg: &mut Pkg, tag: &str, rtl: bool, translated: &[P]) {
    let slot = lang_slot(tag, rtl);
    let first = tag.split('-').next().unwrap_or("").to_lowercase();
    let cs_target = rtl || COMPLEX_SCRIPT.contains(&first.as_str());
    let main = pkg.main;
    let styles = pkg.related(Some(main), &rt("styles")).filter(|&s| pkg.parts[s].dom.is_some());
    let core = pkg.related(None, RT_CORE).filter(|&c| pkg.parts[c].dom.is_some());
    let mut old_default: Option<String> = None;
    let mut langs: StyleLangs = (HashMap::new(), None);
    if let Some(sp) = styles {
        let d = pkg.parts[sp].dom.as_mut().unwrap();
        langs = style_langs(d, slot);
        let root = d.root;
        let dd = get_or_add_first(d, root, "docDefaults");
        let rpd = get_or_add_first(d, dd, "rPrDefault");
        let rpr = match find_w(d, rpd, "rPr") {
            Some(r) => r,
            None => {
                let r = new_w(d, rpd, "rPr");
                d.append(rpd, r);
                r
            }
        };
        let lang = match find_w(d, rpr, "lang") {
            Some(l) => l,
            None => {
                let l = new_w(d, rpr, "lang");
                insert_before(d, rpr, l, &LANG_SUCCESSORS);
                l
            }
        };
        old_default = get_w(d, lang, slot).map(str::to_string);
        set_w(d, lang, slot, tag);
        if let Some(old) = old_default.as_deref().filter(|o| !o.is_empty()) {
            for el in d.iter(root) {
                if is_w(d, el, "lang") && get_w(d, el, slot) == Some(old) {
                    set_w(d, el, slot, tag);
                }
            }
        }
    }
    for &(part, p) in translated {
        let d = pkg.parts[part].dom.as_mut().unwrap();
        let runs = w6_runs(d, p);
        if styles.is_some() {
            let mut owners = runs.clone();
            owners.extend(find_w(d, p, "pPr"));
            for o in owners {
                if let Some(el) = find_w(d, o, "rPr").and_then(|r| find_w(d, r, "lang")) {
                    if get_w(d, el, slot).is_some() {
                        set_w(d, el, slot, tag);
                    }
                }
            }
        } else {
            for &r in &runs {
                set_run_lang(d, r, slot, tag);
            }
        }
        if cs_target {
            for &r in &runs {
                mirror_cs_props(d, r);
            }
        }
        let bidi = find_w(d, p, "pPr").and_then(|ppr| find_w(d, ppr, "bidi"));
        if rtl && bidi.is_none() {
            let ppr = get_or_add_first(d, p, "pPr");
            let b = new_w(d, ppr, "bidi");
            insert_before(d, ppr, b, &BIDI_SUCCESSORS);
        } else if !rtl {
            if let Some(b) = bidi {
                remove_with_tail(d, b);
            }
        }
    }
    if let Some(old) = old_default.filter(|o| !o.is_empty()) {
        if styles.is_some() {
            keep_source_language(pkg, slot, &old, &langs, translated);
        }
    }
    if let Some(c) = core {
        let d = pkg.parts[c].dom.as_mut().unwrap();
        let root = d.root;
        let el = match d.find(root, DC, "language") {
            Some(e) => e,
            None => {
                let e = new_el(d, root, DC, "language", "dc");
                d.append(root, e);
                e
            }
        };
        set_lead_text(d, el, tag);
    }
}

fn remove_with_tail(d: &mut Dom, el: usize) {
    if let Some(par) = parent(d, el) {
        let kids = &d.nodes[par].children;
        let at = kids.iter().position(|&k| k == el).unwrap();
        let tail: Vec<usize> = kids[at + 1..].iter().copied().take_while(|&k| d.nodes[k].kind == Kind::Text).collect();
        for t in tail {
            d.detach(t);
        }
    }
    d.detach(el);
}

/// `_keep_source_language`.
fn keep_source_language(pkg: &mut Pkg, slot: &str, source: &str, langs: &StyleLangs, translated: &[P]) {
    let (info, default_para) = langs;
    let written: HashSet<P> = translated.iter().copied().collect();
    let chain = |mut sid: Option<String>| -> Option<String> {
        let mut seen = HashSet::new();
        while let Some(s) = sid.filter(|s| !s.is_empty() && !seen.contains(s) && info.contains_key(s)) {
            seen.insert(s.clone());
            let (lang, next) = &info[&s];
            if let Some(l) = lang.as_deref().filter(|l| !l.is_empty()) {
                return Some(l.to_string());
            }
            sid = next.clone();
        }
        None
    };
    let inherited = |d: &Dom, r: usize| -> String {
        let rs = find_w(d, r, "rPr").and_then(|rpr| find_w(d, rpr, "rStyle"));
        if let Some(rs) = rs {
            if let Some(l) = chain(get_w(d, rs, "val").map(str::to_string)) {
                return l;
            }
        }
        let mut p = parent(d, r);
        while let Some(x) = p {
            if is_w(d, x, "p") {
                break;
            }
            p = parent(d, x);
        }
        let ps = p.and_then(|p| d.element_children(p).filter(|&c| is_w(d, c, "pPr")).find_map(|ppr| find_w(d, ppr, "pStyle")));
        let sid = match (p, ps) {
            (_, Some(ps)) => get_w(d, ps, "val").map(str::to_string),
            _ => default_para.clone(),
        };
        chain(sid).unwrap_or_else(|| source.to_string())
    };
    let stamp = |d: &mut Dom, part: usize, root: usize| -> bool {
        let mut changed = false;
        for r in d.iter(root) {
            if !is_w(d, r, "r") {
                continue;
            }
            if w6_paragraph_of_run(d, r).is_some_and(|p| written.contains(&(part, p))) {
                continue;
            }
            let lang = find_w(d, r, "rPr").and_then(|rpr| find_w(d, rpr, "lang"));
            if lang.is_some_and(|l| get_w(d, l, slot).is_some()) {
                continue;
            }
            if inherited(d, r) == source {
                set_run_lang(d, r, slot, source);
                changed = true;
            }
        }
        changed
    };
    let main = pkg.main;
    {
        let d = pkg.parts[main].dom.as_mut().unwrap();
        if let Some(body) = find_w(d, d.root, "body") {
            stamp(d, main, body);
        }
    }
    let kinds: Vec<String> = ["header", "footer", "footnotes", "endnotes", "comments"].iter().map(|k| rt(k)).collect();
    let targets: Vec<usize> = pkg.parts[main]
        .rels
        .iter()
        .filter_map(|r| match r.target {
            Target::Part(t) if kinds.contains(&r.reltype) => Some(t),
            _ => None,
        })
        .collect();
    for t in targets {
        if !pkg.parts[t].xml && !pkg.open(t) {
            continue; // not XML (a damaged part): left exactly as it was
        }
        let p = &mut pkg.parts[t];
        let d = p.dom.as_mut().unwrap();
        let root = d.root;
        if stamp(d, t, root) && !p.xml {
            p.dirty = true;
        }
    }
}

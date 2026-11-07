//! Differential test against the LocOS `dita` and `docbook` filters: 409 documents (the LocOS fixtures, the oracle
//! corpus, hand-written edge cases (19 from the gate) and 370 random ones: DITA topics of six types, maps, bookmaps, DocBook 4.5 and 5,
//! roots neither vocabulary names), with the units, styles, notes and the bytes LocOS builds for two mappings, in
//! place and for three target languages, recorded in `data/locos_golden.json` (made by `tools/gen_golden.py`).

mod common;

use bilingua_model::profile::tokens::{from_locos_text, to_locos_text};
use bilingua_model::*;
use common::*;

fn bytes(v: &serde_json::Value) -> Vec<u8> {
    let s = v.as_str().unwrap();
    match s.split_at(2) {
        ("t:", t) => t.as_bytes().to_vec(),
        ("h:", h) => (0..h.len() / 2).map(|i| u8::from_str_radix(&h[2 * i..2 * i + 2], 16).unwrap()).collect(),
        _ => panic!("bad golden value"),
    }
}

fn mapping(kind: &str, doc: &Document) -> Vec<(usize, String)> {
    doc.units
        .iter()
        .enumerate()
        .filter_map(|(i, u)| {
            let t = to_locos_text(&u.source()).0;
            match kind {
                "all" => Some((i, format!("T{i} \u{e9} {} <&>", t.replace('a', "\u{e0}")))),
                "third" if i % 3 == 0 => Some((i, format!("Zeta {i} \u{65e5}"))),
                _ => None,
            }
        })
        .collect()
}

fn build(f: &dyn Filter, data: &[u8], target: Option<&str>, kind: &str) -> Result<Vec<u8>, Error> {
    let mut doc = f.extract(data, &opts(target))?;
    for (i, t) in mapping(kind, &doc.clone()) {
        let (_, map) = to_locos_text(&doc.units[i].source());
        doc.units[i].segments[0].target = from_locos_text(&t, &map);
    }
    f.merge(&doc)
}

#[test]
fn matches_locos_dita_and_docbook_filters() {
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/locos_golden.json")).unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 409);
    let (dita, docbook) = (Dita::new(), DocBook::new());
    let (mut units_ok, mut unit_count, mut builds_ok, mut both_refuse, mut only_locos_refuses) = (0usize, 0usize, 0usize, 0usize, Vec::<String>::new());
    let mut fails = Vec::<String>::new();
    for c in &cases {
        let name = c["name"].as_str().unwrap();
        let f: &dyn Filter = if c["filter"] == "docbook" { &docbook } else { &dita };
        let data = bytes(&c["data"]);
        let doc = match f.extract(&data, &opts(None)) {
            Ok(d) => d,
            Err(e) => {
                assert!(c.get("error").is_some(), "{name}: LocOS reads it, we refuse: {e}");
                both_refuse += 1;
                continue;
            }
        };
        if let Some(err) = c.get("error") {
            // known difference: lxml refuses a repeated xml:id (an ID validity rule, not well-formedness)
            let err = err.as_str().unwrap();
            // and LocOS refuses a DTD internal subset, which Bilingua keeps (no entity is expanded)
            assert!(err.contains("already defined") || err.contains("already de") || err.contains("internal subset"), "{name}: LocOS refuses ({err}), we read it");
            only_locos_refuses.push(name.to_string());
            continue;
        }
        let want: Vec<(String, String, String)> = c["units"]
            .as_array()
            .unwrap()
            .iter()
            .map(|u| (u["text"].as_str().unwrap().into(), u["style"].as_str().unwrap().into(), u["note"].as_str().unwrap().into()))
            .collect();
        let got: Vec<(String, String, String)> =
            doc.units.iter().map(|u| (show(u), u.context.style.clone().unwrap_or_default(), u.notes.first().map(|n| n.text.clone()).unwrap_or_default())).collect();
        // known difference (issue 809.3, fixed in Bilingua first): no attribute unit on an element under translate="no"
        // or with a conref / conkeyref. Only such LocOS attribute units may be missing from ours, everything else in
        // order; the builds are not comparable then (LocOS translates the extra attributes)
        let known_809 = got != want && {
            let blocked = blocked_attrs(&data);
            let dropped_ok = |w: &(String, String, String)| {
                let attr = match w.1.split_once(':') {
                    Some(("attr", a)) => a,
                    Some(("a11y", "img-alt")) => "alt",
                    _ => return false,
                };
                blocked.iter().any(|(a, v)| a == attr && *v == w.0)
            };
            let mut it = want.iter().peekable();
            let mut ok = true;
            for g in &got {
                while it.peek().is_some_and(|w| *w != g && dropped_ok(w)) {
                    it.next();
                }
                if it.next() != Some(g) {
                    ok = false;
                    break;
                }
            }
            ok && it.all(dropped_ok)
        };
        if got != want && !known_809 {
            fails.push(format!("{name}: units differ\n  ours  {got:?}\n  locos {want:?}"));
            continue;
        }
        units_ok += 1;
        unit_count += got.len();
        if known_809 {
            continue;
        }
        for (key, golden) in c["builds"].as_object().unwrap() {
            let (kind, target) = match key.split_once('@') {
                Some((k, t)) => (k, Some(t)),
                None => (key.as_str(), None),
            };
            let mut want = bytes(golden);
            if name == "gate_db_prefix_decl.dbk" && target.is_some() {
                // known difference (a LocOS bug): a DocBook 4.x root (no namespace) that merely declares the DocBook
                // 5 namespace for a prefix gets `xml:lang` from LocOS, which tests for the namespace URI anywhere in
                // the start tag; the root is outside the namespace, so Bilingua writes 4.x's `lang`
                want = String::from_utf8(want).unwrap().replace(" xml:lang=\"", " lang=\"").into_bytes();
            }
            if doc.units.is_empty() && target.is_some() {
                // known difference: LocOS stamps the target language even when nothing was translated; a Bilingua
                // merge with nothing translated is the original bytes (the LocOS profile stamps, see locos_its.rs)
                assert_eq!(build(f, &data, target, kind).unwrap(), data, "{name} [{key}]");
                builds_ok += 1;
                continue;
            }
            match build(f, &data, target, kind) {
                // known difference (issue 809.2, fixed in Bilingua first): a nested element that still declares the source
                // language is restamped with the target; every other byte must match
                Ok(b) if b == want || (target.is_some() && only_nested_restamps(&b, &want, &data)) => builds_ok += 1,
                Ok(b) => fails.push(format!("{name} [{key}]: bytes differ\n  ours  {:?}\n  locos {:?}", String::from_utf8_lossy(&b), String::from_utf8_lossy(&want))),
                Err(e) => fails.push(format!("{name} [{key}]: we fail: {e}")),
            }
        }
    }
    for x in fails.iter().take(12) {
        eprintln!("{x}");
    }
    assert!(fails.is_empty(), "{} differences ({units_ok} unit lists and {builds_ok} builds agree)", fails.len());
    eprintln!(
        "{units_ok} unit lists ({unit_count} units) and {builds_ok} builds agree with LocOS; {both_refuse} refused by both; {} read by us only (repeated xml:id): {:?}",
        only_locos_refuses.len(),
        only_locos_refuses
    );
    assert!(units_ok >= 355, "{units_ok}");
}

/// The `lang="..."` values of a text (`xml:lang` and DocBook 4 `lang`) and the text between them.
fn lang_split(b: &[u8]) -> (Vec<String>, Vec<String>) {
    let s = String::from_utf8_lossy(b).into_owned();
    let (mut rest_parts, mut vals) = (Vec::new(), Vec::new());
    let mut rest = s.as_str();
    while let Some(i) = rest.find("lang=\"") {
        let v = i + 6;
        rest_parts.push(rest[..v].to_string());
        let end = rest[v..].find('"').map_or(rest.len(), |e| v + e);
        vals.push(rest[v..end].to_string());
        rest = &rest[end..];
    }
    rest_parts.push(rest.to_string());
    (rest_parts, vals)
}

/// Same language for a restamp (the rule of `bilingua-xml`): primary subtag equal, region equal when both have one.
fn same_lang(a: &str, b: &str) -> bool {
    let parts = |s: &str| {
        let s = s.replace('_', "-").to_ascii_lowercase();
        let mut it = s.split('-');
        let p = it.next().unwrap_or("").to_string();
        let r = it.find(|x| x.len() == 2).map(str::to_string);
        (p, r)
    };
    let ((pa, ra), (pb, rb)) = (parts(a), parts(b));
    !pa.is_empty() && pa == pb && (ra.is_none() || rb.is_none() || ra == rb)
}

/// Our build and LocOS's differ only where a nested language attribute that still named the source language (the
/// first language the input declares) now holds the target language, which the root also holds.
fn only_nested_restamps(ours: &[u8], locos: &[u8], input: &[u8]) -> bool {
    let (o_rest, o_vals) = lang_split(ours);
    let (l_rest, l_vals) = lang_split(locos);
    let Some(src) = lang_split(input).1.into_iter().next() else { return false };
    o_rest == l_rest
        && o_vals.len() == l_vals.len()
        && !o_vals.is_empty()
        && o_vals[0] == l_vals[0]
        && o_vals.iter().zip(&l_vals).skip(1).all(|(o, l)| o == l || (*o == o_vals[0] && same_lang(l, &src)))
}

/// The predefined entities and character references of an attribute value decoded (anything else kept as written).
fn decode(v: &str) -> String {
    let mut out = String::new();
    let mut rest = v;
    while let Some(a) = rest.find('&') {
        out.push_str(&rest[..a]);
        let Some(e) = rest[a..].find(';').map(|e| a + e) else { break };
        let body = &rest[a + 1..e];
        let c = match body {
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => match body.strip_prefix("#x").or_else(|| body.strip_prefix("#X")) {
                Some(h) => u32::from_str_radix(h, 16).ok().and_then(char::from_u32),
                None => body.strip_prefix('#').and_then(|d| d.parse().ok()).and_then(char::from_u32),
            },
        };
        match c {
            Some(c) => out.push(c),
            None => out.push_str(&rest[a..=e]),
        }
        rest = &rest[e + 1..];
    }
    out.push_str(rest);
    out
}

/// `(attribute local name, value as a unit shows it)` for every attribute of an element under `translate="no"` (its
/// own or an ancestor's) or with a `conref` / `conkeyref` (which the element's descendants inherit): the attribute
/// units LocOS still offers there and Bilingua does not (issue 809.3). A small scanner for the golden inputs.
fn blocked_attrs(data: &[u8]) -> Vec<(String, String)> {
    let s = String::from_utf8_lossy(data).into_owned();
    let mut out = Vec::new();
    let mut stack: Vec<bool> = Vec::new();
    let mut i = 0;
    let skip_to = |from: usize, end: &str| s[from..].find(end).map_or(s.len(), |e| from + e + end.len());
    while let Some(lt) = s[i..].find('<').map(|x| x + i) {
        let r = &s[lt..];
        if r.starts_with("<!--") {
            i = skip_to(lt, "-->");
        } else if r.starts_with("<![CDATA[") {
            i = skip_to(lt, "]]>");
        } else if r.starts_with("<?") {
            i = skip_to(lt, "?>");
        } else if r.starts_with("<!") {
            let mut depth = 0i32;
            i = s.len();
            for (k, c) in r.char_indices() {
                match c {
                    '[' => depth += 1,
                    ']' => depth -= 1,
                    '>' if depth <= 0 => {
                        i = lt + k + 1;
                        break;
                    }
                    _ => {}
                }
            }
        } else if r.starts_with("</") {
            stack.pop();
            i = skip_to(lt, ">");
        } else {
            // a start tag: name, then name="value" pairs up to > or />
            let b = r.as_bytes();
            let mut k = 1;
            while k < b.len() && !b[k].is_ascii_whitespace() && b[k] != b'>' && b[k] != b'/' {
                k += 1;
            }
            let mut attrs: Vec<(String, String)> = Vec::new();
            let mut empty = false;
            loop {
                while k < b.len() && b[k].is_ascii_whitespace() {
                    k += 1;
                }
                if k >= b.len() || b[k] == b'>' {
                    break;
                }
                if b[k] == b'/' {
                    empty = true;
                    k += 1;
                    continue;
                }
                let ns = k;
                while k < b.len() && b[k] != b'=' && !b[k].is_ascii_whitespace() && b[k] != b'>' {
                    k += 1;
                }
                let name = r[ns..k].to_string();
                while k < b.len() && (b[k] == b'=' || b[k].is_ascii_whitespace()) {
                    k += 1;
                }
                if k >= b.len() || !(b[k] == b'"' || b[k] == b'\'') {
                    continue;
                }
                let q = b[k] as char;
                let vs = k + 1;
                let ve = r[vs..].find(q).map_or(r.len(), |e| vs + e);
                attrs.push((name, r[vs..ve].to_string()));
                k = ve + 1;
            }
            i = (lt + k + 1).min(s.len());
            let local = |n: &str| n.rsplit(':').next().unwrap_or(n).to_string();
            let parent = stack.last().copied().unwrap_or(false);
            let mut blocked = match attrs.iter().find(|(n, _)| local(n) == "translate").map(|(_, v)| v.as_str()) {
                Some("no") => true,
                Some("yes") => false,
                _ => parent,
            };
            blocked |= attrs.iter().any(|(n, _)| n == "conref" || n == "conkeyref");
            if blocked {
                for (n, v) in &attrs {
                    out.push((local(n), decode(v).split_whitespace().collect::<Vec<_>>().join(" ")));
                }
            }
            if !empty {
                stack.push(blocked);
            }
        }
    }
    out
}

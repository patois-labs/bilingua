//! Differential test against the LocOS HTML filter (`app/filters/html_filter.py`, `HtmlFilter`): random pages and
//! XHTML documents in 11 languages, legacy code pages, byte mutations, hostile pages, `.html` pages with server code
//! and the two filter variants (`page_meta=False`, a template pattern), with the units, the markup behind every
//! token, the bytes LocOS builds with four mappings and the localized builds recorded in `data/locos_golden.json`
//! (made by `tools/gen_golden.py`). Every unit and every build must match; the differences kept on purpose are
//! listed in `known_difference`.
use bilingua_html::HtmlFilter;
use bilingua_model::*;

fn hex(s: &str) -> Vec<u8> {
    (0..s.len() / 2).map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap()).collect()
}

fn filter(cfg: &str) -> HtmlFilter {
    match cfg {
        "nometa" => HtmlFilter::new().with_page_meta(false),
        "template" => HtmlFilter::new().with_template_pattern(regex::Regex::new(r"(?s)\{\{.*?\}\}|\{%.*?%\}").unwrap()),
        _ => HtmlFilter::new(),
    }
}

/// The unit text the way LocOS shows it: `g` pairs and `x` points numbered by the unit's own token ids. An orphan
/// end tag (kept on purpose, LocOS issue 810) has no token in LocOS: it is left out and the ids after it are
/// renumbered the way LocOS counts ([`locos_id`]).
fn locos_text(u: &Unit) -> String {
    let orphans = orphan_ids(u);
    let mut s = String::new();
    for i in u.source() {
        match i {
            Inline::Text(t) => s.push_str(&t),
            Inline::Tag { id, .. } if orphans.contains(&id) => {}
            Inline::Tag { id, kind: TagKind::Open, .. } => s.push_str(&format!("\u{27E6}g{}\u{27E7}", locos_id(&id, &orphans))),
            Inline::Tag { id, kind: TagKind::Close, .. } => s.push_str(&format!("\u{27E6}/g{}\u{27E7}", locos_id(&id, &orphans))),
            Inline::Tag { id, kind: TagKind::Standalone, .. } => s.push_str(&format!("\u{27E6}x{}\u{27E7}", locos_id(&id, &orphans))),
        }
    }
    s
}

/// The ids of a unit's orphan end tags: point codes whose markup is an end tag (`</b>` with no `<b>`). Bilingua
/// keeps them on purpose (LocOS issue 810); LocOS drops them from the unit and gives them no token number.
fn orphan_ids(u: &Unit) -> Vec<String> {
    u.source()
        .iter()
        .filter_map(|i| match i {
            Inline::Tag { id, kind: TagKind::Standalone, .. }
                if u.codes.get(id).and_then(|c| c.standalone.as_deref()).is_some_and(|t| t.starts_with("</")) =>
            {
                Some(id.clone())
            }
            _ => None,
        })
        .collect()
}

/// A token id as LocOS numbers it: one counter for every token and no number for an orphan end tag, so each id
/// after an orphan is one lower.
fn locos_id(id: &str, orphans: &[String]) -> String {
    match id.parse::<usize>() {
        Ok(n) => (n - orphans.iter().filter(|o| o.parse::<usize>().is_ok_and(|m| m < n)).count()).to_string(),
        Err(_) => id.to_string(),
    }
}

/// A built page as UTF-8 text with its charset declaration blanked: read as UTF-8 when it is, else as Windows-1252.
fn page_text(b: &[u8]) -> Vec<u8> {
    let s = match std::str::from_utf8(b) {
        Ok(s) => s.to_string(),
        Err(_) => bilingua_model::encoding_rs::WINDOWS_1252.decode_without_bom_handling(b).0.into_owned(),
    };
    regex::Regex::new(r#"charset="[^"]*""#).unwrap().replace_all(&s, "charset=\"?\"").into_owned().into_bytes()
}

/// LocOS reads a page that declares ISO-8859-8 as Windows-1252 (LocOS issue 810.3; Bilingua reads it as Hebrew).
/// The page, and LocOS's builds of it, with the declaration saying Windows-1252, which Bilingua reads the LocOS way.
fn as_locos_1252(b: &[u8]) -> Vec<u8> {
    let (from, to): (&[u8], &[u8]) = (b"charset=\"iso-8859-8\"", b"charset=\"windows-1252\"");
    let mut out = Vec::with_capacity(b.len() + 8);
    let mut i = 0;
    while i < b.len() {
        if b[i..].starts_with(from) {
            out.extend_from_slice(to);
            i += from.len();
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    out
}

/// What the LocOS adapter does with a translated text: `x`, `g` and `/g` tokens name the source tag with that
/// number and kind; a token the source does not have, or a second use of one, is dropped.
fn target_from_locos(u: &Unit, text: &str) -> Vec<Inline> {
    let src = u.source();
    let find = |id: &str, kind: TagKind| src.iter().find(|i| matches!(i, Inline::Tag { id: i2, kind: k2, .. } if i2 == id && *k2 == kind)).cloned();
    let mut out: Vec<Inline> = Vec::new();
    let mut used: Vec<(String, TagKind)> = Vec::new();
    let mut lit = String::new();
    let mut rest = text;
    while let Some(i) = rest.find('\u{27E6}') {
        lit.push_str(&rest[..i]);
        let after = &rest[i + '\u{27E6}'.len_utf8()..];
        let Some(j) = after.find('\u{27E7}') else {
            lit.push('\u{27E6}');
            rest = after;
            continue;
        };
        let inner = &after[..j];
        let (kind, body) = if let Some(b) = inner.strip_prefix('/') {
            (TagKind::Close, b.trim_start_matches(|c: char| c.is_ascii_lowercase()))
        } else if inner.starts_with('x') || inner.starts_with('p') {
            (TagKind::Standalone, &inner[1..])
        } else {
            (TagKind::Open, inner.trim_start_matches(|c: char| c.is_ascii_lowercase()))
        };
        if !body.is_empty() && body.bytes().all(|b| b.is_ascii_digit()) {
            if let Some(t) = find(body, kind).filter(|_| !used.contains(&(body.to_string(), kind))) {
                if !lit.is_empty() {
                    out.push(Inline::Text(std::mem::take(&mut lit)));
                }
                out.push(t);
                used.push((body.to_string(), kind));
            }
        } else {
            lit.push_str(&rest[i..i + '\u{27E6}'.len_utf8() + j + '\u{27E7}'.len_utf8()]);
        }
        rest = &after[j + '\u{27E7}'.len_utf8()..];
    }
    lit.push_str(rest);
    if !lit.is_empty() {
        out.push(Inline::Text(lit));
    }
    out
}

fn mapping(n: usize, label: &str, want: &[serde_json::Value], bom: bool) -> Vec<(usize, String)> {
    let text = |i: usize| {
        let t = want[i]["text"].as_str().unwrap();
        if i == 0 && bom {
            t.trim_start_matches('\u{feff}').to_string()
        } else {
            t.to_string()
        }
    };
    match label {
        "all" => (0..n).map(|i| (i, format!("T{i} \"q\" \u{e9} <b> & &amp; ${{x}} {{{{ v }}}} \\"))).collect(),
        "third" => (0..n).step_by(3).map(|i| (i, format!("Zeta {i} \u{65e5} 'it's' &nbsp;"))).collect(),
        "tok" => (0..n).map(|i| (i, format!("{} Z", text(i)))).collect(),
        "swap" => {
            let mut out = Vec::new();
            for i in 0..n {
                let t = text(i);
                let mut toks: Vec<String> = Vec::new();
                let mut rest = t.as_str();
                while let Some(a) = rest.find('\u{27E6}') {
                    let Some(b) = rest[a..].find('\u{27E7}') else { break };
                    toks.push(rest[a..a + b + '\u{27E7}'.len_utf8()].to_string());
                    rest = &rest[a + b + '\u{27E7}'.len_utf8()..];
                }
                if !toks.is_empty() {
                    toks.reverse();
                    out.push((i, format!("{} Q", toks.join(" "))));
                }
            }
            out
        }
        _ => unreachable!(),
    }
}

fn build(f: &HtmlFilter, data: &[u8], m: &[(usize, String)], loc: Option<&str>) -> Result<Vec<u8>, Error> {
    let mut doc = f.extract(data, &ExtractOptions::default())?;
    doc.target_lang = loc.map(String::from);
    for (k, t) in m {
        let id = doc.units[*k].segments[0].id.clone();
        let inl = target_from_locos(&doc.units[*k], t);
        doc.set_target(&id, inl).expect("set_target");
    }
    // LocOS build_localized declares the language even when nothing is translated
    let opts = if loc.is_some() { MergeOptions::new().with_stamp_target_always(true) } else { MergeOptions::new() };
    f.merge_with(&doc, &opts)
}

/// LocOS reads a UTF-8 byte order mark as a text character (lost when the first unit is translated); Bilingua
/// reads it as the envelope and keeps it (B6). Compared without it.
fn nobom(b: &[u8]) -> Vec<u8> {
    b.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(b).to_vec()
}

/// A difference kept on purpose (see the README, "Differences from LocOS"); `None` when there is none.
fn known_difference(name: &str, err: &Error) -> Option<&'static str> {
    match err {
        Error::Limit(l) if l.kind == LimitKind::Depth && name.starts_with("hostile") => Some("depth cap"),
        _ => None,
    }
}

#[test]
fn matches_locos_on_random_pages_and_hostile_cases() {
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/locos_golden.json")).unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 500, "{} cases", cases.len());
    let (mut pages, mut units_n, mut builds, mut refused, mut skipped) = (0, 0, 0, 0, 0);
    let mut kept: Vec<String> = Vec::new();
    let mut bad: Vec<String> = Vec::new();
    for c in &cases {
        let name = c["name"].as_str().unwrap();
        let cfg = c["cfg"].as_str().unwrap();
        let f = filter(cfg);
        let mut data = hex(c["hex"].as_str().unwrap());
        let orig = data.clone();
        // kept on purpose (LocOS issue 810.3): Bilingua reads the declared ISO-8859-8 and writes it back unchanged;
        // the units are compared reading the page the LocOS way (as Windows-1252), the builds as text (see below)
        let hebrew = name.starts_with("legacy-iso-8859-8");
        if hebrew {
            let d = f.extract(&data, &ExtractOptions::default()).expect("ISO-8859-8 page");
            let he = |u: &Unit| u.source().iter().any(|i| matches!(i, Inline::Text(t) if t.chars().any(|c| ('\u{5d0}'..='\u{5ea}').contains(&c))));
            assert!(d.units.iter().any(he), "{name}: not read as Hebrew");
            assert_eq!(f.merge(&d).unwrap(), data, "{name}: unchanged merge");
            data = as_locos_1252(&data);
        }
        if let Some(err) = c.get("error") {
            match f.extract(&data, &ExtractOptions::default()) {
                Ok(_) => bad.push(format!("{name}: LocOS refuses ({err}), Bilingua opens it")),
                Err(Error::Parse { msg, .. }) if err == "ServerPageRefused" && msg != c["message"].as_str().unwrap() => {
                    bad.push(format!("{name}: refusal text {msg:?}"));
                }
                Err(_) => refused += 1,
            }
            continue;
        }
        let bom = data.starts_with(&[0xEF, 0xBB, 0xBF]);
        // LocOS reads the mark as a text character, so white space right after it is not leading white space there
        if bom && std::str::from_utf8(&data[3..]).is_ok_and(|s| s.starts_with(|c: char| c.is_whitespace())) {
            skipped += 1;
            continue;
        }
        let doc = match f.extract(&data, &ExtractOptions::default()) {
            Ok(d) => d,
            Err(e) => {
                match known_difference(name, &e) {
                    Some(why) => kept.push(format!("{name}: {why}")),
                    None => bad.push(format!("{name}: Bilingua refuses ({e}), LocOS opens it")),
                }
                continue;
            }
        };
        // kept on purpose (LocOS issue 810): an orphan end tag is a point code here; LocOS drops it from the unit
        // (compared without it, see `locos_text`) and blanks it from a build (such a page's builds are not compared)
        let orphan_end = doc.units.iter().any(|u| !orphan_ids(u).is_empty());
        let want = c["units"].as_array().unwrap();
        if doc.units.len() != want.len() {
            bad.push(format!("{name}: {} units, LocOS {}: {:?} vs {:?}", doc.units.len(), want.len(), doc.units.iter().map(locos_text).collect::<Vec<_>>(), want.iter().map(|w| w["text"].as_str().unwrap()).collect::<Vec<_>>()));
            continue;
        }
        let mut unit_bad = false;
        for (k, (u, w)) in doc.units.iter().zip(want).enumerate() {
            let wt = w["text"].as_str().unwrap();
            let wt = if k == 0 && bom { wt.trim_start_matches('\u{feff}') } else { wt };
            let style = w["style"].as_str().filter(|s| !s.is_empty());
            // the white space next to an orphan end tag at a unit's edge is an edge of the unit in LocOS (trimmed there)
            let lt = locos_text(u);
            let same = if orphan_ids(u).is_empty() { lt == wt } else { lt.trim() == wt.trim() };
            if !same || u.context.style.as_deref() != style {
                bad.push(format!("{name}: unit {k}: {:?} {:?}, LocOS {:?} {:?}", locos_text(u), u.context.style, wt, style));
                unit_bad = true;
                break;
            }
            let map = w["map"].as_object().unwrap();
            let mut seen = 0;
            let orphans = orphan_ids(u);
            for i in u.source() {
                let Inline::Tag { id, kind, .. } = i else { continue };
                if orphans.contains(&id) {
                    continue;
                }
                let (Some(entry), Some(code)) = (map.get(&locos_id(&id, &orphans)), u.codes.get(&id)) else {
                    bad.push(format!("{name}: unit {k}: token {id} missing"));
                    unit_bad = true;
                    break;
                };
                let ok = match kind {
                    TagKind::Open => {
                        seen += 1;
                        entry["open"].as_str() == code.open.as_deref() && entry["close"].as_str() == code.close.as_deref()
                    }
                    TagKind::Standalone => {
                        seen += 1;
                        entry["point"].as_str() == code.standalone.as_deref()
                    }
                    TagKind::Close => true,
                };
                if !ok {
                    bad.push(format!("{name}: unit {k}: markup of token {id}: {code:?} vs {entry}"));
                    unit_bad = true;
                    break;
                }
            }
            if unit_bad {
                break;
            }
            if seen != map.len() {
                bad.push(format!("{name}: unit {k}: {seen} tokens, LocOS {}", map.len()));
                unit_bad = true;
                break;
            }
        }
        if unit_bad {
            continue;
        }
        pages += 1;
        units_n += want.len();
        if orphan_end {
            kept.push(format!("{name}: orphan end tag kept (units match without it, builds not compared)"));
            continue;
        }
        let n = want.len();
        let jsp = c["server_code"].as_str() == Some("jsp");
        let mut runs: Vec<(String, Option<&str>, &serde_json::Value)> = ["all", "third", "tok", "swap"].iter().map(|l| (l.to_string(), None, &c["builds"][*l])).collect();
        for loc in ["fr-FR", "ar"] {
            runs.push(("tok".into(), Some(loc), &c["loc"][loc]));
        }
        for (label, loc, exp) in runs {
            let m = mapping(n, &label, want, bom);
            // ISO-8859-8 pages: the mapping that replaces every unit ("all") is built from the page as it is and must
            // give LocOS's bytes; one that keeps some source text (LocOS's mojibake) is built from the page read the
            // LocOS way, and the two are compared as text, since the code page each side can write the mojibake in differs
            let as_text = hebrew && label != "all";
            let got = build(&f, if hebrew && !as_text { &orig } else { &data }, &m, loc);
            let what = format!("{label}{}", loc.map(|l| format!(" {l}")).unwrap_or_default());
            match (exp, got) {
                (serde_json::Value::String(h), Ok(g)) => {
                    let e = if as_text { page_text(&as_locos_1252(&hex(h))) } else { hex(h) };
                    let g = if as_text { page_text(&g) } else { g };
                    // kept (bilingua-serverpages): a JSP translation ending in a backslash before `${` gets the
                    // backslash as `&#92;` so the expression stays live; LocOS writes it raw
                    let g = if jsp { String::from_utf8_lossy(&g).replace("&#92;", "\\").into_bytes() } else { g };
                    if nobom(&g) != nobom(&e) {
                        bad.push(format!("{name}: build {what}:\n  got  {:?}\n  want {:?}", String::from_utf8_lossy(&g), String::from_utf8_lossy(&e)));
                    } else {
                        builds += 1;
                    }
                }
                (serde_json::Value::String(_), Err(e)) => bad.push(format!("{name}: build {what}: Bilingua refuses ({e}), LocOS builds")),
                (err, Ok(_)) => bad.push(format!("{name}: build {what}: LocOS refuses ({}), Bilingua builds", err["error"])),
                (_, Err(_)) => builds += 1,
            }
        }
    }
    println!("{pages} pages ({units_n} units) match LocOS, {builds} builds identical, {refused} refused by both, {skipped} skipped (BOM + white space)");
    println!("kept differences: {kept:?}");
    for b in bad.iter().take(40) {
        println!("MISMATCH {b}");
    }
    assert!(bad.is_empty(), "{} mismatches", bad.len());
    assert!(pages >= 500, "only {pages} pages");
}

//! Differential test against the LocOS image filter: 782 files (random SVGs in several encodings, mutated and
//! broken ones, the SVGs of the LocOS suites, PNGs with region sidecars, other rasters) with the units and the build
//! digests LocOS produced, recorded in `data/locos_golden.json` (made by `tools/gen_golden.py`).
mod common;
use bilingua_image::{ImageFilter, SvgFilter};
use bilingua_model::*;
use common::*;

fn mapped(label: &str, i: usize, t: &str) -> Option<String> {
    Some(match label {
        "all" => format!("FR {t}"),
        "third" => return i.is_multiple_of(3).then(|| format!("Zeta {i} \u{65e5}\u{672c}")),
        "plain" => format!("Plain{i}"),
        "cjk" => format!("\u{65e5}\u{672c}\u{8a9e}{i}"),
        "ar" => format!("\u{645}\u{631}\u{62d}\u{628}\u{627} {i}"),
        "sym" => format!("A & \"B\" <c> 'd' ]]> {t}"),
        "pad" => format!("  {t}  "),
        _ => unreachable!("{label}"),
    })
}

/// LocOS leaves a declared charset in the XML declaration of a file that is valid UTF-8 already (it never rewrites
/// it there, so an ISO-8859-1 declaration can sit over UTF-8 text); Bilingua rewrites it to UTF-8 so the file says
/// what it is (a deliberate difference). Put the original declaration back to compare the bytes.
fn digest(r: Result<Vec<u8>, Error>, decl: &str, input: &[u8]) -> String {
    match r {
        Ok(mut b) => {
            if !decl.is_empty() && std::str::from_utf8(input).is_ok() {
                let q = decl.chars().next_back().unwrap();
                let eq = decl.find('=').unwrap();
                let spaced = format!("{}={q}UTF-8{q}", decl[..eq].trim_end());
                let t = String::from_utf8_lossy(&b).into_owned();
                let alt = format!("{}{}", &decl[..eq], &format!("={q}UTF-8{q}")[..]);
                for pat in [spaced, alt] {
                    if let Some(p) = t.find(&pat) {
                        b = format!("{}{}{}", &t[..p], decl, &t[p + pat.len()..]).into_bytes();
                        break;
                    }
                }
            }
            fnv(&b)
        }
        Err(e) => format!("ERR:{e}"),
    }
}

#[test]
fn matches_locos_on_random_files() {
    let text = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/locos_golden.json")).unwrap();
    let cases: Vec<serde_json::Value> = serde_json::from_str(&text).unwrap();
    assert!(cases.len() >= 780);
    let (mut refused, mut nunits, mut builds, mut svgs, mut pngs, mut enc_refused) = (0, 0, 0, 0, 0, 0);
    let mut bad: Vec<String> = Vec::new();
    for c in &cases {
        let name = c["name"].as_str().unwrap();
        let data = hex(c["hex"].as_str().unwrap());
        let png = c["kind"] == "png";
        let r = if png { ImageFilter.extract(&data, &opts()) } else { SvgFilter.extract(&data, &opts()) };
        if c["units"] == "error" {
            refused += 1;
            if r.is_ok() {
                bad.push(format!("{name}: LocOS refuses ({}), we accept", c["error"]));
            }
            continue;
        }
        let doc = match r {
            Ok(d) => d,
            // LocOS cannot import a file its sniff misses (the golden calls the SVG reader directly); we still read it
            Err(e) if !png && !c["sniff_svg"].as_bool().unwrap() && e.to_string().contains("not SVG") => continue,
            // a declared encoding is binding in Bilingua (bytes invalid in it are refused); LocOS falls back to Windows-1252
            Err(Error::Encoding(_)) => {
                enc_refused += 1;
                continue;
            }
            Err(e) => {
                bad.push(format!("{name}: we refuse ({e}), LocOS accepts"));
                continue;
            }
        };
        if png {
            pngs += 1;
            let want: Vec<&str> = c["units"].as_array().unwrap().iter().map(|u| u.as_str().unwrap()).collect();
            let got = texts(&doc);
            if got != want {
                bad.push(format!("{name}: units\n  want {want:?}\n  got  {got:?}"));
            }
            for (u, g) in doc.units.iter().zip(c["geom"].as_array().unwrap()) {
                let loc = format!("{},{},{},{}", g[0], g[1], g[2], g[3]);
                if u.context.location.as_deref() != Some(&loc) {
                    bad.push(format!("{name}: bbox {:?} want {loc}", u.context.location));
                }
            }
            nunits += want.len();
            continue;
        }
        svgs += 1;
        let _ = c["sniff_svg"].as_bool().unwrap();
        let want: Vec<(String, String)> = c["units"].as_array().unwrap().iter().map(|u| (u[0].as_str().unwrap().to_string(), u[1].as_str().unwrap().to_string())).collect();
        let got: Vec<(String, String)> = texts(&doc).into_iter().zip(styles(&doc)).collect();
        if got != want {
            bad.push(format!("{name}: units\n  want {want:?}\n  got  {got:?}
  warnings {:?}", doc.warnings));
            continue;
        }
        nunits += want.len();
        let map = |label: &str| -> Vec<(usize, String)> { want.iter().enumerate().filter_map(|(i, (t, _))| mapped(label, i, t).map(|m| (i, m))).collect() };
        for (label, d) in c["builds"].as_object().unwrap() {
            let m = map(label);
            if m.is_empty() && std::str::from_utf8(&data).is_err() {
                continue; // nothing written: LocOS returns the upload as it is, Bilingua honours "UTF-8 out"
            }
            let mr: Vec<(usize, &str)> = m.iter().map(|(i, s)| (*i, s.as_str())).collect();
            let got = digest(build(&data, &mr), c["decl"].as_str().unwrap(), &data);
            builds += 1;
            if got != d.as_str().unwrap() {
                bad.push(format!("{name}: build {label}: want {d} got {got}"));
            }
        }
        for (key, d) in c["localized"].as_object().unwrap() {
            if !c["sniff_svg"].as_bool().unwrap() {
                continue; // LocOS cannot import this file as an image (its sniff misses it), so it never builds it
            }
            let (loc, label) = key.split_once('|').unwrap();
            let m = map(label);
            if m.is_empty() && (loc.is_empty() || loc.contains('!') || loc.len() > 64) {
                // LocOS build_localized is never short-circuited: with no usable language and nothing translated it
                // still rewrites the file as UTF-8, where Bilingua returns the original bytes (an untouched merge)
                continue;
            }
            let mr: Vec<(usize, &str)> = m.iter().map(|(i, s)| (*i, s.as_str())).collect();
            let got = digest(build_localized(&data, &mr, loc), c["decl"].as_str().unwrap(), &data);
            builds += 1;
            if got != d.as_str().unwrap() {
                bad.push(format!("{name}: localized {key}: want {d} got {got}"));
            }
        }
    }
    println!("{} cases ({svgs} svg, {pngs} png), {refused} refused, {enc_refused} refused for a declared encoding LocOS reads anyway, {nunits} units, {builds} builds, {} differences", cases.len(), bad.len());
    let mut groups: std::collections::BTreeMap<String, (usize, Vec<String>)> = Default::default();
    for b in &bad {
        let rest = b.split_once(": ").map_or(b.as_str(), |x| x.1);
        let cat: String = if let Some(r) = rest.strip_prefix("localized ") { format!("localized {}", r.split(['|', ':']).nth(1).unwrap_or("")) } else { rest.split(':').next().unwrap_or("").to_string() };
        let e = groups.entry(cat).or_default();
        e.0 += 1;
        if e.1.len() < 3 {
            e.1.push(b.clone());
        }
    }
    let mut report = String::new();
    for (k, (n, ex)) in &groups {
        report.push_str(&format!("== {k}: {n}
"));
        for x in ex {
            report.push_str(&format!("   {x}
"));
        }
    }
    assert!(bad.is_empty(), "{}
{}", bad.len(), report);
}

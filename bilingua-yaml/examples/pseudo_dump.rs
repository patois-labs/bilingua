//! Dev tool: for each FILE prints a JSON line with the unit texts (tags written out) and writes the pseudo-translated
//! merge ("[xx] " in front of every unit) to OUTDIR/<n>.out. Used by the differential check against LocOS.
use bilingua_model::*;
use bilingua_yaml::{Limits, Yaml};
use std::io::Write;

fn disp(u: &Unit, v: &[Inline]) -> String {
    let mut s = String::new();
    for i in v {
        match i {
            Inline::Text(t) => s.push_str(t),
            Inline::Tag { id, kind, .. } => {
                let c = &u.codes[id];
                s.push_str(
                    match kind {
                        TagKind::Open => c.open.as_deref(),
                        TagKind::Close => c.close.as_deref(),
                        TagKind::Standalone => c.standalone.as_deref(),
                    }
                    .unwrap_or(""),
                );
            }
        }
    }
    s
}

fn main() {
    let mut args = std::env::args().skip(1);
    let outdir = args.next().unwrap();
    let target = std::env::var("TARGET").ok();
    let out = std::io::stdout();
    let mut out = out.lock();
    for (n, f) in args.enumerate() {
        let bytes = std::fs::read(&f).unwrap();
        let opts = ExtractOptions::new().with_target_lang(target.clone());
        let mut line = serde_json::json!({"file": f, "n": n});
        match Yaml.extract_with(&bytes, &opts, &Limits::unlimited()) {
            Err(e) => line["error"] = serde_json::json!(e.to_string()),
            Ok(mut d) => {
                line["units"] = serde_json::json!(d.units.iter().map(|u| disp(u, &u.source())).collect::<Vec<_>>());
                let plain = Yaml.merge(&d).unwrap();
                line["identity"] = serde_json::json!(plain == bytes);
                for u in &mut d.units {
                    let mut t = vec![Inline::text("[xx] ")];
                    t.extend(u.source());
                    u.segments[0].target = merge_text(t);
                }
                match Yaml.merge(&d) {
                    Ok(m) => {
                        std::fs::write(format!("{outdir}/{n}.out"), m).unwrap();
                    }
                    Err(e) => line["merge_error"] = serde_json::json!(e.to_string()),
                }
            }
        }
        writeln!(out, "{line}").unwrap();
    }
}

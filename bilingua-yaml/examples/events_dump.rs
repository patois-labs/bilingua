//! Dev tool: prints the event stream of FILE.yaml as JSON lines (used by the differential check against PyYAML).
use bilingua_yaml::scan::{scan, Ev, Style};
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = std::io::stdout();
    let mut out = out.lock();
    for f in args {
        let bytes = std::fs::read(&f).unwrap();
        let text = String::from_utf8_lossy(bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes)).to_string();
        let mut line = serde_json::json!({"file": f});
        match scan(&text, 200) {
            Err(e) => line["error"] = serde_json::json!(e.msg),
            Ok(evs) => {
                let list: Vec<serde_json::Value> = evs
                    .iter()
                    .map(|e| match e {
                        Ev::DocStart => serde_json::json!(["D"]),
                        Ev::Alias(n) => serde_json::json!(["A", n]),
                        Ev::MapStart { tag, flow } => serde_json::json!(["M", tag, flow]),
                        Ev::SeqStart { tag, flow } => serde_json::json!(["Q", tag, flow]),
                        Ev::MapEnd => serde_json::json!(["m"]),
                        Ev::SeqEnd => serde_json::json!(["q"]),
                        Ev::Scalar(s) => {
                            let st = match s.style {
                                Style::Plain => "p",
                                Style::Single => "'",
                                Style::Double => "\"",
                                Style::Literal => "|",
                                Style::Folded => ">",
                            };
                            serde_json::json!(["S", s.value, st, s.tag, s.start, s.end, s.token])
                        }
                    })
                    .collect();
                line["events"] = serde_json::Value::Array(list);
            }
        }
        writeln!(out, "{line}").unwrap();
    }
}

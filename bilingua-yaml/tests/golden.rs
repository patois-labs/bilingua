//! The reader's events against PyYAML (libyaml) on 360 documents, frozen in `data/events_golden.json`
//! (3 LocOS fixtures, 57 hand-written edge cases, 300 generated documents; 101 of them are errors).
//! Regenerate with the script described in the README.

mod common;
use bilingua_yaml::scan::{scan, Ev, Style};
use serde_json::{json, Value};

fn events_json(t: &str) -> Result<Vec<Value>, String> {
    let evs = scan(t, 200).map_err(|e| e.msg)?;
    Ok(evs
        .iter()
        .map(|e| match e {
            Ev::DocStart => json!(["D"]),
            Ev::Alias(n) => json!(["A", n]),
            Ev::MapStart { tag, flow } => json!(["M", tag, flow]),
            Ev::SeqStart { tag, flow } => json!(["Q", tag, flow]),
            Ev::MapEnd => json!(["m"]),
            Ev::SeqEnd => json!(["q"]),
            Ev::Scalar(s) => {
                let st = match s.style {
                    Style::Plain => "p",
                    Style::Single => "'",
                    Style::Double => "\"",
                    Style::Literal => "|",
                    Style::Folded => ">",
                };
                if s.value.is_empty() && s.style == Style::Plain {
                    json!(["S", "", "p", s.tag, 0, 0, false])
                } else {
                    json!(["S", s.value, st, s.tag, s.start, s.end, true])
                }
            }
        })
        .collect())
}

#[test]
fn events_match_pyyaml() {
    let g: Vec<Value> = serde_json::from_slice(&std::fs::read(format!("{}/events_golden.json", common::DATA)).unwrap()).unwrap();
    assert!(g.len() > 300);
    let mut errors = 0;
    for (i, case) in g.iter().enumerate() {
        let text = case["text"].as_str().unwrap();
        let ours = events_json(text);
        if case.get("error").is_some() {
            errors += 1;
            assert!(ours.is_err(), "case {i} must be refused: {text:?}");
        } else {
            let want = case["events"].as_array().unwrap();
            assert_eq!(&ours.unwrap_or_else(|e| panic!("case {i} refused ({e}): {text:?}")), want, "case {i}: {text:?}");
        }
    }
    assert!(errors > 50);
}

//! Writes the units of every fixture as JSON, and the pseudo-translated merge of each, when `DESKTOP_DUMP_OUT`
//! names a folder (used to compare with the LocOS filters, see the report). Does nothing otherwise.
mod common;
use common::*;

#[test]
fn dump_units_for_the_oracle() {
    let Ok(dir) = std::env::var("DESKTOP_DUMP_OUT") else { return };
    let mut m = serde_json::Map::new();
    for (name, bytes) in fixtures() {
        let mut d = extract_any(&name, &bytes).unwrap();
        m.insert(name.clone(), serde_json::json!(texts(&d)));
        pseudo(&mut d);
        std::fs::write(format!("{dir}/merged_{name}"), merge(&d)).unwrap();
    }
    std::fs::write(format!("{dir}/bl.json"), serde_json::Value::Object(m).to_string()).unwrap();
}

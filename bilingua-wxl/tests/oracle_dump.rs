//! Writes the units of every fixture as JSON, and the pseudo-translated merge of each, when `WXL_DUMP_OUT` names
//! a folder (used to compare with the LocOS filter, see the report). Does nothing otherwise.
mod common;
use common::*;
use std::collections::BTreeMap;

#[test]
fn dump_units_for_the_oracle() {
    let Ok(dir) = std::env::var("WXL_DUMP_OUT") else { return };
    let mut m = serde_json::Map::new();
    let mut idx: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (name, bytes) in fixtures() {
        let mut d = ex(bytes);
        m.insert(name.into(), serde_json::json!(texts(&d)));
        idx.insert(name.into(), d.units.iter().map(|u| u.context.extra["p_index"].clone()).collect());
        pseudo(&mut d);
        std::fs::write(format!("{dir}/merged_{name}.bin"), merged(&d)).unwrap();
    }
    m.insert("p_index".into(), serde_json::json!(idx));
    std::fs::write(format!("{dir}/bl.json"), serde_json::Value::Object(m).to_string()).unwrap();
}

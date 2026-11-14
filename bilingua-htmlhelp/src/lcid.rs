//! Windows LCIDs for the `Language=` line of a project file (the tables come from `tests/tools/gen_tables.py`).

use crate::data::LCIDS;
use bilingua_model::codepage::{split_locale, target_code_page};
use std::collections::HashMap;
use std::sync::OnceLock;

struct Maps {
    by_name: HashMap<String, u32>,
}

fn maps() -> &'static Maps {
    static M: OnceLock<Maps> = OnceLock::new();
    M.get_or_init(|| {
        let mut by_name = HashMap::new();
        for (lcid, name, _) in LCIDS {
            if *lcid < 0x100 || !name.contains('_') {
                continue; // neutral LCIDs (0x4 zh_CHS) are not what HHW writes
            }
            by_name.entry(name.to_lowercase()).or_insert(*lcid);
            by_name.entry(name.split('_').next().unwrap_or("").to_lowercase()).or_insert(*lcid);
        }
        Maps { by_name }
    })
}

fn display(lcid: u32) -> &'static str {
    LCIDS.iter().find(|(l, _, _)| *l == lcid).map(|(_, _, d)| *d).unwrap_or("")
}

/// (LCID, English display name) of a target locale, as HTML Help Workshop writes `Language=`.
pub fn target_lcid(locale: &str) -> Option<(u32, String)> {
    let (lang, script, mut region) = split_locale(locale);
    if lang.is_empty() {
        return None;
    }
    if lang == "zh" && region.is_empty() {
        region = if script == "Hant" { "TW".into() } else { "CN".into() };
    }
    let m = maps();
    let lcid = (!region.is_empty()).then(|| m.by_name.get(&format!("{lang}_{region}").to_lowercase()).copied()).flatten().or_else(|| m.by_name.get(&lang).copied())?;
    Some((lcid, display(lcid).to_string()))
}

/// The ANSI code page of a `Language=` value such as `0x419 Russian`.
pub fn code_page_of_language(value: &str) -> Option<u16> {
    let token = value.split_whitespace().next()?;
    let lcid = match token.strip_prefix("0x").or_else(|| token.strip_prefix("0X")) {
        Some(h) => u32::from_str_radix(h, 16).ok()?,
        None => token.parse::<u32>().ok()?,
    };
    let posix = LCIDS.iter().find(|(l, _, _)| *l == lcid).map(|(_, n, _)| *n)?;
    target_code_page(posix)
}

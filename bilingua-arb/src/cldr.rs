//! CLDR plural and ordinal categories per language, from a table generated from Babel (the data Localization OS
//! uses), so a target language gets exactly the categories LocOS would give it.

include!("cldr_table.rs");

pub const ORDER: [&str; 6] = ["zero", "one", "two", "few", "many", "other"];

/// The categories the language uses, in CLDR order, always with `other`. `None` for a language the table does
/// not know (callers then leave the plural forms as they are).
pub fn categories(locale: &str, ordinal: bool) -> Option<Vec<&'static str>> {
    let mask = lookup(locale, ordinal)?;
    Some(ORDER.iter().enumerate().filter(|(i, _)| mask & (1 << i) != 0).map(|(_, c)| *c).collect())
}

fn lookup(locale: &str, ordinal: bool) -> Option<u8> {
    let code = locale.trim().replace('-', "_");
    let mut parts = code.split('_');
    let lang = parts.next()?.to_ascii_lowercase();
    if lang.is_empty() || !lang.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    let (mut script, mut terr) = (None, None);
    for p in parts {
        if p.len() == 4 && p.bytes().all(|b| b.is_ascii_alphabetic()) {
            let mut c = p.to_ascii_lowercase();
            c[..1].make_ascii_uppercase();
            script = Some(c);
        } else if (p.len() == 2 && p.bytes().all(|b| b.is_ascii_alphabetic())) || (p.len() == 3 && p.bytes().all(|b| b.is_ascii_digit())) {
            terr = Some(p.to_ascii_uppercase());
        }
    }
    let mut cands = Vec::new();
    if let (Some(s), Some(t)) = (&script, &terr) {
        cands.push(format!("{lang}_{s}_{t}"));
    }
    if let Some(t) = &terr {
        cands.push(format!("{lang}_{t}"));
    }
    if let Some(s) = &script {
        cands.push(format!("{lang}_{s}"));
    }
    cands.push(lang.clone());
    for c in cands {
        if let Ok(i) = TABLE.binary_search_by(|(k, _, _)| (*k).cmp(c.as_str())) {
            let (_, card, ord) = TABLE[i];
            return Some(if ordinal { ord } else { card });
        }
    }
    // Babel also resolves aliases (`deu`, `iw`, `tl`, `und`): LocOS fills plurals for them, so this crate does too.
    let i = ALIASES.binary_search_by(|(k, _, _)| (*k).cmp(lang.as_str())).ok()?;
    let (_, card, ord) = ALIASES[i];
    Some(if ordinal { ord } else { card })
}

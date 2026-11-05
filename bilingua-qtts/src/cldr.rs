//! How many plural forms a Qt numerus message has in a language: the CLDR categories whole numbers reach, from a
//! table generated from Babel (the data Localization OS uses), so a target language gets exactly the forms LocOS
//! would give it. Unlike the cardinal table of the ARB crate, a category that only fractions reach (`other` in
//! Polish and Russian, `many` in Czech) does not count: a Qt message counts whole items.

include!("cldr_table.rs");

/// Number of plural forms for `locale`, or 2 (`one`, `other`) for an unknown or empty locale, as LocOS.
pub fn forms(locale: &str) -> usize {
    lookup(locale).map(|m| m.count_ones() as usize).unwrap_or(2)
}

fn lookup(locale: &str) -> Option<u8> {
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
        if let Ok(i) = TABLE.binary_search_by(|(k, _)| (*k).cmp(c.as_str())) {
            return Some(TABLE[i].1);
        }
    }
    let i = ALIASES.binary_search_by(|(k, _)| (*k).cmp(lang.as_str())).ok()?;
    Some(ALIASES[i].1)
}

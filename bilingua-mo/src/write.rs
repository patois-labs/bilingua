//! Writing a compiled catalog the way Localization OS does: little-endian revision 0, originals sorted bytewise,
//! no hash table (S = 0 is valid; libintl then binary-searches the sorted table).

use crate::read::LE_MAGIC;
use bilingua_model::Error;
use std::collections::BTreeMap;

fn word(v: usize) -> Result<[u8; 4], Error> {
    u32::try_from(v).map(u32::to_le_bytes).map_err(|_| Error::Write("the catalog is too large for the MO format".into()))
}

/// `pairs` maps each original (with its context and plural form already joined) to its translation.
pub fn compile(pairs: &BTreeMap<Vec<u8>, Vec<u8>>) -> Result<Vec<u8>, Error> {
    let n = pairs.len();
    let o = 7 * 4;
    let t = o + 8 * n;
    let strings: usize = pairs.iter().map(|(k, v)| k.len() + v.len() + 2).sum();
    let end = t + 8 * n + strings;
    let mut head = Vec::with_capacity(end);
    for w in [LE_MAGIC as usize, 0, n, o, t, 0, end] {
        head.extend_from_slice(&word(w)?);
    }
    let mut tables = Vec::with_capacity(16 * n);
    let mut blob = Vec::with_capacity(strings);
    let mut pos = t + 8 * n;
    for k in pairs.keys() {
        tables.extend_from_slice(&word(k.len())?);
        tables.extend_from_slice(&word(pos)?);
        blob.extend_from_slice(k);
        blob.push(0);
        pos += k.len() + 1;
    }
    for v in pairs.values() {
        tables.extend_from_slice(&word(v.len())?);
        tables.extend_from_slice(&word(pos)?);
        blob.extend_from_slice(v);
        blob.push(0);
        pos += v.len() + 1;
    }
    head.extend_from_slice(&tables);
    head.extend_from_slice(&blob);
    Ok(head)
}

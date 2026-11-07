//! Writing a translated EBU STL file: text preparation, row layout, encoding in a code table, extension blocks.
//! Follows Localization OS (`stl_filter.py`) step by step.

use crate::codec::{self, Cct};
use crate::read::{self, Atom, Sub, GSI_SIZE, TF_SIZE, TTI_SIZE};
use bilingua_model::{py_isspace_char, Error};
use std::collections::{BTreeMap, BTreeSet};

pub fn refuse(msg: impl Into<String>) -> Error {
    Error::Unwritable { unit: None, reason: msg.into() }
}

/// Python `repr` of one character, close enough for error messages.
pub fn py_repr(c: char) -> String {
    match c {
        '\'' => "\"'\"".into(),
        '\\' => "'\\\\'".into(),
        '\n' => "'\\n'".into(),
        '\r' => "'\\r'".into(),
        '\t' => "'\\t'".into(),
        c if (c as u32) < 0x20 || c as u32 == 0x7F => format!("'\\x{:02x}'", c as u32),
        c if c.is_control() || (c.is_whitespace() && c != ' ') => {
            if (c as u32) < 0x100 {
                format!("'\\x{:02x}'", c as u32)
            } else if (c as u32) < 0x10000 {
                format!("'\\u{:04x}'", c as u32)
            } else {
                format!("'\\U{:08x}'", c as u32)
            }
        }
        c => format!("'{c}'"),
    }
}

/// NFC, then fold the characters the table cannot hold (thin space, ellipsis, typographic quotes).
pub fn prep(atoms: &[Atom], cct: Cct) -> Vec<Atom> {
    fn flush(run: &mut String, out: &mut Vec<Atom>, cct: Cct) {
        if run.is_empty() {
            return;
        }
        for c in codec::nfc(run).chars() {
            match codec::fold(c) {
                Some(rep) if codec::encode_char(c, cct).is_none() => out.extend(rep.chars().map(Atom::C)),
                _ => out.push(Atom::C(c)),
            }
        }
        run.clear();
    }
    let mut out = Vec::with_capacity(atoms.len());
    let mut run = String::new();
    for a in atoms {
        match a {
            Atom::C(c) => run.push(*c),
            other => {
                flush(&mut run, &mut out, cct);
                out.push(*other);
            }
        }
    }
    flush(&mut run, &mut out, cct);
    out
}

/// The distinct characters of a text (after `prep`) the table cannot encode, in order of appearance.
pub fn unsupported(atoms: &[Atom], cct: Cct) -> Vec<char> {
    unsupported_with(atoms, cct, false)
}

/// `pairs`: a base letter and its floating diacritic with no single code are written as a byte pair (LocOS refuses them).
pub fn unsupported_with(atoms: &[Atom], cct: Cct, pairs: bool) -> Vec<char> {
    // a set beside the ordered list: a translation can hold 100 000 distinct characters (gate: was quadratic)
    let mut seen: Vec<char> = Vec::new();
    let mut set: BTreeSet<char> = BTreeSet::new();
    let prepped = prep(atoms, cct);
    for (i, a) in prepped.iter().enumerate() {
        if let Atom::C(c) = *a {
            // a base letter with no single code and its mark are written as a pair
            if pairs && i > 0 && matches!(prepped[i - 1], Atom::C(b) if codec::encode_pair(b, c, cct).is_some()) {
                continue;
            }
            if c != '\n' &&!set.contains(&c) && codec::encode_char(c, cct).is_none() {
                set.insert(c);
                seen.push(c);
            }
        }
    }
    seen
}

fn spacing(b: u8) -> bool {
    b < 0x20 // teletext control codes occupy a character cell
}

fn width<'a>(it: impl Iterator<Item = &'a Atom>) -> usize {
    let (mut n, mut prev_space) = (0, false);
    for a in it {
        match a {
            Atom::C(c) => {
                n += 1;
                prev_space = *c == ' ';
            }
            Atom::T(b) => {
                if spacing(*b) && !prev_space {
                    n += 1;
                }
                prev_space = false;
            }
            Atom::U => {}
        }
    }
    n
}

fn is_ws(a: &Atom) -> bool {
    matches!(a, Atom::C(c) if py_isspace_char(*c))
}

fn strip(r: &[Atom]) -> &[Atom] {
    let s = r.iter().position(|a| !is_ws(a)).unwrap_or(r.len());
    let e = r.iter().rposition(|a| !is_ws(a)).map_or(s, |p| p + 1);
    &r[s..e]
}

/// Greedy wrap; `None` as soon as it needs more than `limit` rows. `widths[i]` is the width of word i on its own,
/// `spaced[i]` its width after a joining space (the space included): a space resets the "previous was a space"
/// state of `width`, so a candidate row's width is the row's width plus `spaced[i]` (gate: no re-measure of the
/// whole row per word, which was quadratic in zero-width codes).
fn wrap(words: &[&[Atom]], widths: &[usize], spaced: &[usize], w: usize, limit: usize) -> Option<Vec<Vec<Atom>>> {
    let mut lines: Vec<Vec<Atom>> = Vec::new();
    let mut cur: Vec<Atom> = Vec::new();
    let mut cur_w = 0usize;
    for (i, word) in words.iter().enumerate() {
        if cur.is_empty() {
            cur.extend_from_slice(word);
            cur_w = widths[i];
            continue;
        }
        if cur_w + spaced[i] <= w {
            cur.push(Atom::C(' '));
            cur.extend_from_slice(word);
            cur_w += spaced[i];
        } else {
            lines.push(std::mem::take(&mut cur));
            if lines.len() >= limit {
                return None;
            }
            cur.extend_from_slice(word);
            cur_w = widths[i];
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    (lines.len() <= limit).then_some(lines)
}

/// Rows for a translation: the translator's own breaks when they fit, else re-wrapped; `None` when it cannot fit.
pub fn layout(text: &[Atom], n_rows: usize, max_rows: usize, mnc: usize) -> Option<Vec<Vec<Atom>>> {
    let own: Vec<&[Atom]> = text.split(|a| *a == Atom::C('\n')).map(strip).filter(|r| !r.is_empty()).collect();
    if !own.is_empty() && own.len() <= max_rows && own.iter().all(|r| width(r.iter()) <= mnc) {
        return Some(own.iter().map(|r| r.to_vec()).collect());
    }
    let words: Vec<&[Atom]> = own.iter().flat_map(|r| r.split(is_ws).filter(|w| !w.is_empty())).collect();
    let widths: Vec<usize> = words.iter().map(|w| width(w.iter())).collect();
    if words.is_empty() || widths.iter().copied().max().unwrap_or(0) > mnc {
        return None;
    }
    if widths.iter().sum::<usize>() > mnc.saturating_mul(max_rows) {
        return None; // cannot fit whatever the breaks: refuse now, not after a re-wrap per width
    }
    let spaced: Vec<usize> = words.iter().map(|w| width(std::iter::once(&Atom::C(' ')).chain(w.iter()))).collect();
    let target = n_rows.min(max_rows).max(1);
    for w in 1..=mnc {
        if let Some(rows) = wrap(&words, &widths, &spaced, w, target) {
            return Some(rows);
        }
    }
    wrap(&words, &widths, &spaced, mnc, max_rows)
}

fn encode_atoms(atoms: &[Atom], cct: Cct, pairs: bool) -> Option<Vec<u8>> {
    let kept: Vec<&Atom> = atoms.iter().filter(|a| **a != Atom::U).collect();
    let mut out: Vec<u8> = Vec::with_capacity(kept.len());
    let mut skip_mark = false;
    for (i, a) in kept.iter().enumerate() {
        if std::mem::take(&mut skip_mark) {
            continue;
        }
        match a {
            Atom::T(b) => {
                if spacing(*b) && i > 0 && *kept[i - 1] == Atom::C(' ') && out.last() == Some(&0x20) {
                    out.pop(); // the control code takes the space's cell
                }
                out.push(*b);
            }
            Atom::C(c) => {
                if let (true, Some(Atom::C(m)), Some(_)) = (pairs, kept.get(i + 1), codec::encode_char(*c, cct)) {
                    if let Some(pair) = codec::encode_pair(*c, *m, cct) {
                        out.extend_from_slice(&pair);
                        skip_mark = true;
                        continue;
                    }
                }
                let (bytes, n) = codec::encode_char(*c, cct)?;
                out.extend_from_slice(&bytes[..n]);
            }
            Atom::U => {}
        }
    }
    Some(out)
}

/// (TF bytes, row delta) or the reason it cannot be written.
fn encode_subtitle(sub: &Sub, text: &[Atom], cct: Cct, mnc: usize, keep_rows: bool, pairs: bool) -> Result<(Vec<u8>, i64), String> {
    let text = prep(text, cct);
    let bad = unsupported_with(&text, cct, pairs);
    if !bad.is_empty() {
        let list: Vec<String> = bad.iter().take(8).map(|c| py_repr(*c)).collect();
        return Err(format!("characters the code table cannot encode: {}", list.join(" ")));
    }
    let n = sub.rows.len();
    let rows: Vec<Vec<Atom>> = if keep_rows {
        text.split(|a| *a == Atom::C('\n')).map(|r| r.to_vec()).collect()
    } else {
        let max_rows = n.max(2);
        layout(&text, n, max_rows, mnc).ok_or_else(|| format!("does not fit in {max_rows} rows of {mnc} characters"))?
    };
    let m = rows.len();
    let mut out = sub.head.clone();
    for (i, row_text) in rows.iter().enumerate() {
        let j = i as i64 - (m as i64 - n as i64);
        let tpl = if j >= 0 { sub.rows.get(j as usize) } else { sub.rows.first() };
        let Some(tpl) = tpl else { return Err("the subtitle has no text rows".into()) };
        if i > 0 {
            if j >= 1 {
                out.extend_from_slice(sub.seps.get(j as usize - 1).map_or(&[][..], |v| v));
            } else if let Some(s) = sub.seps.first() {
                out.extend_from_slice(s);
            } else if tpl.prefix.contains(&0x0D) || tpl.prefix.contains(&0x0F) {
                out.extend_from_slice(&[0x8A, 0x8A]);
            } else {
                out.push(0x8A);
            }
        }
        out.extend_from_slice(&tpl.prefix);
        out.extend(encode_atoms(row_text, cct, pairs).ok_or("a character the code table cannot encode")?);
        out.extend_from_slice(&tpl.suffix);
    }
    out.extend_from_slice(&sub.tail);
    let step = if sub.double_height(0) { 2 } else { 1 };
    Ok((out, (m as i64 - n as i64) * step))
}

/// Split a text field into TTI blocks; `headers` are the original 16-byte block headers.
fn blocks_for(tf: &[u8], headers: &[[u8; 16]], cct: Cct) -> Option<Vec<u8>> {
    let mut data = tf.to_vec();
    data.push(0x8F); // the last block's TF always ends with 8Fh
    let mut chunks: Vec<&[u8]> = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let mut end = (i + TF_SIZE).min(data.len());
        if end < data.len() && cct.is_latin() && codec::diacritic(data[end - 1]).is_some() {
            end -= 1; // never split a diacritic from its letter
        }
        chunks.push(&data[i..end]);
        i = end;
    }
    if chunks.len() > 0xF0 {
        return None;
    }
    let mut out = Vec::with_capacity(chunks.len() * TTI_SIZE);
    for (k, chunk) in chunks.iter().enumerate() {
        let mut hdr = *headers.get(k).or(headers.first())?;
        hdr[3] = if k == chunks.len() - 1 { 0xFF } else { k as u8 };
        out.extend_from_slice(&hdr);
        out.extend_from_slice(chunk);
        out.extend(std::iter::repeat_n(0x8Fu8, TF_SIZE - chunk.len()));
    }
    Some(out)
}

pub struct UnitInfo {
    pub group: usize,
    pub sub: Sub,
    pub atoms: Vec<Atom>,
}

/// Rebuild the file. `changed` maps unit index to the new text atoms; `lc` is the language byte to stamp.
pub fn rebuild(raw: &[u8], parsed: &read::Parsed, units: &[UnitInfo], changed: &BTreeMap<usize, Vec<Atom>>, lc: Option<u8>, pairs: bool) -> Result<Vec<u8>, Error> {
    let cct = parsed.cct;
    let mnc = read::mnc(raw);
    let fits = |cand: Cct| changed.values().all(|t| unsupported_with(t, cand, pairs).is_empty());
    let mut problems: Vec<String> = Vec::new();
    // the units already named in `problems` (gate: a prefix search of `problems` per refused unit was quadratic)
    let mut reported: BTreeSet<usize> = BTreeSet::new();
    let chosen = if fits(cct) {
        cct
    } else {
        let mut usable = vec![cct];
        for c in Cct::ALL {
            if c != cct && !units.iter().enumerate().any(|(k, u)| !changed.contains_key(&k) && !unsupported_with(&u.atoms, c, pairs).is_empty()) {
                usable.push(c);
            }
        }
        match usable.iter().copied().find(|c| fits(*c)) {
            Some(c) => c,
            None => {
                let cost = |c: Cct| changed.values().map(|t| unsupported_with(t, c, pairs).len()).sum::<usize>();
                let best = usable.iter().copied().min_by_key(|c| cost(*c)).unwrap_or(cct);
                for (k, t) in changed {
                    let bad = unsupported_with(t, best, pairs);
                    if !bad.is_empty() {
                        reported.insert(*k);
                        let list: Vec<String> = bad.iter().take(8).map(|c| py_repr(*c)).collect();
                        problems.push(format!(
                            "subtitle {}: characters no EBU STL code table (00-04) can hold together with the rest of the file: {}",
                            k + 1,
                            list.join(" ")
                        ));
                    }
                }
                best
            }
        }
    };
    let mut new_tf: BTreeMap<usize, (usize, Vec<u8>, i64)> = BTreeMap::new(); // first block -> (unit, tf, dvp)
    for (k, u) in units.iter().enumerate() {
        let r = if let Some(t) = changed.get(&k) {
            encode_subtitle(&u.sub, t, chosen, mnc, false, pairs)
        } else if chosen != cct {
            encode_subtitle(&u.sub, &u.atoms, chosen, mnc, true, pairs)
        } else {
            continue;
        };
        match r {
            Ok((tf, dvp)) => {
                let first = parsed.groups[u.group].blocks[0];
                new_tf.insert(first, (k, tf, dvp));
            }
            Err(why) => {
                if reported.insert(k) {
                    problems.push(format!("subtitle {}: {why}", k + 1));
                }
            }
        }
    }
    if !problems.is_empty() {
        let more = if problems.len() > 10 { format!(" (and {} more)", problems.len() - 10) } else { String::new() };
        return Err(refuse(format!(
            "The EBU STL file was not exported: {}{more}. Shorten these subtitles (or check the target language) and export again.",
            problems.iter().take(10).cloned().collect::<Vec<_>>().join("; ")
        )));
    }
    let teletext = matches!(raw[11], b'1' | b'2');
    let mut skip: BTreeSet<usize> = BTreeSet::new();
    let mut body: Vec<u8> = Vec::with_capacity(raw.len() - GSI_SIZE);
    let mut count = 0usize;
    for i in 0..parsed.nblocks {
        if skip.contains(&i) {
            continue;
        }
        if let Some((k, tf, dvp)) = new_tf.get(&i) {
            let g = &parsed.groups[units[*k].group];
            let mut headers: Vec<[u8; 16]> = g
                .blocks
                .iter()
                .map(|b| {
                    let mut h = [0u8; 16];
                    h.copy_from_slice(&read::block(raw, *b)[..16]);
                    h
                })
                .collect();
            if *dvp != 0 {
                let vp = headers[0][13] as i64 - dvp;
                headers[0][13] = vp.min(0xFF).max(if teletext { 1 } else { 0 }) as u8;
            }
            let built = blocks_for(tf, &headers, chosen)
                .ok_or_else(|| refuse(format!("The EBU STL file was not exported: subtitle at block {} needs more than 240 extension blocks.", i + 1)))?;
            count += built.len() / TTI_SIZE;
            body.extend_from_slice(&built);
            skip.extend(g.blocks.iter().copied());
            continue;
        }
        body.extend_from_slice(read::block(raw, i));
        count += 1;
    }
    let mut gsi = raw[..GSI_SIZE].to_vec();
    if chosen != cct {
        gsi[12..14].copy_from_slice(&chosen.digits());
    }
    if count != parsed.nblocks {
        if count > read::MAX_TTI_BLOCKS {
            return Err(refuse("The EBU STL file was not exported: over 99999 blocks."));
        }
        gsi[238..243].copy_from_slice(format!("{count:05}").as_bytes());
    }
    if let Some(lc) = lc {
        gsi[14..16].copy_from_slice(format!("{lc:02X}").as_bytes());
    }
    gsi.extend_from_slice(&body);
    Ok(gsi)
}

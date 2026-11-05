//! The group-code walk of an ASCII DXF file: pairs of lines (code, value), the sections, and the groups that hold
//! drawing text. Byte spans only; nothing is decoded here.

use crate::lex::Kind;
use bilingua_model::*;

const BINARY_SENTINEL: &[u8] = b"AutoCAD Binary DXF";

/// One group: spans in the file.
#[derive(Debug, Clone, Copy)]
pub struct Pair {
    pub code: i32,
    /// Start of the line that holds the code (leading blanks included).
    pub start: usize,
    /// End of the code digits.
    pub code_e: usize,
    pub val_s: usize,
    pub val_e: usize,
    /// End of the value line's line break (or of the file).
    pub end: usize,
}

/// A unit of drawing text: one value, or the 1/3 groups of one MTEXT.
#[derive(Debug)]
pub struct RawUnit {
    pub kind: Kind,
    pub style: &'static str,
    pub pairs: Vec<Pair>,
    /// Zero-based index of the unit's first group, for the location.
    pub group: usize,
}

pub struct Walked {
    pub units: Vec<RawUnit>,
    pub acadver: Option<String>,
    pub codepage: Option<String>,
}

pub fn is_py_ws(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

pub fn py_strip(b: &[u8]) -> &[u8] {
    let s = b.iter().position(|&c| !is_py_ws(c)).unwrap_or(b.len());
    let e = b.iter().rposition(|&c| !is_py_ws(c)).map_or(s, |p| p + 1);
    &b[s..e]
}

/// One group at `pos`: blanks, a code of one to four digits (optionally negative), blanks, a line break, the value
/// line and its line break or the end of the file.
pub fn match_pair(data: &[u8], pos: usize) -> Option<Pair> {
    let n = data.len();
    let mut p = pos;
    while p < n && (data[p] == b' ' || data[p] == b'\t') {
        p += 1;
    }
    let neg = p < n && data[p] == b'-';
    if neg {
        p += 1;
    }
    let ds = p;
    while p < n && data[p].is_ascii_digit() && p - ds < 5 {
        p += 1;
    }
    let digits = p - ds;
    if digits == 0 || digits > 4 {
        return None;
    }
    let code_e = p;
    let mut code: i32 = 0;
    for &d in &data[ds..p] {
        code = code * 10 + i32::from(d - b'0');
    }
    if neg {
        code = -code;
    }
    while p < n && (data[p] == b' ' || data[p] == b'\t') {
        p += 1;
    }
    p += newline_len(data, p)?;
    let val_s = p;
    while p < n && data[p] != b'\r' && data[p] != b'\n' {
        p += 1;
    }
    let val_e = p;
    p += newline_len(data, p).unwrap_or(0);
    Some(Pair { code, start: pos, code_e, val_s, val_e, end: p })
}

fn newline_len(data: &[u8], p: usize) -> Option<usize> {
    match data.get(p) {
        Some(b'\r') => Some(if data.get(p + 1) == Some(&b'\n') { 2 } else { 1 }),
        Some(b'\n') => Some(1),
        _ => None,
    }
}

fn trailing_ok(data: &[u8], pos: usize) -> bool {
    data[pos..].iter().all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n' | 0x1a | 0x00))
}

fn malformed(msg: String) -> Error {
    Error::Parse { format: "dxf".into(), msg }
}

pub fn is_binary(data: &[u8]) -> bool {
    let s = data.iter().position(|&c| !is_py_ws(c)).unwrap_or(data.len());
    data[s..].starts_with(BINARY_SENTINEL)
}

#[derive(Clone, Copy, PartialEq)]
enum Ent {
    Text,
    Mtext,
    Attrib,
    Attdef,
    Dimension,
    Mleader,
}

fn ascii_string(b: &[u8]) -> String {
    b.iter().map(|&c| if c < 0x80 { (c as char).to_ascii_uppercase() } else { '\u{FFFD}' }).collect()
}

pub fn walk(data: &[u8], limits: &Limits) -> Result<Walked, Error> {
    if is_binary(data) {
        return Err(Error::Unsupported(
            "This is a binary DXF file. Only ASCII DXF can be translated: in AutoCAD use SAVEAS, pick DXF and set the format to ASCII (not Binary)."
                .into(),
        ));
    }
    let budget = 200 * limits.max_units;
    let breaks = data.iter().filter(|&&b| b == b'\n' || b == b'\r').count();
    if breaks > 2 * budget {
        return Err(LimitError::new(
            LimitKind::Lines,
            format!("This DXF file has more than {budget} group codes (200 x the segment limit). Split the drawing or export only the layouts you need."),
        )
        .into());
    }
    let mut pos = if data.starts_with(&[0xEF, 0xBB, 0xBF]) { 3 } else { 0 };
    let n = data.len();
    let mut units: Vec<RawUnit> = Vec::new();
    let mut section: Option<Vec<u8>> = None;
    let mut expect_section_name = false;
    let mut header_var: Vec<u8> = Vec::new();
    let mut acadver = None;
    let mut codepage = None;
    let mut entity: Option<Ent> = None;
    let mut after101 = false;
    let mut mt_unit: Option<usize> = None;
    let mut first = true;
    let mut group = 0usize;
    while pos < n {
        let Some(p) = match_pair(data, pos) else {
            if trailing_ok(data, pos) {
                break;
            }
            let line = data[..pos].iter().filter(|&&b| b == b'\n').count() + 1;
            return Err(malformed(format!("line {line} is not a group code followed by a value.")));
        };
        let this_group = group;
        group += 1;
        if first {
            first = false;
            if p.code != 0 && p.code != 999 {
                return Err(malformed("it does not start with a group code 0 or 999.".into()));
            }
        }
        pos = p.end;
        let val = &data[p.val_s..p.val_e];
        let code = p.code;
        if code == 0 {
            let v = py_strip(val);
            expect_section_name = v == b"SECTION";
            if v == b"SECTION" || v == b"ENDSEC" || v == b"EOF" {
                if v != b"SECTION" {
                    section = None;
                }
                entity = None;
            } else if matches!(section.as_deref(), Some(b"ENTITIES") | Some(b"BLOCKS")) {
                let name = ascii_string(v);
                entity = match name.as_str() {
                    "TEXT" => Some(Ent::Text),
                    "MTEXT" => Some(Ent::Mtext),
                    "ATTRIB" => Some(Ent::Attrib),
                    "ATTDEF" => Some(Ent::Attdef),
                    "DIMENSION" => Some(Ent::Dimension),
                    "MULTILEADER" | "MLEADER" => Some(Ent::Mleader),
                    _ => None,
                };
            } else {
                entity = None;
            }
            after101 = false;
            mt_unit = None;
            if v == b"EOF" {
                break;
            }
            continue;
        }
        if expect_section_name && code == 2 {
            section = Some(ascii_string(py_strip(val)).into_bytes());
            expect_section_name = false;
            continue;
        }
        if section.as_deref() == Some(b"HEADER") {
            if code == 9 {
                header_var = ascii_string(py_strip(val)).into_bytes();
            } else if header_var == b"$ACADVER" && code == 1 {
                acadver = Some(ascii_string(py_strip(val)));
            } else if header_var == b"$DWGCODEPAGE" && code == 3 {
                codepage = Some(ascii_string(py_strip(val)));
            }
            continue;
        }
        let Some(ent) = entity else { continue };
        if code >= 1000 {
            continue;
        }
        if code == 101 {
            after101 = true;
            mt_unit = None;
            continue;
        }
        if ent == Ent::Mtext || (after101 && matches!(ent, Ent::Attrib | Ent::Attdef)) {
            if after101 && ent == Ent::Mtext {
                continue;
            }
            if code == 1 || code == 3 {
                let idx = match mt_unit {
                    Some(i) => i,
                    None => {
                        units.push(RawUnit { kind: Kind::Mtext, style: "dxf:mtext", pairs: Vec::new(), group: this_group });
                        guard::check_unit_count(units.len(), limits)?;
                        mt_unit = Some(units.len() - 1);
                        units.len() - 1
                    }
                };
                units[idx].pairs.push(p);
            }
            continue;
        }
        let made = match (ent, code) {
            (Ent::Text, 1) => Some((Kind::Text, "dxf:text")),
            (Ent::Attrib, 1) => Some((Kind::Text, "dxf:attrib")),
            (Ent::Attdef, 1) => Some((Kind::Text, "dxf:attdef")),
            (Ent::Attdef, 3) => Some((Kind::Text, "dxf:prompt")),
            (Ent::Dimension, 1) => Some((Kind::Dim, "dxf:dimension")),
            (Ent::Mleader, 304) => Some((Kind::Mtext, "dxf:mleader")),
            _ => None,
        };
        if let Some((kind, style)) = made {
            units.push(RawUnit { kind, style, pairs: vec![p], group: this_group });
            guard::check_unit_count(units.len(), limits)?;
        }
    }
    Ok(Walked { units, acadver, codepage })
}

//! The region sidecar: the JSON list LocOS's mock detector reads from a PNG (`loc_tms_regions`), normalised into
//! the geometry contract (`normalize_region`), in reading order, at most 100 regions.

use crate::json::{self, Node, Val};
use crate::refuse;
use bilingua_model::{py_isspace_char, Error};

pub const KEY: &str = "loc_tms_regions";
pub const MAX_REGIONS: usize = 100;

#[derive(Debug, Clone)]
pub struct Region {
    pub text: String,
    /// `[x, y, w, h]` in pixels.
    pub bbox: [i64; 4],
    pub polygon: Vec<[i64; 2]>,
    pub angle: f64,
    pub confidence: f64,
    /// Byte span of the `text` string literal in the sidecar JSON.
    pub text_span: (usize, usize),
}

/// Python `float(str)`.
fn py_float_str(s: &str) -> Option<f64> {
    let t = s.trim_matches(py_isspace_char);
    if t.contains('_') {
        let b: Vec<char> = t.chars().collect();
        for (i, c) in b.iter().enumerate() {
            if *c == '_' && !(i > 0 && i + 1 < b.len() && b[i - 1].is_ascii_digit() && b[i + 1].is_ascii_digit()) {
                return None;
            }
        }
    }
    let u: String = t.chars().filter(|c| *c != '_').collect();
    if u.is_empty() || u.contains(char::is_whitespace) {
        return None;
    }
    u.parse::<f64>().ok()
}

/// Python `float(v)` of a JSON value; `None` where Python raises `TypeError` or `ValueError`.
fn py_float(n: &Node) -> Option<f64> {
    match &n.v {
        Val::Num(f) => Some(*f),
        Val::Bool(b) => Some(f64::from(u8::from(*b))),
        Val::Str(s) => py_float_str(s),
        _ => None,
    }
}

/// `int(round(v))`: `Ok(None)` for NaN (Python `ValueError`, the region is skipped), `Err` for an infinity
/// (Python `OverflowError`, which LocOS does not catch: the image is refused).
fn to_int(f: f64) -> Result<Option<i64>, Error> {
    if f.is_nan() {
        return Ok(None);
    }
    if f.is_infinite() {
        return Err(refuse("the image text layer holds a region with an infinite coordinate"));
    }
    Ok(Some(f.round_ties_even().clamp(-9.0e15, 9.0e15) as i64))
}

/// The decimal text of an integer literal (`str(int)`), when the number is written as one and is not zero.
fn int_literal(raw: &str, span: (usize, usize)) -> Option<String> {
    let lit = &raw[span.0..span.1];
    let digits = lit.strip_prefix('-').unwrap_or(lit);
    if digits.is_empty() || !digits.bytes().all(|c| c.is_ascii_digit()) || digits == "0" {
        return None;
    }
    Some(lit.to_string())
}

fn region_of(raw: &str, row: &Node) -> Result<Option<Region>, Error> {
    if !matches!(row.v, Val::Obj(_)) {
        return Ok(None);
    }
    let Some(t) = row.get("text") else { return Ok(None) };
    let text = match &t.v {
        Val::Str(s) => s.clone(),
        // Python `str()` of an integer or a boolean; other values (floats, lists) are skipped here
        Val::Bool(true) => "True".to_string(),
        Val::Num(_) => match int_literal(raw, t.span) {
            Some(s) => s,
            None => return Ok(None),
        },
        _ => return Ok(None),
    };
    if text.is_empty() {
        return Ok(None);
    }
    let Some(bb) = row.get("bbox") else { return Ok(None) };
    let Val::Arr(items) = &bb.v else { return Ok(None) };
    if items.len() != 4 {
        return Ok(None);
    }
    let mut f = [0f64; 4];
    for (i, it) in items.iter().enumerate() {
        match py_float(it) {
            Some(v) => f[i] = v,
            None => return Ok(None),
        }
    }
    let mut bbox = [0i64; 4];
    for i in 0..4 {
        match to_int(f[i])? {
            Some(v) => bbox[i] = v,
            None => return Ok(None),
        }
    }
    let polygon = match row.get("polygon").map(|p| &p.v) {
        None | Some(Val::Null) => {
            let [x, y, w, h] = bbox;
            vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]]
        }
        Some(Val::Arr(pts)) => {
            let mut out = Vec::with_capacity(pts.len());
            for p in pts {
                let Val::Arr(xy) = &p.v else { return Ok(None) };
                if xy.len() != 2 {
                    return Ok(None);
                }
                let mut q = [0i64; 2];
                for (k, c) in xy.iter().enumerate() {
                    let v = match &c.v {
                        Val::Num(v) => *v,
                        Val::Bool(b) => f64::from(u8::from(*b)),
                        _ => return Ok(None),
                    };
                    match to_int(v)? {
                        Some(i) => q[k] = i,
                        None => return Ok(None),
                    }
                }
                out.push(q);
            }
            out
        }
        Some(_) => return Ok(None),
    };
    let angle = match row.get("angle_deg") {
        None => 0.0,
        Some(n) => match py_float(n) {
            Some(v) => v,
            None => return Ok(None),
        },
    };
    let confidence = match row.get("confidence") {
        None => 0.99,
        Some(n) => match py_float(n) {
            Some(v) => v,
            None => return Ok(None),
        },
    };
    Ok(Some(Region { text, bbox, polygon, angle, confidence, text_span: t.span }))
}

/// The regions of a sidecar value, in reading order. Unreadable JSON, or JSON that is not a list, gives none.
pub fn regions(raw: &str) -> Result<Vec<Region>, Error> {
    let Some(root) = json::parse(raw) else { return Ok(Vec::new()) };
    let Val::Arr(rows) = &root.v else { return Ok(Vec::new()) };
    let mut out = Vec::new();
    for r in rows {
        if let Some(reg) = region_of(raw, r)? {
            out.push(reg);
        }
    }
    out.sort_by_key(|r| (r.bbox[1], r.bbox[0]));
    out.truncate(MAX_REGIONS);
    Ok(out)
}

fn num(f: f64) -> String {
    if f.is_nan() {
        "NaN".into()
    } else if f.is_infinite() {
        if f > 0.0 { "Infinity" } else { "-Infinity" }.into()
    } else {
        format!("{f:?}")
    }
}

/// The geometry contract of region `i` as JSON (what LocOS keeps in `Segment.meta["image_region"]`).
pub fn contract_json(i: usize, r: &Region) -> String {
    let poly = |p: &[[i64; 2]]| format!("[{}]", p.iter().map(|q| format!("[{}, {}]", q[0], q[1])).collect::<Vec<_>>().join(", "));
    let [x, y, w, h] = r.bbox;
    format!(
        "{{\"id\": \"r{i}\", \"reading_order_index\": {i}, \"polygon\": {p}, \"bbox_axis_aligned\": [{x}, {y}, {w}, {h}], \"angle_deg\": {a}, \"baseline\": {{\"slope\": 0.0, \"intercept\": {b:?}}}, \"text\": {t}, \"confidence\": {c}, \"lines\": [{{\"text\": {t}, \"polygon\": {p}}}], \"source\": \"mock\"}}",
        p = poly(&r.polygon),
        a = num(r.angle),
        b = (y + h) as f64,
        t = json::ascii_literal(&r.text),
        c = num(r.confidence),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reading_order_and_skips() {
        let raw = r#"[{"text":"b","bbox":[30,100,1,1]},{"text":"a","bbox":[30,20,160,36]},{"text":"","bbox":[1,2,3,4]},{"text":"x","bbox":[1,2,3]},{"text":"y","bbox":["1","2",3.5,true],"confidence":"0.5"}]"#;
        let r = regions(raw).unwrap();
        let t: Vec<&str> = r.iter().map(|r| r.text.as_str()).collect();
        assert_eq!(t, ["y", "a", "b"]);
        assert_eq!(r[0].bbox, [1, 2, 4, 1]);
        assert!(regions(r#"[{"text":"a","bbox":[1e999,0,1,1]}]"#).is_err());
        assert!(regions("{}").unwrap().is_empty() && regions("nope").unwrap().is_empty());
    }
}

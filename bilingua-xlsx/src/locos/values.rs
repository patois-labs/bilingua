//! Cell values as openpyxl reads them, for column mode's header detection, notes and filled-target checks: numbers
//! as Python prints them, dates and durations from the number format (`styles.xml`, the workbook's epoch), ISO dates
//! (`t="d"`), shared-formula dependents translated from their master (see `formula`), merged cells empty.

use super::*;

/// A Python number read from a cell (`_cast_number`): an `int` (`None` when it does not fit an `i128`) or a `float`.
#[derive(Clone, Copy)]
pub(super) enum Num {
    Int(Option<i128>),
    Float(f64),
}

/// The value of an ASCII or other Unicode decimal digit (Unicode keeps every decimal digit run 0..9 in order).
fn digit_value(c: char) -> u32 {
    if c.is_ascii_digit() {
        return c as u32 - '0' as u32;
    }
    let mut start = c as u32;
    while start > 0 && char::from_u32(start - 1).is_some_and(bilingua_model::py_isdecimal_char) {
        start -= 1;
    }
    (c as u32 - start) % 10
}

/// The string with every Unicode decimal digit written as its ASCII digit (Python's `int()` and `float()` read them).
pub(super) fn ascii_digits(s: &str) -> String {
    s.chars().map(|c| if !c.is_ascii() && bilingua_model::py_isdecimal_char(c) { char::from_digit(digit_value(c), 10).unwrap_or(c) } else { c }).collect()
}

/// Python `float()` of a string (white space around, `_` between digits, `inf` / `nan`), `None` when it refuses it.
pub(super) fn py_float(s: &str) -> Option<f64> {
    let t = ascii_digits(bilingua_model::py_strip(s));
    let b = t.as_bytes();
    for (i, &c) in b.iter().enumerate() {
        if c == b'_' && !(i > 0 && b[i - 1].is_ascii_digit() && b.get(i + 1).is_some_and(u8::is_ascii_digit)) {
            return None;
        }
    }
    let t = t.replace('_', "");
    let body = t.trim_start_matches(['+', '-']);
    if t.len() - body.len() > 1 {
        return None;
    }
    if matches!(body.to_ascii_lowercase().as_str(), "inf" | "infinity" | "nan") {
        return t.parse::<f64>().ok();
    }
    if !body.bytes().all(|c| c.is_ascii_digit() || matches!(c, b'.' | b'e' | b'E' | b'+' | b'-')) {
        return None;
    }
    t.parse::<f64>().ok()
}

/// `_cast_number`: a float when the text has `.`, `E` or `e`, else an int; `None` when Python refuses it.
pub(super) fn cast_number(v: &str) -> Option<Num> {
    if v.contains(['.', 'E', 'e']) {
        return py_float(v).map(Num::Float);
    }
    let (neg, digits) = py_int_parts(v)?;
    let digits = ascii_digits(&digits);
    Some(Num::Int(digits.parse::<i128>().ok().map(|n| if neg { -n } else { n })))
}

/// Python's `str(int)` of an `int()` literal: the digits in ASCII, no leading zeros.
pub(super) fn py_int_str(v: &str) -> Option<String> {
    let (neg, digits) = py_int_parts(v)?;
    let digits = ascii_digits(&digits);
    let t = digits.trim_start_matches('0');
    Some(if t.is_empty() {
        "0".into()
    } else if neg {
        format!("-{t}")
    } else {
        t.to_string()
    })
}

/// Python's `repr(float)`: the shortest digits that read back, positional from 1e-4 up to 1e16, else `1.5e+16`.
pub(super) fn py_float_repr(f: f64) -> String {
    if f.is_nan() {
        return "nan".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "inf".into() } else { "-inf".into() };
    }
    if f == 0.0 {
        return if f.is_sign_negative() { "-0.0".into() } else { "0.0".into() };
    }
    let e = format!("{f:e}");
    let (mant, exp) = e.split_once('e').unwrap_or((&e, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let sign = if mant.starts_with('-') { "-" } else { "" };
    let digits: String = mant.chars().filter(char::is_ascii_digit).collect();
    let n = digits.len() as i32;
    if (-4..16).contains(&exp) {
        if exp >= 0 {
            if n <= exp + 1 {
                format!("{sign}{digits}{}.0", "0".repeat((exp + 1 - n) as usize))
            } else {
                format!("{sign}{}.{}", &digits[..(exp + 1) as usize], &digits[(exp + 1) as usize..])
            }
        } else {
            format!("{sign}0.{}{digits}", "0".repeat((-exp - 1) as usize))
        }
    } else {
        let m = if n == 1 { digits } else { format!("{}.{}", &digits[..1], &digits[1..]) };
        format!("{sign}{m}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs())
    }
}

/// Python's `str(number)`.
pub(super) fn num_str(n: Num, raw: &str) -> String {
    match n {
        Num::Float(f) => py_float_repr(f),
        Num::Int(_) => py_int_str(raw).unwrap_or_else(|| raw.to_string()),
    }
}

// ------------------------------------------------------------------------------------------- number formats

/// `is_date_format`: the first section, quoted text and `[..]` blocks other than `[h]`, `[mm]`, `[s]` and the like
/// left out, has a `d`, `m`, `h`, `y` or `s` not escaped by `_` or `\`.
fn is_date_format(fmt: &str) -> bool {
    let first: Vec<char> = fmt.split(';').next().unwrap_or("").chars().collect();
    let mut kept: Vec<char> = Vec::new();
    let mut i = 0;
    while i < first.len() {
        let c = first[i];
        if c == '"' {
            // `".*?"`: `.` does not cross a line break
            if let Some(j) = (i + 1..first.len()).take_while(|&j| first[j] != '\n').find(|&j| first[j] == '"') {
                i = j + 1;
                continue;
            }
        } else if c == '[' {
            let rest: String = first[i + 1..first.len().min(i + 4)].iter().collect();
            let kept_block = ["h]", "hh]", "m]", "mm]", "s]", "ss]"].iter().any(|p| rest.starts_with(p));
            if !kept_block {
                if let Some(j) = (i + 1..first.len()).find(|&j| first[j] == ']') {
                    i = j + 1;
                    continue;
                }
            }
        }
        kept.push(c);
        i += 1;
    }
    kept.iter().enumerate().any(|(i, &c)| "dmhysDMHYS".contains(c) && !(i > 0 && (kept[i - 1] == '_' || kept[i - 1] == '\\')))
}

/// `is_timedelta_format`: the first section has `[h]`, `[hh]`, `[m]`, `[mm]`, `[s]` or `[ss]` (any case).
fn is_timedelta_format(fmt: &str) -> bool {
    let first = fmt.split(';').next().unwrap_or("").to_ascii_lowercase();
    ["[h]", "[hh]", "[m]", "[mm]", "[s]", "[ss]"].iter().any(|p| first.contains(p))
}

/// openpyxl's built-in number formats that are dates or times (ids 14 to 22 and 45 to 47).
fn builtin_format(id: i64) -> Option<&'static str> {
    Some(match id {
        14 => "mm-dd-yy",
        15 => "d-mmm-yy",
        16 => "d-mmm",
        17 => "mmm-yy",
        18 => "h:mm AM/PM",
        19 => "h:mm:ss AM/PM",
        20 => "h:mm",
        21 => "h:mm:ss",
        22 => "m/d/yy h:mm",
        45 => "mm:ss",
        46 => "[h]:mm:ss",
        47 => "mmss.0",
        0..=49 => "0",
        _ => return None,
    })
}

/// What the workbook's styles say about cell values (`apply_stylesheet`): how many cell styles there are and which
/// are dates or durations; and the epoch.
pub(super) struct Formats {
    styles: usize,
    dates: HashSet<usize>,
    deltas: HashSet<usize>,
    mac: bool,
}

impl Formats {
    pub(super) fn load(pkg: &mut Package) -> Result<Formats, Error> {
        let mut f = Formats { styles: 1, dates: HashSet::new(), deltas: HashSet::new(), mac: false };
        let wb = pkg.workbook.clone();
        if pkg.load(&wb)? {
            let d = &pkg.roots[&wb];
            if let Some(pr) = d.find(d.root, M, "workbookPr") {
                f.mac = d.get(pr, "date1904").is_some_and(|v| !v.is_empty() && !matches!(v, "false" | "f" | "0"));
            }
        }
        // openpyxl reads the styles from this fixed name, whatever the relationships say
        const STYLES: &str = "xl/styles.xml";
        if !pkg.load(STYLES)? {
            return Ok(f);
        }
        let d = &pkg.roots[STYLES];
        let mut custom: HashMap<i64, String> = HashMap::new();
        if let Some(list) = d.find(d.root, M, "numFmts") {
            for n in d.findall(list, M, "numFmt") {
                let id = d.get(n, "numFmtId").and_then(py_int).ok_or_else(|| invalid(STYLES))?;
                custom.insert(id, d.get(n, "formatCode").unwrap_or("").to_string());
            }
        }
        let xfs: Vec<usize> = d.find(d.root, M, "cellXfs").map(|x| d.findall(x, M, "xf")).unwrap_or_default();
        if xfs.is_empty() {
            return Ok(f); // no cell styles: openpyxl keeps its defaults
        }
        f.styles = xfs.len();
        for (idx, &xf) in xfs.iter().enumerate() {
            let id = match d.get(xf, "numFmtId") {
                Some(v) => py_int(v).ok_or_else(|| invalid(STYLES))?,
                None => 0,
            };
            let fmt = match custom.get(&id) {
                Some(c) => Some(c.as_str()),
                None => builtin_format(id),
            };
            if let Some(fmt) = fmt {
                if is_date_format(fmt) {
                    f.dates.insert(idx);
                }
                if is_timedelta_format(fmt) {
                    f.deltas.insert(idx);
                }
            }
        }
        Ok(f)
    }

    /// The cell's style index as openpyxl uses it: an error for one the workbook does not have (openpyxl fails to
    /// load the workbook), `None` for a negative one (Python counts it from the end: never a date).
    pub(super) fn style_of(&self, s: Option<&str>, name: &str) -> Result<Option<usize>, Error> {
        let Some(s) = s else { return Ok(Some(0)) };
        let i = py_int(s).ok_or_else(|| invalid(name))?;
        if i >= 0 && (i as usize) < self.styles {
            Ok(Some(i as usize))
        } else if i < 0 && i.unsigned_abs() as usize <= self.styles {
            Ok(None)
        } else {
            Err(invalid(name))
        }
    }

    /// `str(value)` of a number cell with this style: a date, time or duration when the style is one, else the
    /// number. A date out of range is openpyxl's error cell `#VALUE!`, a string (so, like any error cell, a unit
    /// when it stands in the source column): the bool says so.
    pub(super) fn number_str(&self, n: Num, raw: &str, style: Option<usize>) -> (String, bool) {
        match style.filter(|s| self.dates.contains(s)) {
            Some(s) => match from_excel(n, self.mac, self.deltas.contains(&s)) {
                Some(v) => (v, false),
                None => ("#VALUE!".into(), true),
            },
            None => (num_str(n, raw), false),
        }
    }
}

// ------------------------------------------------------------------------------------------- dates

const US_PER_DAY: i128 = 86_400_000_000;
const MAX_DELTA_DAYS: i128 = 999_999_999;
/// `date(1899, 12, 30).toordinal()`, `date(1904, 1, 1).toordinal()`, `date(9999, 12, 31).toordinal()`.
const WINDOWS_EPOCH: i64 = 693_594;
const MAC_EPOCH: i64 = 695_056;
const MAX_ORDINAL: i64 = 3_652_059;

/// (year, month, day) of a proleptic Gregorian ordinal (day 1 = 0001-01-01).
fn ymd(ordinal: i64) -> (i64, i64, i64) {
    let z = ordinal - 719_163 + 719_468; // days since 1970-01-01, shifted to 0000-03-01
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 => 29,
        _ => 28,
    }
}

fn frac_us(us: i64) -> String {
    if us == 0 {
        String::new()
    } else {
        format!(".{us:06}")
    }
}

/// `str(datetime.time)`.
fn time_str(h: i64, mi: i64, s: i64, us: i64) -> String {
    format!("{h:02}:{mi:02}:{s:02}{}", frac_us(us))
}

/// `str(datetime.datetime)` of an ordinal and the microseconds into that day.
fn datetime_str(ordinal: i64, us_in_day: i64) -> String {
    let (y, m, d) = ymd(ordinal);
    let s = us_in_day / 1_000_000;
    format!("{y:04}-{m:02}-{d:02} {}", time_str(s / 3600, s / 60 % 60, s % 60, us_in_day % 1_000_000))
}

/// `str(datetime.timedelta)` of a number of microseconds (`None` past timedelta's range).
fn delta_str(us: i128) -> Option<String> {
    let days = us.div_euclid(US_PER_DAY);
    if days.abs() > MAX_DELTA_DAYS {
        return None;
    }
    let rest = us.rem_euclid(US_PER_DAY) as i64;
    let s = rest / 1_000_000;
    let clock = format!("{}:{:02}:{:02}{}", s / 3600, s / 60 % 60, s % 60, frac_us(rest % 1_000_000));
    Some(if days == 0 { clock } else { format!("{days} day{}, {clock}", if days.abs() == 1 { "" } else { "s" }) })
}

/// CPython's `timedelta(...)` accumulation of one float argument (`accum`): the exact integral part times the
/// factor, then the fraction's integral microseconds, the rest left over for the final rounding.
fn accum(sum: &mut i128, leftover: &mut f64, num: f64, factor: i128) -> Option<()> {
    if !num.is_finite() || num.abs() >= 1e25 {
        return None;
    }
    let int = num.trunc();
    *sum += (int as i128).checked_mul(factor)?;
    let frac = num - int;
    if frac != 0.0 {
        let d = factor as f64 * frac;
        let i2 = d.trunc();
        *sum += i2 as i128;
        *leftover += d - i2;
    }
    Some(())
}

/// CPython's final rounding of the left-over microseconds: to nearest, a half to the even total.
fn finish(sum: i128, leftover: f64) -> i128 {
    if leftover == 0.0 {
        return sum;
    }
    let mut whole = leftover.round();
    if (whole - leftover).abs() == 0.5 {
        let odd = if sum.rem_euclid(2) == 1 { 1.0 } else { 0.0 };
        whole = 2.0 * ((leftover + odd) * 0.5).round() - odd;
    }
    sum + whole as i128
}

/// `from_excel`: the Python `str` of the datetime, time or timedelta an Excel serial stands for, `None` where
/// openpyxl gets an `OverflowError` or `ValueError` (it then shows `#VALUE!`).
fn from_excel(n: Num, mac: bool, delta: bool) -> Option<String> {
    if delta {
        let us = match n {
            Num::Int(v) => {
                let v = v?;
                if v.abs() > MAX_DELTA_DAYS {
                    return None;
                }
                v * US_PER_DAY
            }
            Num::Float(f) => {
                let (mut sum, mut left) = (0i128, 0.0);
                accum(&mut sum, &mut left, f, US_PER_DAY)?;
                finish(sum, left)
            }
        };
        delta_str(us)?;
        let micro = us.rem_euclid(1_000_000);
        if micro == 0 {
            return delta_str(us);
        }
        // timedelta(seconds=td.total_seconds() // 1, microseconds=round(td.microseconds, -3))
        let secs = us.div_euclid(1_000_000);
        let (q, r) = (micro / 1000, micro % 1000);
        let ms = if r > 500 || (r == 500 && q % 2 == 1) { q + 1 } else { q };
        return delta_str(secs * 1_000_000 + ms * 1000);
    }
    let (value_ge0_lt1, value_gt0_lt60, day, ms): (bool, bool, i128, i128) = match n {
        Num::Int(v) => {
            let v = v?;
            (v == 0, v > 0 && v < 60, v, 0)
        }
        Num::Float(f) => {
            if !f.is_finite() {
                return None;
            }
            // Python's float divmod(f, 1)
            let mut m = f % 1.0;
            let mut div = f - m;
            if m != 0.0 {
                if m < 0.0 {
                    m += 1.0;
                    div -= 1.0;
                }
            } else {
                m = 0.0;
            }
            let day = div.floor();
            if day.abs() > 1e15 {
                return None;
            }
            let ms = (m * 86400.0 * 1000.0).round_ties_even();
            ((0.0..1.0).contains(&f), f > 0.0 && f < 60.0, day as i128, ms as i128)
        }
    };
    let diff_days = ms.div_euclid(86_400_000);
    if value_ge0_lt1 && diff_days == 0 {
        let s = (ms / 1000) as i64;
        return Some(time_str(s / 3600, s / 60 % 60, s % 60, (ms % 1000) as i64 * 1000));
    }
    let day = if value_gt0_lt60 && !mac { day + 1 } else { day };
    if day.abs() > MAX_DELTA_DAYS {
        return None;
    }
    let base = (if mac { MAC_EPOCH } else { WINDOWS_EPOCH }) as i128 + day;
    if !(1..=MAX_ORDINAL as i128).contains(&base) {
        return None;
    }
    let ordinal = base + diff_days;
    if !(1..=MAX_ORDINAL as i128).contains(&ordinal) {
        return None;
    }
    Some(datetime_str(ordinal as i64, (ms.rem_euclid(86_400_000) * 1000) as i64))
}

/// `ordinal` of a valid (year, month, day).
fn ordinal(y: i64, m: i64, d: i64) -> i64 {
    let y1 = y - 1;
    let before: i64 = (1..m).map(|k| days_in_month(y, k)).sum();
    y1 * 365 + y1 / 4 - y1 / 100 + y1 / 400 + before + d
}

/// `n` Unicode decimal digits at `i` (`\d{n}`), as a number.
fn digits_at(s: &[char], i: usize, n: usize) -> Option<i64> {
    let run = s.get(i..i + n)?;
    if !run.iter().all(|&c| bilingua_model::py_isdecimal_char(c)) {
        return None;
    }
    Some(run.iter().fold(0i64, |a, &c| a * 10 + digit_value(c) as i64))
}

/// `from_ISO8601` for a `t="d"` cell: `str` of the date, time, datetime or duration; an error where openpyxl raises
/// (it fails to load the workbook).
pub(super) fn from_iso(v: &str, name: &str) -> Result<String, Error> {
    let s: Vec<char> = v.chars().collect();
    let mut i = 0;
    let date = match (digits_at(&s, 0, 4), s.get(4), digits_at(&s, 5, 2), s.get(7), digits_at(&s, 8, 2)) {
        (Some(y), Some('-'), Some(m), Some('-'), Some(d)) => {
            i = 10;
            Some((y, m, d))
        }
        _ => None,
    };
    if s.get(i) == Some(&'T') {
        i += 1;
    }
    let time = match (digits_at(&s, i, 2), s.get(i + 2), digits_at(&s, i + 3, 2)) {
        (Some(h), Some(':'), Some(mi)) => {
            let mut j = i + 5;
            let (mut sec, mut us) = (0, 0);
            if let (Some(':'), Some(sv)) = (s.get(j), digits_at(&s, j + 1, 2)) {
                sec = sv;
                j += 3;
                if s.get(j) == Some(&'.') {
                    let n = (1..=3).rev().find(|&n| digits_at(&s, j + 1, n).is_some()).unwrap_or(0);
                    if n > 0 {
                        let frac: String = ascii_digits(&s[j..j + 1 + n].iter().collect::<String>());
                        us = (frac.parse::<f64>().unwrap_or(0.0) * 1_000_000.0) as i64;
                    }
                }
            }
            Some((h, mi, sec, us))
        }
        _ => None,
    };
    let bad_value = || bad(format!("This XLSX file has invalid spreadsheet XML in {name}. Save it again in Excel."));
    let ok_date = |(y, m, d): (i64, i64, i64)| (1..=9999).contains(&y) && (1..=12).contains(&m) && d >= 1 && d <= days_in_month(y, m);
    let ok_time = |(h, mi, sec, _): (i64, i64, i64, i64)| h < 24 && mi < 60 && sec < 60;
    match (date, time) {
        (Some(dt), None) => {
            if !ok_date(dt) {
                return Err(bad_value());
            }
            Ok(format!("{:04}-{:02}-{:02}", dt.0, dt.1, dt.2))
        }
        (None, Some(t)) => {
            if !ok_time(t) {
                return Err(bad_value());
            }
            Ok(time_str(t.0, t.1, t.2, t.3))
        }
        (Some(dt), Some(t)) => {
            if !ok_date(dt) || !ok_time(t) {
                return Err(bad_value());
            }
            let ord = ordinal(dt.0, dt.1, dt.2);
            Ok(datetime_str(ord, ((t.0 * 60 + t.1) * 60 + t.2) * 1_000_000 + t.3))
        }
        (None, None) => iso_duration(&s).ok_or_else(bad_value),
    }
}

/// `ISO_DURATION`: `PT` then optional `<n>H`, `<n>M`, `<n>[.ddd]S`, as a timedelta.
fn iso_duration(s: &[char]) -> Option<String> {
    if s.get(..2)? != ['P', 'T'] {
        return None;
    }
    let mut i = 2;
    let run = |i: usize| (i..s.len()).take_while(|&j| bilingua_model::py_isdecimal_char(s[j])).count();
    let mut parts: [Option<f64>; 3] = [None; 3];
    for (k, unit) in ['H', 'M'].into_iter().enumerate() {
        let n = run(i);
        if n > 0 && s.get(i + n) == Some(&unit) {
            parts[k] = py_float(&s[i..i + n].iter().collect::<String>());
            i += n + 1;
        }
    }
    let n = run(i);
    if n > 0 {
        let mut end = i + n;
        if s.get(end) == Some(&'.') {
            let f = run(end + 1).min(3);
            if f > 0 && s.get(end + 1 + f) == Some(&'S') {
                end += 1 + f;
            }
        }
        if s.get(end) == Some(&'S') {
            parts[2] = py_float(&s[i..end].iter().collect::<String>());
        }
    }
    if parts.iter().all(Option::is_none) {
        return None;
    }
    let (mut sum, mut left) = (0i128, 0.0);
    // CPython adds the arguments in this order: seconds, minutes, hours
    for (k, factor) in [(2, 1_000_000i128), (1, 60_000_000), (0, 3_600_000_000)] {
        if let Some(v) = parts[k] {
            accum(&mut sum, &mut left, v, factor)?;
        }
    }
    delta_str(finish(sum, left))
}

#[cfg(test)]
mod value_tests {
    use super::*;

    #[test]
    fn python_float_repr() {
        for (f, want) in [(1e16, "1e+16"), (1.5e16, "1.5e+16"), (123.0, "123.0"), (0.0001, "0.0001"), (1e-5, "1e-05"), (-2.5, "-2.5"), (1e15, "1000000000000000.0"), (0.1 + 0.2, "0.30000000000000004")] {
            assert_eq!(py_float_repr(f), want);
        }
    }

    #[test]
    fn excel_serials_as_python_prints_them() {
        let f = |v: f64, delta: bool| from_excel(Num::Float(v), false, delta);
        assert_eq!(f(45000.5, false).unwrap(), "2023-03-15 12:00:00");
        assert_eq!(f(1.75, true).unwrap(), "1 day, 18:00:00");
        assert_eq!(f(0.25, false).unwrap(), "06:00:00");
        assert_eq!(f(-1.25, true).unwrap(), "-2 days, 18:00:00");
        assert_eq!(from_excel(Num::Int(Some(1)), false, false).unwrap(), "1900-01-01 00:00:00");
        assert_eq!(from_excel(Num::Int(Some(0)), true, false).unwrap(), "00:00:00");
        assert!(f(1e10, false).is_none());
        assert!(f(f64::NAN, false).is_none());
        assert_eq!(from_iso("2024-02-29T10:30:05.5Z", "s").unwrap(), "2024-02-29 10:30:05.500000");
        assert_eq!(from_iso("PT1H30M", "s").unwrap(), "1:30:00");
        assert!(from_iso("2023-02-29", "s").is_err());
        assert!(is_date_format("yyyy-mm-dd") && !is_date_format("0.00") && !is_date_format("\"day\"0") && is_date_format("[h]:mm"));
        assert!(!is_date_format("[Red]0") && !is_date_format("_d0"));
    }
}

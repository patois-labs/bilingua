//! Span patching: keep the original bytes, splice replacements into byte ranges on save.

use std::ops::Range;

/// Replace `range` of the original bytes with `bytes`. A zero-length range is an insertion.
#[derive(Debug, Clone)]
pub struct Patch {
    pub range: Range<usize>,
    pub bytes: Vec<u8>,
}

impl Patch {
    pub fn replace(range: Range<usize>, text: impl Into<String>) -> Patch {
        Patch { range, bytes: text.into().into_bytes() }
    }

    pub fn insert(pos: usize, text: impl Into<String>) -> Patch {
        Patch { range: pos..pos, bytes: text.into().into_bytes() }
    }
}

/// Apply patches to `orig`. Patches must not overlap. Order of insertions at the same
/// position is preserved.
pub fn apply(orig: &[u8], mut patches: Vec<Patch>) -> Vec<u8> {
    patches.sort_by_key(|p| p.range.start); // stable
    let extra: usize = patches.iter().map(|p| p.bytes.len()).sum();
    let mut out = Vec::with_capacity(orig.len() + extra);
    let mut at = 0usize;
    for p in &patches {
        if p.range.start < at {
            // Overlap would corrupt the file; skip rather than guess.
            continue;
        }
        out.extend_from_slice(&orig[at..p.range.start]);
        out.extend_from_slice(&p.bytes);
        at = p.range.end;
    }
    out.extend_from_slice(&orig[at..]);
    out
}

/// A state-like attribute inside a start tag.
#[derive(Debug, Clone)]
pub enum AttrSlot {
    /// The attribute exists: `full` covers ` name="value"`, `value` only the value.
    Existing { full: Range<usize>, value: Range<usize>, raw: String },
    /// The attribute is absent; insert it at `pos`.
    Missing { pos: usize },
}

impl AttrSlot {
    /// Patch to set the attribute to `new` (`None` removes it). `name` is the attribute name.
    /// `default` is the value that means the same as "absent" (so no patch is needed).
    pub fn patch(&self, name: &str, new: Option<&str>, default: Option<&str>) -> Option<Patch> {
        match (self, new) {
            (AttrSlot::Existing { value, raw, .. }, Some(w)) => {
                if raw == w {
                    None
                } else {
                    Some(Patch::replace(value.clone(), w))
                }
            }
            (AttrSlot::Existing { full, .. }, None) => Some(Patch::replace(full.clone(), "")),
            (AttrSlot::Missing { pos }, Some(w)) => {
                if Some(w) == default {
                    None
                } else {
                    Some(Patch::insert(*pos, format!(" {name}=\"{w}\"")))
                }
            }
            (AttrSlot::Missing { .. }, None) => None,
        }
    }

    pub fn from_tag(buf: &[u8], tag: &crate::attrs::TagInfo, name: &str) -> AttrSlot {
        match tag.attr(buf, name) {
            Some(a) => AttrSlot::Existing {
                full: a.full_range(buf),
                value: a.value.clone(),
                raw: crate::attrs::unescape_attr(&buf[a.value.clone()]),
            },
            None => AttrSlot::Missing { pos: tag.insert_pos },
        }
    }
}

/// Whitespace that precedes the element starting at `pos` on its own line, as
/// `"\n" + indent` (or `"\r\n" + indent`). Empty if the element is not at line start.
pub fn line_prefix(buf: &[u8], pos: usize) -> String {
    let mut i = pos;
    while i > 0 && matches!(buf[i - 1], b' ' | b'\t') {
        i -= 1;
    }
    if i > 0 && buf[i - 1] == b'\n' {
        let nl = if i > 1 && buf[i - 2] == b'\r' { "\r\n" } else { "\n" };
        let indent = String::from_utf8_lossy(&buf[i..pos]);
        return format!("{nl}{indent}");
    }
    String::new()
}

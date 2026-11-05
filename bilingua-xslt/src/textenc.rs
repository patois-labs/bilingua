//! Writing a merged XSLT text back in the file's own encoding. Reading is `bilingua_model::enc::decode_text`
//! (declared charsets honoured); this adds what LocOS does for XML: a character the legacy charset cannot hold
//! becomes a numeric character reference (legal in XML content and attribute values), so the file keeps its
//! encoding and its declaration instead of switching to UTF-8.

use bilingua_model::encoding_rs::Encoding;
use bilingua_model::{enc, Error, MergeOptions, OutputEncoding};
use std::borrow::Cow;

fn fits(e: &'static Encoding, base: &str, c: char) -> bool {
    if c.is_ascii() {
        return true;
    }
    if base == "iso-8859-1" {
        return (c as u32) <= 0xFF;
    }
    !e.encode(c.encode_utf8(&mut [0u8; 4])).2
}

pub fn encode(text: &str, tag: &str, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
    let base = enc::base(tag);
    let legacy = match base {
        // written as UTF-8: every character fits, no references needed
        _ if opts.output_encoding == OutputEncoding::Utf8 => None,
        "utf-8" | "utf-8-bom" | "utf-16le" | "utf-16be" => None,
        other => Encoding::for_label(other.as_bytes()),
    };
    let text: Cow<str> = match legacy {
        Some(e) if !text.is_ascii() && (if base == "iso-8859-1" { text.chars().any(|c| c as u32 > 0xFF) } else { e.encode(text).2 }) => {
            let mut out = String::with_capacity(text.len() + 16);
            for c in text.chars() {
                if fits(e, base, c) {
                    out.push(c);
                } else {
                    out.push_str(&format!("&#{};", c as u32));
                }
            }
            Cow::Owned(out)
        }
        _ => Cow::Borrowed(text),
    };
    enc::encode_text_with(&text, tag, opts)
}

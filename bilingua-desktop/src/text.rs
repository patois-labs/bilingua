//! Shared text reading and writing: UTF-8 only, as Localization OS.

use bilingua_model::enc::{self, DecodeOptions};
use bilingua_model::{Error, ExtractOptions, Warning};

pub(crate) struct Read {
    pub text: String,
    pub encoding: String,
    pub warnings: Vec<Warning>,
}

/// Decodes the upload. Anything that is not UTF-8 (a UTF-8 BOM is kept) is refused, as are binary files.
pub(crate) fn read(bytes: &[u8], opts: &ExtractOptions, name: &str) -> Result<Read, Error> {
    let limits = opts.limits();
    bilingua_model::guard::check_input(bytes, &limits)?;
    let not_utf8 = || {
        Error::Encoding(format!(
            "This {name} file is not valid UTF-8 (the format requires UTF-8); re-save the file as UTF-8."
        ))
    };
    // Cheap early checks keep a UTF-16 or Latin-1 file from being guessed at.
    let body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(bytes);
    if std::str::from_utf8(body).is_err() {
        if body.contains(&0) {
            return Err(Error::Encoding(format!("This {name} file looks like a binary file, not text; re-save it as UTF-8.")));
        }
        return Err(not_utf8());
    }
    let dec = enc::decode_text(bytes, &DecodeOptions::new(opts))?;
    if !matches!(enc::base(&dec.encoding), "utf-8" | "utf-8-bom") {
        return Err(not_utf8());
    }
    if dec.text.contains('\0') {
        return Err(Error::Encoding(format!("This {name} file looks like a binary file, not text; re-save it as UTF-8.")));
    }
    Ok(Read { text: dec.text, encoding: dec.encoding, warnings: dec.warnings })
}

/// Python's `str.isspace` set (Unicode white space plus the four ASCII separators U+001C..U+001F).
pub(crate) fn py_ws(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// Python's `str.strip()`.
pub(crate) fn py_strip(s: &str) -> &str {
    s.trim_matches(py_ws)
}

//! AsciiDoc under the LocOS profile and the lone-mark shield (wave E). An untouched or localized build keeps the
//! file's bytes. A pseudo translation (letters rotated) reads back with the codes and text it set; a line that reads
//! back as set gets no character reference, and a code's own mark is never written as one.
use bilingua::profile::tokens;
use bilingua::*;

fn opts(target: Option<&str>) -> ExtractOptions {
    profile::locos().extract_options().with_granularity(Granularity::Paragraph).with_target_lang(target.map(str::to_string))
}

/// ROT13 on the letters outside the code tokens.
fn rot(t: &str) -> String {
    let mut out = String::new();
    let mut in_tok = false;
    for c in t.chars() {
        if c == '\u{27E6}' {
            in_tok = true;
        }
        let r = if in_tok {
            c
        } else if c.is_ascii_lowercase() {
            (((c as u8 - b'a' + 13) % 26) + b'a') as char
        } else if c.is_ascii_uppercase() {
            (((c as u8 - b'A' + 13) % 26) + b'A') as char
        } else {
            c
        };
        if c == '\u{27E7}' {
            in_tok = false;
        }
        out.push(r);
    }
    out
}

fn texts(data: &[u8]) -> Vec<String> {
    let doc = extract("a.adoc", data, &opts(None)).unwrap();
    profile::locos_units(&doc).into_iter().map(|u| u.text).collect()
}

fn build(data: &[u8], target: Option<&str>, f: &dyn Fn(&str) -> Option<String>) -> String {
    let mut doc = extract("a.adoc", data, &opts(target)).unwrap();
    for u in profile::locos_units(&doc) {
        if let Some(t) = f(&u.text) {
            let (inl, _, _) = tokens::repair_locos_text_for(&doc.format, &t, &u.map);
            profile::locos_set_target(&mut doc, &u.segment_id, inl).unwrap();
        }
    }
    String::from_utf8(merge_with(&doc, &profile::locos().merge_options(&doc.format)).unwrap()).unwrap()
}

const DOCS: &[&str] = &[
    // a strong mark at the start of a paragraph, emphasis pairs inside it, and a table with an open strong cell before it
    "|===============\n| `variance` | __u^2^__ | *Variance\n|===============\n\n*The meaning of this rule says that if _u_ is a unit of heat, or a unit of heat times another unit, the heat in _u_ must be read as a heat change.\nSo the **`units_meta`** field, if set, must have the value `heat: change`.\nSee <<heat-units>> for more.\n",
    "*The meaning of this rule says that if _u_ is a unit of heat, the heat in _u_ must be read as a change.\nSo the **`units_meta`** field must be set.\n",
    "A *strong* word, then u_ and _u and a lone * here.\n",
];

#[test]
fn untouched_and_localized_builds_keep_the_bytes() {
    for d in DOCS {
        for lang in ["de-DE", "ar-SA", "ja-JP"] {
            let want = format!(":lang: {lang}\n\n{d}");
            assert_eq!(build(d.as_bytes(), Some(lang), &|_| None), want);
            assert_eq!(build(d.as_bytes(), Some(lang), &|t| Some(t.to_string())), want);
        }
    }
}

/// Mark references back as the characters they print.
fn unref(s: &str) -> String {
    let mut s = s.to_string();
    for c in ['*', '_', '`', '#', '^', '~', '+'] {
        s = s.replace(&format!("&#{};", c as u32), &c.to_string());
    }
    s
}

#[test]
fn a_pseudo_translation_keeps_every_code() {
    for (k, d) in DOCS.iter().enumerate() {
        let want: Vec<String> = texts(d.as_bytes()).iter().map(|t| rot(t)).collect();
        for lang in [None, Some("de-DE")] {
            let out = build(d.as_bytes(), lang, &|t| Some(rot(t)));
            // the codes read back are the codes set, and the text is the text set
            let got: Vec<String> = texts(out.as_bytes()).iter().map(|t| unref(t)).collect();
            assert_eq!(got, want, "{out}");
            if k == 2 {
                // a line that reads back as set as it is gets no reference
                assert!(!out.contains("&#"), "{out}");
            } else {
                // the writer joins the paragraph's lines, which would let the text `*` before `The` pair with the
                // `**` code of the next line (LocOS's own build does that, and its `**` code is lost); only that
                // text mark and the plain `_` beside it are written as references, never a code's own mark
                assert!(out.contains("&#42;Gur") && out.contains("**`units_meta`**"), "{out}");
            }
        }
    }
}

//! Bilingua plain-text filter.
//!
//! One unit per paragraph (a run of lines separated from the next by a blank line). Everything else (blank
//! lines, the indentation of a paragraph's first line, the spaces and line ending after its last line) is
//! skeleton and comes back byte for byte. A line break inside a paragraph is a standalone tag whose code is the
//! original line ending (`\n` or `\r\n`), so mixed endings survive and translators cannot damage them.
//! A UTF-8 byte order mark is kept.

use bilingua_model::*;
use std::collections::BTreeMap;

pub struct TextFilter;

const ID: &str = "txt";
const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

impl Filter for TextFilter {
    fn id(&self) -> &'static str {
        ID
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["txt", "text"]
    }

    fn sniff(&self, bytes: &[u8]) -> bool {
        let body = bytes.strip_prefix(BOM).unwrap_or(bytes);
        !body.contains(&0) && std::str::from_utf8(body).is_ok()
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
            return Err(Error::Unsupported("UTF-16 text files; save the file as UTF-8".into()));
        }
        let (body, encoding) = match bytes.strip_prefix(BOM) {
            Some(rest) => (rest, "utf-8-bom"),
            None => (bytes, "utf-8"),
        };
        let text = std::str::from_utf8(body).map_err(|e| Error::Parse {
            format: ID.into(),
            msg: format!("the file is not valid UTF-8 (at byte {})", e.valid_up_to()),
        })?;

        let mut parts: Vec<SkelPart> = Vec::new();
        let mut units: Vec<Unit> = Vec::new();
        let mut raw = String::new();
        let mut group: Vec<&str> = Vec::new();

        let flush = |group: &mut Vec<&str>, raw: &mut String, parts: &mut Vec<SkelPart>, units: &mut Vec<Unit>| {
            if group.is_empty() {
                return;
            }
            let (prefix, unit, suffix) = build_unit(group, units.len() + 1);
            raw.push_str(&prefix);
            if !raw.is_empty() {
                parts.push(SkelPart::Raw(std::mem::take(raw)));
            }
            parts.push(SkelPart::Unit(units.len()));
            units.push(unit);
            raw.push_str(&suffix);
            group.clear();
        };

        for line in text.split_inclusive('\n') {
            if line.trim().is_empty() {
                flush(&mut group, &mut raw, &mut parts, &mut units);
                raw.push_str(line);
            } else {
                group.push(line);
            }
        }
        flush(&mut group, &mut raw, &mut parts, &mut units);
        if !raw.is_empty() {
            parts.push(SkelPart::Raw(raw));
        }

        Ok(Document {
            format: ID.into(),
            name: None,
            source_lang: opts.source_lang.clone(),
            target_lang: opts.target_lang.clone(),
            granularity: opts.granularity,
            units,
            skeleton: Skeleton::text(parts, encoding, bilingua_model::parser_info!()),
        })
    }

    fn merge(&self, doc: &Document) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("txt needs a text skeleton".into()));
        };
        let mut out: Vec<u8> = Vec::new();
        if encoding == "utf-8-bom" {
            out.extend_from_slice(BOM);
        }
        for p in parts {
            match p {
                SkelPart::Raw(s) => out.extend_from_slice(s.as_bytes()),
                SkelPart::Unit(i) => {
                    let unit = doc.units.get(*i).ok_or_else(|| Error::Write(format!("no unit {i}")))?;
                    out.extend_from_slice(render(unit).as_bytes());
                }
            }
        }
        Ok(out)
    }
}

/// Splits `line` into content and its line ending.
fn split_eol(line: &str) -> (&str, &str) {
    if let Some(c) = line.strip_suffix("\r\n") {
        (c, "\r\n")
    } else if let Some(c) = line.strip_suffix('\n') {
        (c, "\n")
    } else {
        (line, "")
    }
}

/// Builds the unit for a paragraph. Returns (skeleton before, unit, skeleton after).
fn build_unit(lines: &[&str], n: usize) -> (String, Unit, String) {
    let mut inl: Vec<Inline> = Vec::new();
    let mut codes: BTreeMap<String, CodeData> = BTreeMap::new();
    let mut prefix = String::new();
    let mut suffix = String::new();
    let mut text = String::new();
    let last = lines.len() - 1;
    for (i, line) in lines.iter().enumerate() {
        let (mut content, eol) = split_eol(line);
        if i == 0 {
            let trimmed = content.trim_start();
            prefix = content[..content.len() - trimmed.len()].to_string();
            content = trimmed;
        }
        if i == last {
            let trimmed = content.trim_end();
            suffix = format!("{}{}", &content[trimmed.len()..], eol);
            content = trimmed;
            text.push_str(content);
        } else {
            text.push_str(content);
            if !text.is_empty() {
                inl.push(Inline::Text(std::mem::take(&mut text)));
            }
            let id = (codes.len() + 1).to_string();
            codes.insert(
                id.clone(),
                CodeData { standalone: Some(eol.to_string()), label: Some("line break".into()), ..Default::default() },
            );
            inl.push(Inline::tag(id, TagKind::Standalone, "\u{21b5}"));
        }
    }
    if !text.is_empty() {
        inl.push(Inline::Text(text));
    }
    let mut unit = Unit::new(format!("p{n}"), inl);
    unit.codes = codes;
    unit.context.kind = Some("paragraph".into());
    (prefix, unit, suffix)
}

fn render(unit: &Unit) -> String {
    let mut s = String::new();
    for i in unit.output() {
        match i {
            Inline::Text(t) => s.push_str(&t),
            Inline::Tag { id, .. } => {
                if let Some(c) = unit.codes.get(&id).and_then(|c| c.standalone.as_deref()) {
                    s.push_str(c);
                }
            }
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(s: &[u8]) -> Document {
        TextFilter.extract(s, &ExtractOptions::default()).unwrap()
    }

    fn samples() -> Vec<(&'static str, &'static str)> {
        vec![
            ("en", "Welcome to Larchmoor.\n\nThe tide is out."),
            ("es", "Bienvenido a Larchmoor. \u{bf}C\u{f3}mo est\u{e1}s?\n\nLa marea est\u{e1} baja.\n"),
            ("fr", "Bienvenue \u{e0} Larchmoor !\n\n\u{c0} bient\u{f4}t.\n"),
            ("it", "Benvenuti a Larchmoor.\n\nLa marea \u{e8} bassa.\n"),
            ("de", "Willkommen in Larchmoor.\n\nDie Flut ist \u{fc}ber.\n"),
            ("nl", "Welkom in Larchmoor.\n\nHet is eb.\n"),
            ("pt", "Bem-vindo a Larchmoor.\n\nA mar\u{e9} est\u{e1} baixa.\n"),
            ("ja", "\u{30e9}\u{30fc}\u{30c1}\u{30e0}\u{30a2}\u{3078}\u{3088}\u{3046}\u{3053}\u{305d}\u{3002}\n\n\u{6f6e}\u{304c}\u{5f15}\u{3044}\u{3066}\u{3044}\u{307e}\u{3059}\u{3002}\n"),
            ("zh", "\u{6b22}\u{8fce}\u{6765}\u{5230}\u{62c9}\u{5947}\u{83ab}\u{5c14}\u{3002}\n\n\u{6f6e}\u{6c34}\u{9000}\u{4e86}\u{3002}\n"),
            ("ko", "\u{b77c}\u{cc58}\u{baa8}\u{c5b4}\u{c5d0} \u{c624}\u{c2e0} \u{ac83}\u{c744} \u{d658}\u{c601}\u{d569}\u{b2c8}\u{b2e4}.\n\n\u{ac8c}\u{c774}\u{bc8c}\u{c774} \u{c379}\u{c600}\u{c2b5}\u{b2c8}\u{b2e4}.\n"),
            ("ar", "\u{645}\u{631}\u{62d}\u{628}\u{627} \u{628}\u{643}\u{645} \u{641}\u{64a} \u{644}\u{627}\u{631}\u{62a}\u{634}\u{645}\u{648}\u{631}.\n\n\u{627}\u{644}\u{645}\u{62f}\u{62c}\u{632}\u{631} \u{645}\u{646}\u{62e}\u{641}\u{636}.\n"),
        ]
    }

    #[test]
    fn round_trip_in_every_primary_language() {
        for (lang, s) in samples() {
            let d = extract(s.as_bytes());
            assert_eq!(d.units.len(), 2, "{lang}");
            assert_eq!(TextFilter.merge(&d).unwrap(), s.as_bytes(), "{lang}");
        }
    }

    #[test]
    fn paragraphs_are_units_and_text_is_trimmed_from_the_skeleton() {
        let d = extract(b"\n\n   First para.  \n\nSecond\nstill second\n\n\n   \nThird");
        assert_eq!(d.units.len(), 3);
        assert_eq!(plain_text(&d.units[0].source()), "First para.");
        assert_eq!(d.units[0].id, "p1");
        assert_eq!(d.units[0].context.kind.as_deref(), Some("paragraph"));
        assert_eq!(plain_text(&d.units[1].source()), "Secondstill second");
        assert_eq!(d.units[1].source().len(), 3);
        assert_eq!(plain_text(&d.units[2].source()), "Third");
        assert_eq!(TextFilter.merge(&d).unwrap(), b"\n\n   First para.  \n\nSecond\nstill second\n\n\n   \nThird");
    }

    #[test]
    fn crlf_lf_and_mixed_endings_survive() {
        for s in ["A one\r\nA two\r\n\r\nB\r\n", "A one\nA two\n\nB\n", "A one\r\nA two\n\r\nB\n\r\n", "x\r\n\r\n\r\ny", "no newline"] {
            let d = extract(s.as_bytes());
            assert_eq!(TextFilter.merge(&d).unwrap(), s.as_bytes(), "{s:?}");
        }
        let d = extract(b"A one\r\nA two\r\n");
        let Inline::Tag { id, .. } = &d.units[0].source()[1] else { panic!("expected a line break tag") };
        assert_eq!(d.units[0].codes[id].standalone.as_deref(), Some("\r\n"));
    }

    #[test]
    fn bom_is_kept() {
        let mut b = vec![0xEF, 0xBB, 0xBF];
        b.extend_from_slice("Hola.\n".as_bytes());
        let d = extract(&b);
        assert_eq!(d.skeleton, Skeleton::text(d_parts(&d), "utf-8-bom", parser_info!()));
        assert_eq!(TextFilter.merge(&d).unwrap(), b);
        // without a BOM none is added
        let d = extract(b"Hola.\n");
        assert_eq!(TextFilter.merge(&d).unwrap(), b"Hola.\n");
    }

    fn d_parts(d: &Document) -> Vec<SkelPart> {
        match &d.skeleton {
            Skeleton::Text { parts, .. } => parts.clone(),
            _ => unreachable!(),
        }
    }

    #[test]
    fn translation_replaces_the_unit_and_keeps_line_breaks() {
        let mut d = extract("Hello\nworld\n\nBye.\n".as_bytes());
        let src = d.units[0].source();
        let Inline::Tag { id, kind, display, free } = src[1].clone() else { panic!() };
        d.set_target(
            "p1",
            vec![Inline::text("Hola"), Inline::Tag { id, kind, display, free }, Inline::text("mundo")],
        )
        .unwrap();
        d.set_target("p2", vec![Inline::text("Adi\u{f3}s.")]).unwrap();
        assert_eq!(String::from_utf8(TextFilter.merge(&d).unwrap()).unwrap(), "Hola\nmundo\n\nAdi\u{f3}s.\n");
    }

    #[test]
    fn a_dropped_line_break_tag_joins_the_lines() {
        let mut d = extract(b"a\nb\n");
        d.set_target("p1", vec![Inline::text("ab")]).unwrap();
        assert_eq!(TextFilter.merge(&d).unwrap(), b"ab\n");
    }

    #[test]
    fn empty_and_blank_files() {
        assert_eq!(extract(b"").units.len(), 0);
        let d = extract(b"\n\n  \n");
        assert_eq!(d.units.len(), 0);
        assert_eq!(TextFilter.merge(&d).unwrap(), b"\n\n  \n");
        assert_eq!(TextFilter.merge(&extract(b"")).unwrap(), b"");
    }

    #[test]
    fn bad_input_is_an_error() {
        assert!(matches!(TextFilter.extract(&[0x48, 0xFF, 0xFE, 0x41], &Default::default()), Err(Error::Parse { .. })));
        assert!(matches!(TextFilter.extract(&[0xFF, 0xFE, 0x41, 0x00], &Default::default()), Err(Error::Unsupported(_))));
        assert!(matches!(TextFilter.extract(&[0xFE, 0xFF, 0x00, 0x41], &Default::default()), Err(Error::Unsupported(_))));
    }

    #[test]
    fn sniffing() {
        assert!(TextFilter.sniff("hola".as_bytes()));
        assert!(!TextFilter.sniff(&[0x50, 0x4B, 0x03, 0x04, 0, 0]));
        assert!(!TextFilter.sniff(&[0xFF, 0xD8, 0xFF]));
    }

    #[test]
    fn wrong_skeleton_cannot_be_written() {
        let mut d = extract(b"a");
        d.skeleton = Skeleton::Zip { entries: vec![], parser: None };
        assert!(matches!(TextFilter.merge(&d), Err(Error::Write(_))));
    }
}

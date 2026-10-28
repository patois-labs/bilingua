//! Placeholder plain-text filter (one unit per paragraph, paragraphs split by blank lines). Replaced by
//! `bilingua-text` when that crate is merged.

use bilingua_model::*;

pub struct PlainText;

impl Filter for PlainText {
    fn id(&self) -> &'static str {
        "txt"
    }
    fn extensions(&self) -> &'static [&'static str] {
        &["txt"]
    }
    fn sniff(&self, _bytes: &[u8]) -> bool {
        false
    }
    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        let text = std::str::from_utf8(bytes)
            .map_err(|e| Error::Parse { format: "txt".into(), msg: e.to_string() })?;
        let mut parts = Vec::new();
        let mut units = Vec::new();
        for (i, block) in text.split_inclusive("\n\n").enumerate() {
            let body = block.trim_end_matches('\n');
            let tail = &block[body.len()..];
            if body.trim().is_empty() {
                parts.push(SkelPart::Raw(block.to_string()));
                continue;
            }
            let mut u = Unit::new(format!("p{}", i + 1), vec![Inline::text(body)]);
            u.context.kind = Some("paragraph".into());
            parts.push(SkelPart::Unit(units.len()));
            units.push(u);
            if !tail.is_empty() {
                parts.push(SkelPart::Raw(tail.to_string()));
            }
        }
        Ok(Document {
            format: "txt".into(),
            name: None,
            source_lang: opts.source_lang.clone(),
            target_lang: opts.target_lang.clone(),
            granularity: opts.granularity,
            units,
            skeleton: Skeleton::Text { parts, encoding: "utf-8".into() },
        })
    }
    fn merge(&self, doc: &Document) -> Result<Vec<u8>, Error> {
        let Skeleton::Text { parts, .. } = &doc.skeleton else {
            return Err(Error::Write("txt needs a text skeleton".into()));
        };
        let mut out = String::new();
        for p in parts {
            match p {
                SkelPart::Raw(s) => out.push_str(s),
                SkelPart::Unit(i) => out.push_str(&plain_text(&doc.units[*i].output())),
            }
        }
        Ok(out.into_bytes())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn round_trip_and_translate() {
        let src = "Hello there.\n\nSecond paragraph.\n";
        let mut d = crate::extract("a.txt", src.as_bytes(), &Default::default()).unwrap();
        assert_eq!(crate::merge(&d).unwrap(), src.as_bytes());
        d.set_target("p2", vec![crate::Inline::text("Segundo párrafo.")]).unwrap();
        assert_eq!(String::from_utf8(crate::merge(&d).unwrap()).unwrap(), "Hello there.\n\nSegundo párrafo.\n");
    }
}

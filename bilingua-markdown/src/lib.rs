//! Bilingua Markdown filter (CommonMark plus GitHub tables, strikethrough, task lists and footnotes).
//!
//! One unit per paragraph, heading (`context.kind = "heading"`, level in `style` as `h1`..`h6`), list item and
//! table cell; image alt text (`kind = "alt"`) and link titles (`kind = "title"`) are units of their own. Emphasis,
//! strong, strikethrough and links are paired tags; code spans, images, autolinks, inline HTML, footnote
//! references and hard breaks are standalone tags. The URL stays in the tag's code, not in the text. Fenced and
//! indented code, HTML blocks, reference definitions and everything else stay in the skeleton.
//!
//! The file is never re-rendered: only the byte ranges of unit text are rewritten, so an untouched file comes
//! back byte for byte. YAML front matter keys `title` and `description` (one-line values) are units too.

mod extract;
mod frontmatter;
mod render;

use bilingua_model::*;

pub struct MarkdownFilter;

const ID: &str = "markdown";
const BOM: &[u8] = &[0xEF, 0xBB, 0xBF];

impl Filter for MarkdownFilter {
    fn id(&self) -> &'static str {
        ID
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["md", "markdown", "mdown", "mkd"]
    }

    fn sniff(&self, _bytes: &[u8]) -> bool {
        false // Markdown is plain text; only the extension says so
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
            return Err(Error::Unsupported("UTF-16 Markdown files; save the file as UTF-8".into()));
        }
        let (body, encoding) = match bytes.strip_prefix(BOM) {
            Some(rest) => (rest, "utf-8-bom"),
            None => (bytes, "utf-8"),
        };
        let text = std::str::from_utf8(body)
            .map_err(|e| Error::Parse { format: ID.into(), msg: format!("the file is not valid UTF-8 (at byte {})", e.valid_up_to()) })?;
        let ex = extract::extract(text, opts.include_locked);
        let doc = Document {
            format: ID.into(),
            name: None,
            source_lang: opts.source_lang.clone(),
            target_lang: opts.target_lang.clone(),
            granularity: opts.granularity,
            units: ex.units,
            skeleton: Skeleton::text(ex.parts, encoding, bilingua_model::parser_info!()),
        };
        // Safety net: nothing translated must mean nothing changed.
        if self.merge(&doc)? != bytes {
            return Err(Error::Parse { format: ID.into(), msg: "internal error: the file does not round-trip".into() });
        }
        Ok(doc)
    }

    fn merge(&self, doc: &Document) -> Result<Vec<u8>, Error> {
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("markdown needs a text skeleton".into()));
        };
        let mut out = String::new();
        for p in parts {
            match p {
                SkelPart::Raw(s) => out.push_str(s),
                SkelPart::Unit(i) => {
                    let unit = doc.units.get(*i).ok_or_else(|| Error::Write(format!("no unit {i}")))?;
                    out.push_str(&render::render_unit(doc, unit));
                }
            }
        }
        let mut bytes = Vec::with_capacity(out.len() + 3);
        if encoding == "utf-8-bom" {
            bytes.extend_from_slice(BOM);
        }
        bytes.extend_from_slice(out.as_bytes());
        Ok(bytes)
    }

    fn preview(&self, doc: &Document) -> Option<String> {
        render::preview(doc)
    }

    fn render_segment(&self, doc: &Document, seg_id: &str) -> Option<String> {
        render::render_segment(doc, seg_id)
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_oracle;

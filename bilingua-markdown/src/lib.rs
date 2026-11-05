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

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 1;

mod extract;
mod frontmatter;
mod render;

use bilingua_model::*;

pub struct MarkdownFilter;

const ID: &str = "markdown";

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
        let mut dec = bilingua_model::enc::decode_text(bytes, &bilingua_model::enc::DecodeOptions::new(opts))?;
        let text_owned = std::mem::take(&mut dec.text);
        let text = text_owned.as_str();
        let encoding = dec.encoding.as_str();
        let ex = extract::extract(text, opts.include_locked);
        let doc = Document::new(ID, ex.units, Skeleton::text(ex.parts, encoding, bilingua_model::parser_info!())).with_options(opts);
        // Safety net: nothing translated must mean nothing changed.
        if self.merge(&doc)? != bytes {
            return Err(Error::Parse { format: ID.into(), msg: "internal error: the file does not round-trip".into() });
        }
        bilingua_model::guard::finish(doc, std::mem::take(&mut dec.warnings), &opts.limits())
    }

    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
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
        bilingua_model::enc::encode_text_with(&out, encoding, opts)
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

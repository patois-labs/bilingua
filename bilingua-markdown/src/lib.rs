//! Bilingua Markdown filter (CommonMark plus GitHub tables, strikethrough, task lists and footnotes).
//!
//! One unit per paragraph, heading (`context.kind = "heading"`, level in `style` as `h1`..`h6`), list item and
//! table cell; image alt text (`kind = "alt"`) and link titles (`kind = "title"`) are units of their own. Emphasis,
//! strong, strikethrough and links are paired tags; code spans, images, autolinks, inline HTML, footnote
//! references and hard breaks are standalone tags. The URL stays in the tag's code, not in the text. Fenced and
//! indented code, HTML blocks, reference definitions and everything else stay in the skeleton.
//!
//! The file is never re-rendered: only the byte ranges of unit text are rewritten, so an untouched file comes
//! back byte for byte. YAML front matter keys `title` and `description` (one-line values) are units too, or the
//! keys `markdown.fm_keys` lists.

/// Skeleton schema number: bump only when this crate's skeleton layout changes (see `bilingua_model::check_parser`).
pub const SKELETON_SCHEMA: u32 = 2;

mod extract;
mod frontmatter;
mod locos;
mod locos_emphasis;
mod render;

use bilingua_model::*;

pub struct MarkdownFilter;

const ID: &str = "markdown";

/// The `format_options` keys of the Markdown filter: LocOS export keywords, accepted so the adapter is not refused.
pub const FORMAT_OPTIONS: &[bilingua_model::FormatOptionKey] = &[
    bilingua_model::FormatOptionKey::text("markdown.walk", "Accepted and ignored: LocOS's pre-#731 walk stamp; Bilingua has one walk, the current one."),
    bilingua_model::FormatOptionKey::text("markdown.md_walk", "Accepted and ignored: the same as markdown.walk (LocOS's own keyword name)."),
    bilingua_model::FormatOptionKey::text(
        "markdown.fm_keys",
        "The front matter keys whose one-line values are translated: comma separated, case-insensitive (LocOS's LOC_TMS_FRONT_MATTER_KEYS); empty = none. Unset: title and description (LocOS profile: LocOS's default list).",
    ),
];

/// `markdown.fm_keys` as LocOS's `front_matter_keys` reads `LOC_TMS_FRONT_MATTER_KEYS`: split at commas, blanks
/// around a name dropped (Python `str.strip`), lower case, empty names dropped. `None` when the option is not set.
pub(crate) fn fm_keys(options: &std::collections::BTreeMap<String, String>) -> Option<Vec<String>> {
    let raw = options.get("markdown.fm_keys")?;
    let mut keys: Vec<String> =
        raw.split(',').map(|n| bilingua_model::pytext::py_strip(n).to_lowercase()).filter(|n| !n.is_empty()).collect();
    keys.sort();
    keys.dedup();
    Some(keys)
}

impl Filter for MarkdownFilter {
    fn id(&self) -> &'static str {
        ID
    }

    fn format_option_keys(&self) -> &'static [bilingua_model::FormatOptionKey] {
        FORMAT_OPTIONS
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["md", "markdown", "mdown", "mkd"]
    }

    fn sniff(&self, _bytes: &[u8]) -> bool {
        false // Markdown is plain text; only the extension says so
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        let mut dec = bilingua_model::enc::decode_text(bytes, &bilingua_model::enc::DecodeOptions::new(opts))?;
        let text_owned = std::mem::take(&mut dec.text);
        let text = text_owned.as_str();
        let encoding = dec.encoding.as_str();
        if opts.is_profile(profile::LOCOS) {
            return locos::extract(text, encoding, opts, std::mem::take(&mut dec.warnings));
        }
        let ex = extract::extract(text, opts.include_locked, fm_keys(&opts.format_options).as_deref());
        let doc = Document::new(ID, ex.units, Skeleton::text(ex.parts, encoding, bilingua_model::parser_info!())).with_options(opts);
        // Safety net: nothing translated must mean nothing changed.
        if self.merge(&doc)? != bytes {
            return Err(Error::Parse { format: ID.into(), msg: "internal error: the file does not round-trip".into() });
        }
        bilingua_model::guard::finish(doc, std::mem::take(&mut dec.warnings), &opts.limits())
    }

    fn merge_with(&self, doc: &Document, opts: &bilingua_model::MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        // a document extracted under the LocOS profile carries the decoded file and is written the LocOS way
        if let Skeleton::Binary { data, .. } = &doc.skeleton {
            return locos::merge(doc, data);
        }
        let Skeleton::Text { parts, encoding, .. } = &doc.skeleton else {
            return Err(Error::Write("markdown needs a text skeleton".into()));
        };
        let mut out = String::new();
        for p in parts {
            match p {
                SkelPart::Raw(s) => out.push_str(s),
                SkelPart::Unit(i) => {
                    let unit = doc.units.get(*i).ok_or_else(|| Error::Write(format!("no unit {i}")))?;
                    if unit.segments.iter().any(|s| s.has_target()) && !unit.codes.contains_key(extract::FM_KEY) {
                        // a translation opens no block, splits no cell and breaks no link its source did not
                        // (LocOS #704, #790, structure fuzz)
                        let text = render::inlines_md_translated(doc, unit, &render::escape_link_text(unit, &unit.output()), out.chars().next_back());
                        let line = &out[out.rfind('\n').map_or(0, |p| p + 1)..];
                        let src = render::inlines_md(doc, unit, &unit.source());
                        let mut text = render::guard_block_starts(line, &src, &text);
                        match unit.context.kind.as_deref() {
                            Some("cell") => text = render::guard_cell(&text),
                            Some("heading") if line.trim_start().starts_with('#') => text = render::guard_atx_close(&text),
                            _ => {}
                        }
                        out.push_str(&text);
                    } else {
                        out.push_str(&render::render_unit(doc, unit));
                    }
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

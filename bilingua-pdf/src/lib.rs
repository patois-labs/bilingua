//! PDF text layer filter for Bilingua. Spec and baseline: the Localization OS filter `app/filters/pdf_filter.py`
//! and its suite `tests/verify_pdf.py`; Okapi's `okf_pdf` for extraction only.
//!
//! Extract reads the embedded text layer of every page the way LocOS does: the page text PDFium returns
//! (`FPDFText_GetText`, re-implemented here in Rust, see [`textpage`]), stripped, split into paragraphs on blank
//! lines, the lines of a paragraph joined with one space. A page whose stripped text is shorter than 20 characters
//! is a scanned page: LocOS sends it to OCR, which stays in LocOS, so this crate gives it no units and lists it
//! (warning `pdf_ocr_page`, part `page N`; [`page_texts`] gives every page's size and text) for the caller to OCR.
//!
//! Merge: Localization OS does not write into the original PDF; it re-typesets the translated paragraphs into a
//! new PDF with typst. That needs a full typesetter and font stack, so it is not in this crate (see
//! `DESIGN-MERGE.md`): an untouched merge returns the original bytes, a translated merge is
//! [`Error::Unsupported`] and the caller builds the PDF from the unit targets.

use bilingua_model::guard;
use bilingua_model::*;

mod content;
mod crypt;
mod decode;
mod doc;
mod font;
mod fontprog;
mod obj;
mod tables;
mod text;
mod textpage;

/// Bumped only when the skeleton changes shape.
pub const SKELETON_SCHEMA: u32 = 1;


/// LocOS `OCR_MIN_CHARS`: a page with less stripped text than this goes to OCR.
pub const OCR_MIN_CHARS: usize = 20;
/// LocOS `PDF_MAX_PAGES`.
pub const MAX_PAGES: usize = 1000;
/// LocOS `PDF_OCR_MAX_PIXEL_DIM`.
pub const OCR_MAX_PIXEL_DIM: f64 = 4000.0;
/// LocOS `PDF_OCR_MAX_TOTAL_PIXELS`.
pub const OCR_MAX_TOTAL_PIXELS: f64 = 2_500_000_000.0;
/// LocOS renders scanned pages at 150 dpi.
const OCR_RENDER_DPI: f64 = 150.0;

/// One page as extraction saw it.
#[derive(Debug, Clone, PartialEq)]
pub struct PageInfo {
    /// 0-based, as LocOS numbers pages.
    pub page: usize,
    pub width: f64,
    pub height: f64,
    /// LocOS sends this page to OCR (its text layer is shorter than [`OCR_MIN_CHARS`]).
    pub ocr: bool,
    /// The page text as PDFium returns it (`get_text_range`), before LocOS strips and splits it.
    pub text: String,
}

/// The text layer of every page, with PDFium's page text. Exposed for the oracle and for callers that need the raw
/// page text (an OCR adapter, a preview).
pub fn page_texts(bytes: &[u8], opts: &ExtractOptions) -> Result<Vec<PageInfo>, Error> {
    bilingua_model::check_own_options(&Pdf, &opts.format_options)?;
    let limits = opts.limits();
    guard::check_input(bytes, &limits)?;
    let budget = uint_option(opts, "pdf.max_stream_bytes")?.unwrap_or(limits.max_zip_bytes);
    let d = doc::Doc::open(bytes, budget).map_err(|e| open_error(e, budget))?;
    let n = d.page_count();
    let max_pages = uint_option(opts, "pdf.max_pages")?.map_or(MAX_PAGES, |v| v.min(usize::MAX as u64) as usize);
    if n > max_pages {
        return Err(LimitError::new(LimitKind::Units, format!("This PDF has {n} pages; the limit is {max_pages}. Split it into smaller files and upload those.")).into());
    }
    let pages = d.pages(n);
    if pages.len() < n {
        return Err(parse_err("a page of the page tree does not load"));
    }
    let max_objects = limits.max_units.saturating_mul(80);
    let mut fonts = content::FontCache::default();
    let rtl = d.rtl();
    let min_chars = uint_option(opts, "pdf.ocr_min_chars")?.map_or(OCR_MIN_CHARS, |v| v.min(usize::MAX as u64) as usize);
    let mut objects_left = max_objects as i64;
    let max_dim = uint_option(opts, "pdf.ocr_max_pixel_dim")?.map_or(OCR_MAX_PIXEL_DIM, |v| v as f64).max(1.0);
    let max_total = uint_option(opts, "pdf.ocr_max_total_pixels")?.map_or(OCR_MAX_TOTAL_PIXELS, |v| v as f64);
    let mut total_pixels = 0.0;
    let mut out = Vec::with_capacity(n);
    for (i, p) in pages.iter().enumerate() {
        let Some(p) = p else { return Err(parse_err("a page of the page tree does not load")) };
        let data = d.page_content(p);
        let mut it = content::Interp::new(&d, &mut fonts, max_objects);
        let objs = it.run_page(&data, p.resources.clone());
        objects_left -= it.top_level_count as i64;
        if objects_left < 0 || it.text_objects > max_objects {
            return Err(LimitError::new(
                LimitKind::Units,
                format!("This PDF has far more text and drawing objects than MAX_DOCUMENT_SEGMENTS allows ({} segments). Split it into smaller files.", limits.max_units),
            )
            .into());
        }
        if d.budget.borrow().exceeded {
            return Err(budget_error(budget));
        }
        if d.work.get() == 0 {
            return Err(work_error());
        }
        let tp = textpage::TextPage::new(&objs, p.bbox().map(|v| v as f32), p.rotation(), rtl);
        let text = tp.text();
        let (w, h) = p.size();
        let ocr = printable_count(&text) < min_chars;
        // LocOS refuses, before any page is rendered, a document whose scanned pages render to too many pixels.
        if ocr {
            if let Some(scale) = ocr_render_scale(w, h, max_dim) {
                total_pixels += (w * scale).ceil() * (h * scale).ceil();
                if total_pixels > max_total {
                    return Err(LimitError::new(LimitKind::Units, format!("This PDF exceeds the total OCR render pixel limit ({max_total} pixels). Split it into smaller files.")).into());
                }
            }
        }
        out.push(PageInfo { page: i, width: w, height: h, ocr, text });
    }
    if d.budget.borrow().exceeded {
        return Err(budget_error(budget));
    }
    if d.work.get() == 0 {
        return Err(work_error());
    }
    Ok(out)
}

/// One line per text object of a page (font, size, position, rectangle, text), for debugging the text layer.
#[doc(hidden)]
pub fn debug_page(bytes: &[u8], page: usize) -> Result<String, Error> {
    let d = doc::Doc::open(bytes, 256 * 1024 * 1024).map_err(|e| open_error(e, 256 * 1024 * 1024))?;
    let pages = d.pages(d.page_count());
    let Some(Some(p)) = pages.get(page) else { return Err(parse_err("no such page")) };
    let data = d.page_content(p);
    let mut fonts = content::FontCache::default();
    let mut it = content::Interp::new(&d, &mut fonts, usize::MAX / 2);
    let objs = it.run_page(&data, p.resources.clone());
    let mut out = String::new();
    fn walk(objs: &[content::PageObj], depth: usize, out: &mut String) {
        for o in objs {
            match o {
                content::PageObj::Text(t) => {
                    let s: String = t.codes.iter().map(|c| char::decode_utf16(t.font.unicode(*c)).filter_map(|r| r.ok()).collect::<String>()).collect();
                    out.push_str(&format!(
                        "{}T font#{} size {:.2} m [{:.3} {:.3} {:.3} {:.3}] pos ({:.2}, {:.2}) rect ({:.2}, {:.2}, {:.2}, {:.2}) codes {:?} w {:?} {:?}
",
                        "  ".repeat(depth),
                        t.font.id,
                        t.size,
                        t.m.a,
                        t.m.b,
                        t.m.c,
                        t.m.d,
                        t.pos.0,
                        t.pos.1,
                        t.rect.left,
                        t.rect.bottom,
                        t.rect.right,
                        t.rect.top,
                        &t.codes[..t.codes.len().min(8)],
                        t.codes.iter().take(8).map(|c| (t.font.width(*c), t.font.char_bbox(*c))).collect::<Vec<_>>(),
                        s
                    ));
                }
                content::PageObj::Form(m, inner) => {
                    out.push_str(&format!("{}F [{:.3} {:.3} {:.3} {:.3} {:.2} {:.2}]
", "  ".repeat(depth), m.a, m.b, m.c, m.d, m.e, m.f));
                    walk(inner, depth + 1, out);
                }
                content::PageObj::Other => {}
            }
        }
    }
    walk(&objs, 0, &mut out);
    Ok(out)
}

/// A `pdf.*` integer option: `None` when unset; a negative value is refused (the kind check already refused text
/// that is not an integer).
fn uint_option(opts: &ExtractOptions, key: &str) -> Result<Option<u64>, Error> {
    match opts.format_int(key) {
        None => Ok(None),
        Some(v) if v >= 0 => Ok(Some(v as u64)),
        Some(v) => Err(Error::FormatOption { key: key.into(), msg: format!("the value {v} is negative") }),
    }
}

/// LocOS `_ocr_render_scale`: 150 dpi, lowered until the longer edge fits; `None` for a page with no usable size.
pub fn ocr_render_scale(w: f64, h: f64, max_dim: f64) -> Option<f64> {
    let mut scale = OCR_RENDER_DPI / 72.0;
    let longest = w.max(h);
    if longest.is_nan() || longest <= 0.0 || longest.is_infinite() {
        return None;
    }
    if longest * scale > max_dim {
        scale = max_dim / longest;
    }
    Some(scale)
}

fn parse_err(msg: &str) -> Error {
    Error::Parse { format: "pdf".into(), msg: msg.into() }
}

fn budget_error(budget: u64) -> Error {
    LimitError::new(LimitKind::ZipBytes, format!("The streams of this PDF unpack to more than {} MB. Split it into smaller files.", budget.div_ceil(1024 * 1024))).into()
}

fn work_error() -> Error {
    LimitError::new(LimitKind::Units, "This PDF needs far too much work to read its fonts and glyphs. Split it into smaller files.").into()
}

fn open_error(e: doc::OpenError, budget: u64) -> Error {
    match e {
        doc::OpenError::Format(m) => parse_err(m),
        doc::OpenError::Password => Error::Unsupported("pdf: this PDF is protected with a password".into()),
        doc::OpenError::Security => Error::Unsupported("pdf: this PDF uses a security handler that is not supported".into()),
        doc::OpenError::Budget => budget_error(budget),
    }
}

/// Clean PDFium's page text for units (#808). PDFium writes U+FFFE in two places, and both are dropped:
/// - a hyphen at a line end that breaks a word (`textpage.rs`, `Gen::Hyphen`): PDFium puts no line break after it,
///   the next line's text follows at once, so dropping the marker joins the word (`extraor` + `dinarily`);
/// - a glyph whose font maps it to U+0000 (`textpage.rs`, `object_items`): it has no text, so it is removed.
///
/// A hyphen PDFium does not mark (inside a line, or at a line end before a non-letter) is an ordinary `-` and stays.
/// The line break after an unmapped glyph stays too. C0 control characters (the raw char codes of a font with no
/// mapping to Unicode) are removed, except tab and the line breaks `\n` and `\r`.
fn clean_text(text: &str) -> String {
    text.chars().filter(|&c| c != '\u{fffe}' && (c >= ' ' || matches!(c, '\t' | '\n' | '\r'))).collect()
}

/// The characters of a page that count toward the OCR threshold: the cleaned text, stripped, without U+FFFD.
/// A page of garbage glyph codes counts as empty, so it goes to OCR.
fn printable_count(text: &str) -> usize {
    let kept: String = text.chars().filter(|&c| c != '\u{fffd}').collect();
    py_strip(&clean_text(&kept)).chars().count()
}

/// LocOS `_paragraphs`: split on blank lines (`\n\s*\n`), join each block's stripped, non-empty lines with a space.
pub fn paragraphs(text: &str) -> Vec<String> {
    let cleaned = clean_text(text);
    let t = py_strip(&cleaned);
    let chars: Vec<char> = t.chars().collect();
    let mut blocks: Vec<String> = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\n' {
            // The longest whitespace run after it that ends in another newline.
            let mut j = i + 1;
            let mut last_nl = None;
            while j < chars.len() && py_isspace_char(chars[j]) {
                if chars[j] == '\n' {
                    last_nl = Some(j);
                }
                j += 1;
            }
            if let Some(k) = last_nl {
                blocks.push(chars[start..i].iter().collect());
                start = k + 1;
                i = k + 1;
                continue;
            }
        }
        i += 1;
    }
    blocks.push(chars[start..].iter().collect());
    let mut out = Vec::new();
    for b in blocks {
        let lines: Vec<&str> = py_splitlines(&b).into_iter().map(py_strip).filter(|l| !l.is_empty()).collect();
        let joined = lines.join(" ");
        if !joined.is_empty() {
            out.push(joined);
        }
    }
    out
}

/// Python `str.splitlines()` (without keepends).
fn py_splitlines(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut it = s.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        let brk = matches!(c, '\n' | '\r' | '\x0b' | '\x0c' | '\x1c' | '\x1d' | '\x1e' | '\u{85}' | '\u{2028}' | '\u{2029}');
        if brk {
            out.push(&s[start..i]);
            let mut end = i + c.len_utf8();
            if c == '\r' {
                if let Some(&(_, '\n')) = it.peek() {
                    it.next();
                    end += 1;
                }
            }
            start = end;
        }
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

pub struct Pdf;

impl Filter for Pdf {
    fn id(&self) -> &'static str {
        "pdf"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["pdf"]
    }

    /// `%PDF-` in the first 1024 bytes (PDFium's header search).
    fn sniff(&self, bytes: &[u8]) -> bool {
        let head = &bytes[..bytes.len().min(1024)];
        doc::find(head, b"%PDF-", 0).is_some()
    }

    fn format_option_keys(&self) -> &'static [FormatOptionKey] {
        &OPTION_KEYS
    }

    fn extract(&self, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        let limits = opts.limits();
        let pages = page_texts(bytes, opts)?;
        let mut units: Vec<Unit> = Vec::new();
        let mut warnings = Vec::new();
        for p in &pages {
            let mut k = 0;
            if p.ocr {
                warnings.push(Warning::new(
                    "pdf_ocr_page",
                    format!("page {}", p.page + 1),
                    format!("Page {} has no text layer (a scanned page); it needs OCR.", p.page + 1),
                ));
            } else {
                for para in paragraphs(&p.text) {
                    k += 1;
                    let mut u = Unit::new(format!("p{}u{}", p.page + 1, k), vec![Inline::text(para)]);
                    u.context.kind = Some("paragraph".into());
                    u.context.location = Some(format!("page {} paragraph {}", p.page + 1, k));
                    u.context.extra.insert("page".into(), (p.page + 1).to_string());
                    units.push(u);
                    guard::check_unit_count(units.len(), &limits)?;
                }
            }
        }
        let skeleton = Skeleton::binary(bytes.to_vec(), bilingua_model::parser_info!());
        guard::finish(Document::new("pdf", units, skeleton).with_options(opts), warnings, &limits)
    }

    fn merge_with(&self, doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
        bilingua_model::check_own_options(self, &opts.format_options)?;
        bilingua_model::check_parser(doc, &bilingua_model::parser_info!())?;
        let Skeleton::Binary { data: original, .. } = &doc.skeleton else {
            return Err(Error::Write("pdf needs the skeleton made by its own extract".into()));
        };
        let original = original.clone();
        if !MergeOptions::any_translated(doc) && !opts.should_stamp(doc) {
            return Ok(original);
        }
        Err(Error::Unsupported(
            "pdf: a translated PDF is re-typeset from the unit targets (Localization OS builds it with typst); this crate does not include a typesetter".into(),
        ))
    }
}

/// The `format_options` keys this filter reads.
static OPTION_KEYS: [FormatOptionKey; 5] = [
    FormatOptionKey::int("pdf.max_pages", "Most pages (Localization OS PDF_MAX_PAGES, default 1000)."),
    FormatOptionKey::int("pdf.max_stream_bytes", "Most bytes all streams of the file may unpack to (default the zip limit, 256 MB)."),
    FormatOptionKey::int("pdf.ocr_min_chars", "A page with less text than this is a scanned page for OCR (default 20)."),
    FormatOptionKey::int("pdf.ocr_max_pixel_dim", "Longest edge of a scanned page rendered for OCR (PDF_OCR_MAX_PIXEL_DIM, default 4000)."),
    FormatOptionKey::int("pdf.ocr_max_total_pixels", "Total render pixels of the scanned pages (PDF_OCR_MAX_TOTAL_PIXELS, default 2500000000)."),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locos_paragraph_split() {
        assert_eq!(paragraphs("Hello world.\r\nSecond line."), vec!["Hello world. Second line."]);
        assert_eq!(paragraphs("  a\n\n b \n c\n \n\nd  "), vec!["a", "b c", "d"]);
        assert_eq!(paragraphs("a\u{3000}\n\u{a0}\nb"), vec!["a", "b"]);
        assert_eq!(paragraphs("extraor\u{fffe}dinarily"), vec!["extraordinarily"]);
        assert!(paragraphs(" \r\n ").is_empty());
        assert_eq!(py_splitlines("a\r\nb\rc\x0bd\u{2028}e"), vec!["a", "b", "c", "d", "e"]);
    }

    #[test]
    fn hyphen_marker_is_joined_and_unmapped_glyphs_are_dropped() {
        // PDFium's line-end hyphen marker has the next line's text right after it: the word is joined.
        assert_eq!(paragraphs("require extraor\u{fffe}dinarily long text"), vec!["require extraordinarily long text"]);
        // An unmapped glyph mid-word is dropped: no hyphen is invented.
        assert_eq!(paragraphs("o\u{fffe}ce work"), vec!["oce work"]);
        // An unmapped glyph at a line end keeps the line break, so the words of the two lines stay apart.
        assert_eq!(paragraphs("first\u{fffe}\r\nsecond"), vec!["first second"]);
        // A visible hyphen stays.
        assert_eq!(paragraphs("well-known\r\npage 12-\r\n14"), vec!["well-known page 12- 14"]);
        assert!(paragraphs("a\u{fffe}\r\nb\u{fffe}c").iter().all(|p| !p.contains('\u{fffe}') && !p.contains('-')));
    }

    #[test]
    fn c0_controls_are_removed_and_do_not_count_for_ocr() {
        assert_eq!(paragraphs("ab\u{1}\u{5}c\u{b}d\u{1f}e\tf\r\ng"), vec!["abcde\tf g"]);
        let garbage: String = "\u{1}\u{5}\u{7}".repeat(20);
        assert!(paragraphs(&garbage).is_empty());
        assert_eq!(printable_count(&garbage), 0);
        assert_eq!(printable_count("\u{fffd}\u{fffe}\u{fffe}\u{fffd}"), 0);
        assert_eq!(printable_count("  hello \u{3} world  "), 12);
    }

    #[test]
    fn render_scale() {
        assert_eq!(ocr_render_scale(595.0, 842.0, 4000.0), Some(150.0 / 72.0));
        assert_eq!(ocr_render_scale(3000.0, 3000.0, 4000.0), Some(4000.0 / 3000.0));
        assert_eq!(ocr_render_scale(0.0, 0.0, 4000.0), None);
    }
}

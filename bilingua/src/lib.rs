//! Bilingua: open a source file for translation, segment it, and write it back.
//!
//! ```ignore
//! let opts = ExtractOptions { target_lang: Some("es-ES".into()), ..Default::default() };
//! let mut doc = bilingua::extract("page.html", &bytes, &opts)?;
//! doc.set_target("p3#1", vec![Inline::text("Hola.")])?;
//! let out: Vec<u8> = bilingua::merge(&doc)?;
//! ```
//!
//! Formats register in [`filters`]: DOCX, XLSX, PPTX, HTML, Markdown, JSON, JSON5 and JSONC, PO, SRT, WebVTT, plain text, XLIFF 1.2 and 2
//! (with SDLXLIFF, MQXLIFF and MXLIFF) and the Trados and memoQ packages. `extract` also applies sentence
//! segmentation (`bilingua-seg`) for the granularity in the options.

pub use bilingua_model::*;
pub use bilingua_seg::{join, split, Rules};

/// All available filters, most specific first. Plain text goes last because it accepts any UTF-8 file.
pub fn filters() -> Vec<Box<dyn Filter>> {
    // The packages (zip) and the XLIFF family are recognised by extension or header; they go before the loose
    // sniffers.
    let mut all: Vec<Box<dyn Filter>> = bilingua_sdl::filters();
    all.extend(bilingua_xliff::filters());
    // OpenDocument (zip): a strict mimetype sniff, so it goes with the other packages.
    all.extend(bilingua_odf::filters());
    all.extend(vec![
        // Xcode localization bundle (a zip): by extension or by its contents.json and Localized Contents folder.
        Box::new(bilingua_xcstrings::Xcloc) as Box<dyn Filter>,
        // EPUB (a zip): by extension or by its stored mimetype; a strict sniff, so it cannot take a Word file.
        Box::new(bilingua_epub::Epub),
        // IDML sniffs strictly (its own mimetype or a design map with stories), so it sits beside the other packages.
        Box::new(bilingua_idml::Idml),
        Box::new(bilingua_docx::Docx),
        Box::new(bilingua_xlsx::Xlsx::default()),
        Box::new(bilingua_pptx::Pptx),
        // Visio sniffs a zip holding visio/ parts; docx, pptx and xlsx need word/, ppt/ and xl/, so it cannot clash.
        Box::new(bilingua_vsdx::Vsdx),
        Box::new(bilingua_subtitle::Vtt),
        Box::new(bilingua_subtitle::Srt),
        Box::new(bilingua_po::Po),
        Box::new(bilingua_wxl::Wxl),
        Box::new(bilingua_resjson::Resjson),
        // XMB and XTB are recognised by extension or by their root element (a tight sniff).
        Box::new(bilingua_xmb::Xmb),
        Box::new(bilingua_xmb::Xtb),
        // InCopy ICML and WoodWing WCML sniff strictly (InCopyInterchange snippet type or the Smart Connection namespace).
        Box::new(bilingua_icml::Icml),
        // ARB before JSON: a no-extension file with a string @@locale is ARB.
        Box::new(bilingua_arb::Arb),
        // String Catalog: JSON with a string sourceLanguage and an object strings (strict sniff), before plain JSON.
        Box::new(bilingua_xcstrings::Xcstrings),
        Box::new(bilingua_json::JsonFilter),
        Box::new(bilingua_json5::Json5Filter::default()),
        Box::new(bilingua_markdown::MarkdownFilter),
        // .desktop sniffs a [Desktop Entry] line; it must come before INI, which would also accept the file.
        Box::new(bilingua_desktop::DesktopFilter),
        Box::new(bilingua_desktop::CatkeysFilter),
        // AsciiDoc is plain text with no sniffer (extension only), like Markdown.
        Box::new(bilingua_adoc::Adoc),
        // MDX is plain text claimed by extension only (its sniff is always false).
        Box::new(bilingua_mdx::Mdx),
        Box::new(bilingua_ini::IniFilter),
        Box::new(bilingua_strings::Strings),
        // DTD sniffs strictly (the first declaration must be an ENTITY, ELEMENT or ATTLIST), so it can sit early.
        Box::new(bilingua_dtd::DtdFilter),
        // a stringsdict is a plist: it sniffs first; both are claimed by extension, and sniff only an XML or binary plist
        Box::new(bilingua_plist::Stringsdict),
        Box::new(bilingua_plist::Plist),
        // rc sniffs only resource-script keywords (STRINGTABLE, DIALOG, MENU, VERSIONINFO, winres.h); strict enough to sit early.
        Box::new(bilingua_rc::Rc),
        // XPress Tags sniffs only the <vNN.NN> / <eN> header at the very start of the file (also UTF-16); strict enough to sit early.
        Box::new(bilingua_xtg::Xtg),
        // LaTeX sniffs strictly (documentclass, usepackage or begin-document at a line start); chapter files go by extension.
        Box::new(bilingua_latex::Latex::default()),
        // TOML after INI: a valid-TOML INI file without an extension should stay INI.
        Box::new(bilingua_toml::TomlFilter),
        // Qt Linguist .ts sniffs strictly (XML with a <TS> root); `detect` decides the shared .ts extension.
        Box::new(bilingua_qtts::QtTs),
        Box::new(bilingua_jsmodule::JsModule),
        // PHP language arrays sniff strictly (the file must open with <?php and parse as a static array).
        Box::new(bilingua_php::Php),
        // MIF sniffs strictly: the first statement must be <MIFFile version>.
        Box::new(bilingua_mif::Mif),
        // server pages sniff a code marker (<?php, <%@, <jsp:, <!--#): never a plain HTML page; ERB (<%=) is the loosest
        Box::new(bilingua_serverpages::Phtml),
        Box::new(bilingua_serverpages::Jsp),
        Box::new(bilingua_serverpages::Aspx),
        Box::new(bilingua_serverpages::Shtml),
        Box::new(bilingua_serverpages::Erb),
        // resx sniffs XML with <root> and resource headers; it goes before the loose key-value sniffers.
        Box::new(bilingua_resx::Resx),
        // MO sniffs only its binary magic number, so its place does not matter.
        Box::new(bilingua_mo::Mo),
        // YAML sniffs any `word: text` line, so it goes after the stricter formats and before properties.
        Box::new(bilingua_yaml::Yaml),
        // properties sniffs any key=value or key: value text, so it goes after the stricter key-value formats.
        Box::new(bilingua_properties::Properties),
        Box::new(bilingua_xaml::Xaml::default()),
        // XSLT sniffs only XML that names the XSLT namespace in its first 4 KB, so it sits with the other XML formats.
        Box::new(bilingua_xslt::Xslt),
        // Fluent sniffs only with a Fluent-only marker, but its text looks like key = value: after the others.
        Box::new(bilingua_ftl::FtlFilter),
        // generic XML sniffs any well-formed document, so it goes after every filter that owns an XML vocabulary.
        Box::new(bilingua_xml::XmlFilter::new()),
        // CSV and TSV sniff strictly (three aligned lines) but a delimited table looks like nothing else: after the
        // key-value formats, before the loose sniffers.
        Box::new(bilingua_csv::CsvFilter::default()),
        Box::new(bilingua_csv::TsvFilter::default()),
        // reStructuredText sniffs loosely (a directive, a role, or an underlined title), so it goes after the strict ones.
        Box::new(bilingua_rst::Rst),
        // HTML sniffs loosely and plain text accepts any UTF-8, so they go last.
        Box::new(bilingua_html::Html),
        Box::new(bilingua_text::TextFilter),
    ]);
    all
}

/// Formats whose files carry their own segmentation (XLIFF and its dialects, translation packages): sentence
/// splitting would break the link between a segment and the file's own segment.
fn own_segmentation(id: &str) -> bool {
    matches!(id, "xliff" | "sdlxliff" | "mqxliff" | "mxliff" | "sdlppx" | "mqxlz" | "xcloc")
}

/// The filter for a file: by extension first, then by sniffing the bytes.
pub fn detect(name: &str, bytes: &[u8]) -> Option<Box<dyn Filter>> {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    // `.ts` is shared by Qt Linguist (XML) and TypeScript: the bytes decide, as in LocOS.
    if ext == "ts" {
        return Some(if bilingua_qtts::QtTs.sniff(bytes) { Box::new(bilingua_qtts::QtTs) } else { Box::new(bilingua_jsmodule::JsModule) });
    }
    // LocOS sends a .json file in ARB shape (a string @@locale) to its ARB dialect; so do we.
    if ext == "json" && bilingua_arb::Arb.sniff(bytes) {
        return Some(Box::new(bilingua_arb::Arb));
    }
    let all = filters();
    let mut by_ext = None;
    let mut sniffed = None;
    for f in all {
        if by_ext.is_none() && f.extensions().contains(&ext.as_str()) {
            by_ext = Some(f);
        } else if sniffed.is_none() && f.sniff(bytes) {
            sniffed = Some(f);
        }
    }
    // A file with an extension no parser claims is refused rather than guessed: plain text and HTML sniff
    // loosely, and opening a YAML or config file as plain text would let translators change its keys.
    // Sniffing only decides for files with no extension.
    if by_ext.is_none() && !ext.is_empty() {
        return None;
    }
    by_ext.or(sniffed)
}

/// Extracts and segments a file. The returned document's `format` names the filter that `merge` will use.
/// The source language comes from `opts.source_lang` (default "en"); it picks the sentence rules.
pub fn extract(name: &str, bytes: &[u8], opts: &ExtractOptions) -> Result<Document, Error> {
    let f = detect(name, bytes).ok_or_else(|| Error::Unsupported(format!("no parser for {name}")))?;
    // the safety net for every format, also one that forgets to call the guards: input size first, the unit,
    // segment and unit-length caps at the end
    bilingua_model::guard::check_input(bytes, &opts.limits())?;
    let mut doc = f.extract(bytes, opts)?;
    doc.name = Some(name.to_string());
    if own_segmentation(f.id()) {
        bilingua_model::guard::check_document(&doc, &opts.limits())?;
        return Ok(doc);
    }
    let lang = opts.source_lang.as_deref().unwrap_or("en");
    bilingua_seg::segment(&mut doc, lang, opts.granularity, Rules::bundled());
    // sentences count against the segment cap too (LocOS checks again after splitting)
    bilingua_model::guard::check_document(&doc, &opts.limits())?;
    Ok(doc)
}

/// Changes the granularity of an extracted document (sentence, paragraph or document). Targets already set
/// survive only for units whose segments all had one. A bilingual file (XLIFF family, packages) keeps the file's
/// own segments: merge finds them by id, so joining or splitting them would lose the translations.
pub fn resegment(doc: &mut Document, source_lang: &str, mode: Granularity) {
    if own_segmentation(&doc.format) {
        return;
    }
    bilingua_seg::segment(doc, source_lang, mode, Rules::bundled());
}

fn filter_for(doc: &Document) -> Result<Box<dyn Filter>, Error> {
    filters()
        .into_iter()
        .find(|f| f.id() == doc.format)
        .ok_or_else(|| Error::Unsupported(format!("no writer for format {}", doc.format)))
}

/// Writes the document back in its original format.
pub fn merge(doc: &Document) -> Result<Vec<u8>, Error> {
    filter_for(doc)?.merge(doc)
}

/// [`merge`] with [`MergeOptions`]: `output_encoding` (keep the file's encoding, or write UTF-8 and make the
/// declarations say so) and `stamp_target_always` (stamp the target language and fill plurals even when nothing is
/// translated, as Localization OS does). `merge(doc)` is `merge_with(doc, &MergeOptions::default())`.
pub fn merge_with(doc: &Document, opts: &MergeOptions) -> Result<Vec<u8>, Error> {
    filter_for(doc)?.merge_with(doc, opts)
}

/// Translated HTML preview with `data-bl-seg` spans, for formats that have one.
pub fn preview(doc: &Document) -> Option<String> {
    filter_for(doc).ok()?.preview(doc)
}

/// Inner HTML for one segment's preview span, for live patches.
pub fn render_segment(doc: &Document, seg_id: &str) -> Option<String> {
    filter_for(doc).ok()?.render_segment(doc, seg_id)
}

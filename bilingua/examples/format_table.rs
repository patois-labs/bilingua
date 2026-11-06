//! Prints the format table of the workspace README from `bilingua::filters()`: one row per filter id, with its
//! extensions and whether it is a bilingual (translation exchange) format. `tools/gen-format-table.sh` runs it and
//! splices the output into README.md.
//!
//!     cargo run -q -p bilingua --example format_table            the Markdown table
//!     cargo run -q -p bilingua --example format_table -- --raw   id, extensions, bilingual (tab separated)
//!
//! A new filter id needs a line in `name()`; the example stops with the id until it has one, so the README never
//! shows an id without a name.

fn name(id: &str) -> Option<&'static str> {
    Some(match id {
        "adoc" => "AsciiDoc",
        "arb" => "Flutter ARB",
        "aspx" => "ASP.NET pages and controls",
        "ass" => "Advanced SubStation Alpha and SSA subtitles",
        "catkeys" => "Haiku catkeys",
        "csv" => "CSV",
        "desktop" => "freedesktop .desktop entries",
        "docx" => "Microsoft Word",
        "dokuwiki" => "DokuWiki markup",
        "dtd" => "DTD entity files (Mozilla)",
        "dxf" => "AutoCAD DXF (ASCII)",
        "eml" => "Email messages",
        "epub" => "EPUB 2 and 3",
        "erb" => "ERB templates",
        "flprjzip" => "MadCap Flare project",
        "ftl" => "Mozilla Fluent",
        "hhc" => "HTML Help table of contents",
        "hhk" => "HTML Help index",
        "hhp" => "HTML Help project",
        "html" => "HTML and XHTML",
        "icml" => "Adobe InCopy ICML and WoodWing WCML",
        "idml" => "Adobe InDesign IDML",
        "image" => "Raster images (text layer: PNG, JPEG, TIFF, BMP, GIF, WebP)",
        "ini" => "INI and Joomla language files",
        "js" => "JavaScript and TypeScript locale modules",
        "json" => "JSON",
        "json5" => "JSON5 and JSONC",
        "jsp" => "JavaServer Pages",
        "markdown" => "Markdown (CommonMark, GitHub tables)",
        "mdx" => "MDX",
        "mif" => "Adobe FrameMaker MIF",
        "mo" => "gettext MO (binary)",
        "mqxliff" => "memoQ MQXLIFF",
        "mqxlz" => "memoQ package (MQXLZ)",
        "mxliff" => "Phrase MXLIFF",
        "odg" => "OpenDocument drawing",
        "odp" => "OpenDocument presentation",
        "ods" => "OpenDocument spreadsheet",
        "odt" => "OpenDocument text",
        "php" => "PHP array language files (Laravel, CodeIgniter)",
        "phtml" => "PHP templates",
        "plist" => "Apple property lists (XML and binary)",
        "po" => "gettext PO and POT",
        "pdf" => "PDF (text layer, extract only)",
        "pptx" => "Microsoft PowerPoint",
        "properties" => "Java and Mozilla .properties",
        "rc" => "Windows resource scripts",
        "resjson" => "Windows .resjson",
        "resx" => "Microsoft .NET resx and resw",
        "rst" => "reStructuredText",
        "sbv" => "YouTube SBV subtitles",
        "scc" => "Scenarist Closed Captions",
        "sdlppx" => "Trados project and return packages",
        "sdlproj" => "Trados Studio project file",
        "sdlxliff" => "Trados SDLXLIFF",
        "shtml" => "Server-side includes (SSI)",
        "srt" => "SubRip subtitles",
        "stf" => "Salesforce Translation Workbench",
        "stl" => "EBU STL subtitles",
        "strings" => "Apple .strings",
        "stringsdict" => "Apple .stringsdict",
        "sub" => "SubViewer 2.0 and MicroDVD subtitles",
        "svg" => "SVG text",
        "tex" => "LaTeX",
        "toml" => "TOML",
        "ts" => "Qt Linguist TS",
        "tsv" => "TSV",
        "ttml" => "TTML and DFXP subtitles",
        "ttx" => "Trados TagEditor TTX",
        "txlf" => "Wordfast Pro TXLF",
        "txml" => "Wordfast Pro TXML",
        "txt" => "Plain text",
        "vsdx" => "Microsoft Visio",
        "vtt" => "WebVTT subtitles",
        "wiki" => "MediaWiki markup",
        "wxl" => "WiX localization",
        "xaml" => "XAML resources (WPF, UWP, WinUI, MAUI, Avalonia)",
        "xcloc" => "Xcode localization bundle",
        "xcstrings" => "Xcode String Catalog",
        "xliff" => "XLIFF 1.2 and 2",
        "xlsx" => "Microsoft Excel",
        "xlz" => "WorldServer and Idiom XLIFF packages",
        "xmb" => "Google and Angular XMB",
        "dita" => "DITA topics, maps and bookmaps",
        "docbook" => "DocBook 4.x and 5",
        "xml" => "XML with ITS 2.0 rules",
        "xslt" => "XSLT stylesheets",
        "xtb" => "Google and Angular XTB",
        "xtg" => "QuarkXPress Tags",
        "yaml" => "YAML (generic, Rails i18n, Symfony)",
        _ => return None,
    })
}

fn main() {
    let raw = std::env::args().any(|a| a == "--raw");
    let mut missing = Vec::new();
    let mut rows = Vec::new();
    for f in bilingua::filters() {
        let exts: Vec<String> = f.extensions().iter().map(|e| format!("`.{}`", e.trim_start_matches('.'))).collect();
        if raw {
            println!("{}\t{}\t{}", f.id(), f.extensions().join(","), f.bilingual());
            continue;
        }
        match name(f.id()) {
            Some(n) => rows.push((n, f.id(), exts.join(" "), f.bilingual())),
            None => missing.push(f.id()),
        }
    }
    if raw {
        return;
    }
    if !missing.is_empty() {
        eprintln!("format_table: add a name for these filter ids in bilingua/examples/format_table.rs: {missing:?}");
        std::process::exit(1);
    }
    rows.sort_by_key(|r| r.0.to_lowercase());
    println!("| Format | Filter id | Extensions | Bilingual |");
    println!("|---|---|---|---|");
    for (n, id, exts, bi) in &rows {
        let exts = if exts.is_empty() { "by content".to_string() } else { exts.clone() };
        println!("| {n} | `{id}` | {exts} | {} |", if *bi { "yes" } else { "" });
    }
    println!();
    println!("{} formats.", rows.len());
}

//! A made-up presentation built in code, for tests.

use crate::zipio::{write_zip, EntryMeta};

pub const NS_A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
pub const NS_P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
pub const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// The 11 primary languages, one line each.
pub const LINES: [(&str, &str); 11] = [
    ("en", "Welcome to the show"),
    ("es", "Bienvenidos al espectáculo"),
    ("fr", "Bienvenue au spectacle"),
    ("it", "Benvenuti allo spettacolo"),
    ("de", "Willkommen zur Show"),
    ("nl", "Welkom bij de show"),
    ("pt", "Bem-vindos ao espetáculo"),
    ("ja", "ショーへようこそ"),
    ("zh", "欢迎观看演出"),
    ("ko", "쇼에 오신 것을 환영합니다"),
    ("ar", "مرحبا بكم في العرض"),
];

pub fn zip(files: &[(&str, String)]) -> Vec<u8> {
    let entries: Vec<(EntryMeta, Vec<u8>)> = files
        .iter()
        .map(|(n, d)| {
            (
                EntryMeta { name: (*n).to_string(), stored: false, dir: false, modified: Some([2026, 10, 28, 9, 30, 0]) },
                d.clone().into_bytes(),
            )
        })
        .collect();
    write_zip(&entries).expect("fixture zip")
}

fn hdr() -> &'static str {
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n"
}

fn nv(id: u32, name: &str, ph: &str) -> String {
    format!("<p:nvSpPr><p:cNvPr id=\"{id}\" name=\"{name}\"/><p:cNvSpPr/><p:nvPr>{ph}</p:nvPr></p:nvSpPr><p:spPr/>")
}

fn para(text: &str) -> String {
    format!("<a:p><a:pPr lvl=\"0\"/><a:r><a:rPr lang=\"en-US\" dirty=\"0\"/><a:t>{text}</a:t></a:r><a:endParaRPr lang=\"en-US\"/></a:p>")
}

fn sp(id: u32, name: &str, ph: &str, paras: &str) -> String {
    format!("<p:sp>{}<p:txBody><a:bodyPr/><a:lstStyle/>{paras}</p:txBody></p:sp>", nv(id, name, ph))
}

fn slide(shapes: &str) -> String {
    format!(
        "{}<p:sld xmlns:a=\"{NS_A}\" xmlns:p=\"{NS_P}\" xmlns:r=\"{NS_R}\" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{shapes}</p:spTree></p:cSld></p:sld>",
        hdr()
    )
}

pub fn slide1() -> String {
    let body: String = LINES.iter().map(|(_, t)| para(t)).collect();
    let rich = "<a:p><a:pPr algn=\"l\"/><a:r><a:rPr lang=\"en-US\" dirty=\"0\"/><a:t>Plain start, </a:t></a:r><a:r><a:rPr lang=\"en-US\" b=\"1\" dirty=\"0\"/><a:t>bold middle</a:t></a:r><a:r><a:rPr lang=\"en-US\" dirty=\"0\"/><a:t> and an </a:t></a:r><a:r><a:rPr lang=\"en-US\"><a:hlinkClick r:id=\"rId9\"/></a:rPr><a:t>example link</a:t></a:r><a:br><a:rPr lang=\"en-US\"/></a:br><a:r><a:rPr lang=\"en-US\" err=\"1\"/><a:t>second line tex</a:t></a:r><a:fld id=\"{B1}\" type=\"slidenum\"><a:rPr lang=\"en-US\"/><a:t>3</a:t></a:fld><a:endParaRPr lang=\"en-US\"/></a:p>";
    let table = "<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id=\"9\" name=\"Table 8\"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm/><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/table\"><a:tbl><a:tblPr/><a:tblGrid><a:gridCol w=\"1000\"/><a:gridCol w=\"1000\"/></a:tblGrid><a:tr h=\"100\"><a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\"/><a:t>Product</a:t></a:r></a:p></a:txBody><a:tcPr/></a:tc><a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\"/><a:t>Precio</a:t></a:r></a:p></a:txBody><a:tcPr/></a:tc></a:tr><a:tr h=\"100\"><a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\"/><a:t>Widget</a:t></a:r></a:p></a:txBody><a:tcPr/></a:tc><a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\"/><a:t>12.50</a:t></a:r></a:p></a:txBody><a:tcPr/></a:tc></a:tr></a:tbl></a:graphicData></a:graphic></p:graphicFrame>";
    let group = format!(
        "<p:grpSp><p:nvGrpSpPr><p:cNvPr id=\"20\" name=\"Group 19\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{}</p:grpSp>",
        sp(21, "Grouped box", "", &para("Inside a group"))
    );
    let numfield = "<a:p><a:fld id=\"{B2}\" type=\"slidenum\"><a:rPr lang=\"en-US\"/><a:t>&#8249;#&#8250;</a:t></a:fld><a:endParaRPr lang=\"en-US\"/></a:p>";
    slide(&format!(
        "{}{}{}{}{}{}{}{}",
        sp(2, "Title 1", "<p:ph type=\"title\"/>", &para("Welcome to the demo")),
        sp(3, "Subtitle 2", "<p:ph type=\"subTitle\" idx=\"1\"/>", &para("A short subtitle")),
        sp(4, "Content Placeholder 3", "<p:ph idx=\"2\"/>", &body),
        sp(5, "TextBox 4", "", rich),
        table,
        group,
        sp(6, "Slide Number Placeholder 5", "<p:ph type=\"sldNum\" idx=\"12\"/>", numfield),
        sp(7, "Empty", "", "<a:p><a:endParaRPr lang=\"en-US\"/></a:p><a:p><a:r><a:rPr lang=\"en-US\"/><a:t>  </a:t></a:r></a:p><a:p><a:r><a:rPr lang=\"en-US\"/><a:t>2026</a:t></a:r></a:p>")
    ))
}

pub fn slide2() -> String {
    let gf = "<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id=\"30\" name=\"Diagram 29\"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm/><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/diagram\"/></a:graphic></p:graphicFrame>";
    let alt = "<mc:AlternateContent><mc:Choice Requires=\"a14\">".to_string()
        + &sp(40, "Choice box", "", &para("Choice text"))
        + "</mc:Choice><mc:Fallback>"
        + &sp(41, "Fallback box", "", &para("Choice text"))
        + "</mc:Fallback></mc:AlternateContent>";
    slide(&format!(
        "{}{}{}{}",
        sp(2, "Title 1", "<p:ph type=\"ctrTitle\"/>", &para("Second slide, first in the deck")),
        gf,
        alt,
        sp(3, "Mixed", "", "<a:p><a:pPr rtl=\"1\" algn=\"r\"/><a:r><a:rPr lang=\"ar-SA\"/><a:t>نص عربي أصلي</a:t></a:r></a:p>")
    ))
}

pub fn slide3() -> String {
    slide(&sp(2, "Title 1", "<p:ph type=\"title\"/>", &para("Last slide")))
}

pub fn presentation() -> String {
    format!(
        "{}<p:presentation xmlns:a=\"{NS_A}\" xmlns:p=\"{NS_P}\" xmlns:r=\"{NS_R}\"><p:sldMasterIdLst><p:sldMasterId id=\"2147483648\" r:id=\"rId10\"/></p:sldMasterIdLst><p:sldIdLst><p:sldId id=\"256\" r:id=\"rId2\"/><p:sldId id=\"257\" r:id=\"rId1\"/><p:sldId id=\"258\" r:id=\"rId3\"/></p:sldIdLst><p:sldSz cx=\"9144000\" cy=\"6858000\"/><p:notesSz cx=\"6858000\" cy=\"9144000\"/></p:presentation>",
        hdr()
    )
}

pub fn layout() -> String {
    format!(
        "{}<p:sldLayout xmlns:a=\"{NS_A}\" xmlns:p=\"{NS_P}\" xmlns:r=\"{NS_R}\" type=\"title\"><p:cSld name=\"Title Slide\"><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{}{}</p:spTree></p:cSld></p:sldLayout>",
        hdr(),
        sp(2, "Title 1", "<p:ph type=\"ctrTitle\"/>", &para("Click to edit Master title style")),
        sp(3, "Brand line", "", &para("Contoso Labs internal"))
    )
}

pub fn master() -> String {
    format!(
        "{}<p:sldMaster xmlns:a=\"{NS_A}\" xmlns:p=\"{NS_P}\" xmlns:r=\"{NS_R}\"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{}</p:spTree></p:cSld><p:txStyles><p:titleStyle><a:lvl1pPr><a:defRPr sz=\"4400\"/></a:lvl1pPr></p:titleStyle></p:txStyles></p:sldMaster>",
        hdr(),
        sp(2, "Title Placeholder 1", "<p:ph type=\"title\"/>", &para("Click to edit Master title style"))
    )
}

pub fn notes() -> String {
    format!(
        "{}<p:notes xmlns:a=\"{NS_A}\" xmlns:p=\"{NS_P}\" xmlns:r=\"{NS_R}\"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{}</p:spTree></p:cSld></p:notes>",
        hdr(),
        sp(3, "Notes Placeholder 2", "<p:ph type=\"body\" idx=\"1\"/>", &para("Remember to pause here."))
    )
}

pub fn dgm_data() -> String {
    let pt = |id: &str, t: &str| {
        format!("<dgm:pt modelId=\"{id}\"><dgm:prSet/><dgm:spPr/><dgm:t><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang=\"en-US\"/><a:t>{t}</a:t></a:r></a:p></dgm:t></dgm:pt>")
    };
    format!(
        "{}<dgm:dataModel xmlns:dgm=\"http://schemas.openxmlformats.org/drawingml/2006/diagram\" xmlns:a=\"{NS_A}\"><dgm:ptLst>{}{}</dgm:ptLst><dgm:cxnLst/></dgm:dataModel>",
        hdr(),
        pt("{1}", "First step"),
        pt("{2}", "Second step")
    )
}

pub fn dgm_drawing() -> String {
    let sp = |t: &str| {
        format!("<dsp:sp modelId=\"{{1}}\"><dsp:spPr/><dsp:txBody><a:bodyPr/><a:lstStyle/><a:p><a:pPr algn=\"ctr\"/><a:r><a:rPr lang=\"en-US\" sz=\"1800\"/><a:t>{t}</a:t></a:r></a:p></dsp:txBody></dsp:sp>")
    };
    format!(
        "{}<dsp:drawing xmlns:dsp=\"http://schemas.microsoft.com/office/drawing/2008/diagram\" xmlns:a=\"{NS_A}\"><dsp:spTree><dsp:nvGrpSpPr><dsp:cNvPr id=\"0\" name=\"\"/><dsp:cNvGrpSpPr/></dsp:nvGrpSpPr><dsp:grpSpPr/>{}{}</dsp:spTree></dsp:drawing>",
        hdr(),
        sp("First step"),
        sp("Second step")
    )
}

pub fn rels(items: &[(&str, &str, &str)]) -> String {
    let body: String = items
        .iter()
        .map(|(id, t, target)| {
            let ty = if t.starts_with("http") { (*t).to_string() } else { format!("{REL}/{t}") };
            format!("<Relationship Id=\"{id}\" Type=\"{ty}\" Target=\"{target}\"/>")
        })
        .collect();
    format!("{}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{body}</Relationships>", hdr())
}

pub fn presentation_zip() -> Vec<u8> {
    let ov = |p: &str, t: &str| format!("<Override PartName=\"{p}\" ContentType=\"application/vnd.openxmlformats-officedocument.{t}+xml\"/>");
    let ct = format!(
        "{}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>{}{}{}{}{}{}{}{}</Types>",
        hdr(),
        ov("/ppt/presentation.xml", "presentationml.presentation.main"),
        ov("/ppt/slides/slide1.xml", "presentationml.slide"),
        ov("/ppt/slides/slide2.xml", "presentationml.slide"),
        ov("/ppt/slides/slide3.xml", "presentationml.slide"),
        ov("/ppt/slideLayouts/slideLayout1.xml", "presentationml.slideLayout"),
        ov("/ppt/slideMasters/slideMaster1.xml", "presentationml.slideMaster"),
        ov("/ppt/notesSlides/notesSlide1.xml", "presentationml.notesSlide"),
        ov("/ppt/diagrams/data1.xml", "drawingml.diagramData")
    );
    zip(&[
        ("[Content_Types].xml", ct),
        ("_rels/.rels", rels(&[("rId1", "officeDocument", "ppt/presentation.xml")])),
        ("ppt/presentation.xml", presentation()),
        (
            "ppt/_rels/presentation.xml.rels",
            rels(&[
                ("rId1", "slide", "slides/slide1.xml"),
                ("rId2", "slide", "slides/slide2.xml"),
                ("rId3", "slide", "slides/slide3.xml"),
                ("rId10", "slideMaster", "slideMasters/slideMaster1.xml"),
            ]),
        ),
        ("ppt/slides/slide1.xml", slide1()),
        ("ppt/slides/slide2.xml", slide2()),
        ("ppt/slides/slide3.xml", slide3()),
        (
            "ppt/slides/_rels/slide1.xml.rels",
            rels(&[("rId1", "slideLayout", "../slideLayouts/slideLayout1.xml"), ("rId9", "hyperlink", "https://example.com/")]),
        ),
        (
            "ppt/slides/_rels/slide2.xml.rels",
            rels(&[
                ("rId1", "slideLayout", "../slideLayouts/slideLayout1.xml"),
                ("rId2", "notesSlide", "../notesSlides/notesSlide1.xml"),
                ("rId3", "diagramData", "../diagrams/data1.xml"),
                ("rId4", "http://schemas.microsoft.com/office/2007/relationships/diagramDrawing", "../diagrams/drawing1.xml"),
            ]),
        ),
        ("ppt/slides/_rels/slide3.xml.rels", rels(&[("rId1", "slideLayout", "../slideLayouts/slideLayout1.xml")])),
        ("ppt/slideLayouts/slideLayout1.xml", layout()),
        ("ppt/slideMasters/slideMaster1.xml", master()),
        ("ppt/notesSlides/notesSlide1.xml", notes()),
        ("ppt/diagrams/data1.xml", dgm_data()),
        ("ppt/diagrams/drawing1.xml", dgm_drawing()),
    ])
}

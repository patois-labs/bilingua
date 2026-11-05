//! Hand-written packages, ported from the LocOS suite (`tests/verify_vsdx_filter.py`).

use bilingua_zip::{write_zip, EntryMeta};

pub const NS: &str = "http://schemas.microsoft.com/office/visio/2012/main";
pub const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
pub const PAGE_REL: &str = "http://schemas.microsoft.com/visio/2010/relationships/page";

pub fn content_types(vba: bool) -> String {
    let main = if vba { "application/vnd.ms-visio.drawing.macroEnabled.main+xml" } else { "application/vnd.ms-visio.drawing.main+xml" };
    format!(
        "<?xml version='1.0' encoding='utf-8' standalone='yes'?><Types xmlns='http://schemas.openxmlformats.org/package/2006/content-types'><Default Extension='rels' ContentType='application/vnd.openxmlformats-package.relationships+xml'/><Default Extension='xml' ContentType='application/xml'/><Override PartName='/visio/document.xml' ContentType='{main}'/><Override PartName='/visio/pages/pages.xml' ContentType='application/vnd.ms-visio.pages+xml'/><Override PartName='/visio/pages/page1.xml' ContentType='application/vnd.ms-visio.page+xml'/><Override PartName='/visio/pages/page2.xml' ContentType='application/vnd.ms-visio.page+xml'/></Types>"
    )
}

pub fn pages() -> String {
    format!(
        "<?xml version='1.0' encoding='utf-8' ?>\r\n<Pages xmlns='{NS}' xmlns:r='{R}' xml:space='preserve'><Page ID='4' NameU='Overview' Name='Overview' ViewScale='1'><PageSheet/><Rel r:id='rId2'/></Page><Page ID='0' NameU='Page-1' Name='Process details'><PageSheet/><Rel r:id='rId1'/></Page></Pages>"
    )
}

pub fn pages_rels() -> String {
    format!(
        "<?xml version='1.0' encoding='utf-8' ?><Relationships xmlns='http://schemas.openxmlformats.org/package/2006/relationships'><Relationship Id='rId1' Type='{PAGE_REL}' Target='page1.xml'/><Relationship Id='rId2' Type='{PAGE_REL}' Target='page2.xml'/></Relationships>"
    )
}

pub fn page1() -> String {
    format!(
        "<?xml version='1.0' encoding='utf-8' ?>\r\n<PageContents xmlns='{NS}' xmlns:r='{R}' xml:space='preserve'><Shapes><Shape ID='1' Type='Shape' Master='2'><Cell N='PinX' V='1'/><Section N='Property'><Row N='Owner'><Cell N='Value' V='Ops' U='STR'/><Cell N='Label' V='Process owner'/><Cell N='Prompt' V=''/></Row><Row N='Calc'><Cell N='Label' V='Computed' F='Sheet.5!Prop.X'/></Row></Section><Text><cp IX='0'/><pp IX='0'/>Review the <cp IX='1'/>request<cp IX='0'/> by %1$s\n</Text></Shape><Shape ID='2' Type='Group'><Shapes><Shape ID='3' Type='Shape'><Text>Printed on <fld IX='0'>10/10/2026</fld>\n</Text></Shape><Shape ID='4' Type='Shape'><Text><fld IX='0'>10/10/2026</fld></Text></Shape><Shape ID='5' Type='Shape'><Text>  42  </Text></Shape></Shapes></Shape></Shapes></PageContents>"
    )
}

pub fn page2() -> String {
    format!(
        "<?xml version='1.0' encoding='utf-8' ?>\r\n<PageContents xmlns='{NS}' xmlns:r='{R}' xml:space='preserve'><Shapes><Shape ID='7' Type='Shape'><Text>Start</Text></Shape></Shapes></PageContents>"
    )
}

pub fn master() -> String {
    format!("<?xml version='1.0' encoding='utf-8' ?><MasterContents xmlns='{NS}'><Shapes><Shape ID='5'><Text>Master default</Text></Shape></Shapes></MasterContents>")
}

pub struct Pkg {
    pub pages: String,
    pub page1: String,
    pub page2: String,
    pub vba: bool,
    pub extra: Vec<(String, Vec<u8>)>,
}

impl Default for Pkg {
    fn default() -> Pkg {
        Pkg { pages: pages(), page1: page1(), page2: page2(), vba: false, extra: vec![] }
    }
}

pub fn zip_of(parts: Vec<(String, Vec<u8>)>) -> Vec<u8> {
    let v: Vec<(EntryMeta, Vec<u8>)> = parts
        .into_iter()
        .map(|(name, d)| (EntryMeta { name, stored: false, dir: false, modified: Some([2026, 10, 10, 12, 0, 0]) }, d))
        .collect();
    write_zip(&v).unwrap()
}

impl Pkg {
    pub fn build(&self) -> Vec<u8> {
        let mut parts: Vec<(String, Vec<u8>)> = vec![
            ("[Content_Types].xml".into(), content_types(self.vba).into_bytes()),
            ("_rels/.rels".into(), b"<Relationships xmlns='http://schemas.openxmlformats.org/package/2006/relationships'/>".to_vec()),
            ("visio/document.xml".into(), format!("<VisioDocument xmlns='{NS}'/>").into_bytes()),
            ("visio/pages/pages.xml".into(), self.pages.clone().into_bytes()),
            ("visio/pages/_rels/pages.xml.rels".into(), pages_rels().into_bytes()),
            ("visio/pages/page1.xml".into(), self.page1.clone().into_bytes()),
            ("visio/pages/page2.xml".into(), self.page2.clone().into_bytes()),
            ("visio/masters/master1.xml".into(), master().into_bytes()),
        ];
        if self.vba {
            parts.push(("visio/vbaProject.bin".into(), b"\xd0\xcf\x11\xe0VBA-PROJECT-BYTES".repeat(4)));
        }
        parts.extend(self.extra.iter().cloned());
        zip_of(parts)
    }
}

pub fn package() -> Vec<u8> {
    Pkg::default().build()
}

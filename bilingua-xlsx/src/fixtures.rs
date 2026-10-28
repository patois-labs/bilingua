//! A made-up workbook built in code, for tests.

use crate::zipio::{write_zip, EntryMeta};

pub const NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
pub const RNS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// The 11 primary languages, one greeting each.
pub const GREETINGS: [(&str, &str); 11] = [
    ("en", "Welcome"),
    ("es", "Bienvenido"),
    ("fr", "Bienvenue"),
    ("it", "Benvenuto"),
    ("de", "Willkommen"),
    ("nl", "Welkom"),
    ("pt", "Bem-vindo"),
    ("ja", "ようこそ"),
    ("zh", "欢迎"),
    ("ko", "환영합니다"),
    ("ar", "مرحبا بكم"),
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

pub fn sst_xml() -> String {
    let mut items: Vec<String> = vec![
        "<si><t>Product name</t></si>".into(),
        "<si><t>Price</t></si>".into(),
    ];
    for (_, g) in GREETINGS {
        items.push(format!("<si><t>{g}</t></si>"));
    }
    // 13: rich text
    items.push("<si><r><t xml:space=\"preserve\">Total </t></r><r><rPr><b/><sz val=\"11\"/></rPr><t>amount</t></r><r><t xml:space=\"preserve\"> due</t></r></si>".into());
    // 14: shared by two cells
    items.push("<si><t>Subtotal</t></si>".into());
    // 15: only digits
    items.push("<si><t>2024</t></si>".into());
    // 16: in a hidden row
    items.push("<si><t>Secret note</t></si>".into());
    // 17: in a hidden column
    items.push("<si><t>Hidden column text</t></si>".into());
    // 18: phonetic guide
    items.push("<si><r><t>漢字</t></r><rPh sb=\"0\" eb=\"2\"><t>かんじ</t></rPh><phoneticPr fontId=\"1\"/></si>".into());
    // 19: only on a hidden sheet
    items.push("<si><t>Only on the hidden sheet</t></si>".into());
    // 20: ampersand and quotes
    items.push("<si><t>Tom &amp; Jerry &lt;3 \"quoted\"</t></si>".into());
    // 21: unused
    items.push("<si><t>Never used by a cell</t></si>".into());
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<sst xmlns=\"{NS}\" count=\"30\" uniqueCount=\"{}\">{}</sst>",
        items.len(),
        items.join("")
    )
}

pub fn sheet1_xml() -> String {
    let mut rows = String::new();
    rows.push_str("<row r=\"1\"><c r=\"A1\" t=\"s\"><v>0</v></c><c r=\"B1\" t=\"s\"><v>1</v></c></row>");
    for i in 0..11 {
        let r = i + 2;
        rows.push_str(&format!(
            "<row r=\"{r}\"><c r=\"A{r}\" t=\"s\"><v>{}</v></c><c r=\"B{r}\"><v>{}</v></c></row>",
            2 + i,
            10 * (i + 1)
        ));
    }
    rows.push_str("<row r=\"13\"><c r=\"A13\" t=\"s\"><v>13</v></c><c r=\"B13\" t=\"inlineStr\"><is><t>Inline text</t></is></c></row>");
    rows.push_str("<row r=\"14\"><c r=\"A14\" t=\"s\"><v>14</v></c><c r=\"B14\" t=\"s\"><v>14</v></c><c r=\"C14\" t=\"str\"><f>A1&amp;\"x\"</f><v>Product namex</v></c></row>");
    rows.push_str("<row r=\"15\" hidden=\"1\"><c r=\"A15\" t=\"s\"><v>16</v></c></row>");
    rows.push_str("<row r=\"16\"><c r=\"A16\" t=\"s\"><v>15</v></c><c r=\"D16\" t=\"s\"><v>17</v></c></row>");
    rows.push_str("<row r=\"17\"><c r=\"A17\" t=\"s\"><v>18</v></c><c r=\"B17\" t=\"s\"><v>20</v></c></row>");
    rows.push_str("<row r=\"18\"><c r=\"A18\"><f>Oculta!A1</f><v>0</v></c><c r=\"B18\"><f>'Hoja de datos'!A1</f><v>0</v></c></row>");
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<worksheet xmlns=\"{NS}\" xmlns:r=\"{RNS}\"><sheetViews><sheetView workbookViewId=\"0\"/></sheetViews><cols><col min=\"4\" max=\"4\" width=\"12\" hidden=\"1\" customWidth=\"1\"/></cols><sheetData>{rows}</sheetData><dataValidations count=\"1\"><dataValidation type=\"textLength\" operator=\"lessThanOrEqual\" allowBlank=\"1\" sqref=\"A17\"><formula1>20</formula1></dataValidation></dataValidations><headerFooter><oddHeader>&amp;L&amp;\"Arial,Bold\"Quarterly report&amp;R&amp;P</oddHeader><oddFooter>&amp;CTom &amp;&amp; Jerry</oddFooter></headerFooter><tableParts count=\"1\"><tablePart r:id=\"rId2\"/></tableParts></worksheet>"
    )
}

pub fn sheet2_xml() -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<worksheet xmlns=\"{NS}\"><sheetData><row r=\"1\"><c r=\"A1\" t=\"s\"><v>19</v></c></row></sheetData></worksheet>"
    )
}

pub fn sheet3_xml() -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<worksheet xmlns=\"{NS}\"><sheetData><row r=\"1\"><c r=\"A1\"><v>1</v></c></row></sheetData></worksheet>"
    )
}

pub fn workbook_xml() -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<workbook xmlns=\"{NS}\" xmlns:r=\"{RNS}\"><sheets><sheet name=\"Ventas\" sheetId=\"1\" r:id=\"rId1\"/><sheet name=\"Oculta\" sheetId=\"2\" state=\"hidden\" r:id=\"rId2\"/><sheet name=\"Hoja de datos\" sheetId=\"3\" r:id=\"rId3\"/></sheets><definedNames><definedName name=\"Datos\">'Hoja de datos'!$A$1:$A$5</definedName></definedNames></workbook>"
    )
}

pub fn comments_xml() -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<comments xmlns=\"{NS}\"><authors><author>Ana</author></authors><commentList><comment ref=\"A3\" authorId=\"0\"><text><r><rPr><b/></rPr><t>Ana:</t></r><r><t xml:space=\"preserve\">\nPlease check the greeting.</t></r></text></comment></commentList></comments>"
    )
}

pub fn table_xml() -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<table xmlns=\"{NS}\" id=\"1\" name=\"Table1\" displayName=\"Table1\" ref=\"A1:B12\"><autoFilter ref=\"A1:B12\"/><tableColumns count=\"2\"><tableColumn id=\"1\" name=\"Product name\"/><tableColumn id=\"2\" name=\"Price\"/></tableColumns></table>"
    )
}

pub fn workbook_zip() -> Vec<u8> {
    let rel = |items: &[(&str, &str, &str)]| {
        let body: String = items
            .iter()
            .map(|(id, t, target)| format!("<Relationship Id=\"{id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/{t}\" Target=\"{target}\"/>"))
            .collect();
        format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{body}</Relationships>")
    };
    zip(&[
        (
            "[Content_Types].xml",
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/><Override PartName=\"/xl/worksheets/sheet1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/><Override PartName=\"/xl/worksheets/sheet2.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/><Override PartName=\"/xl/worksheets/sheet3.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/><Override PartName=\"/xl/sharedStrings.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml\"/><Override PartName=\"/xl/comments1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.comments+xml\"/><Override PartName=\"/xl/tables/table1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.table+xml\"/></Types>".to_string(),
        ),
        ("_rels/.rels", rel(&[("rId1", "officeDocument", "xl/workbook.xml")])),
        ("xl/workbook.xml", workbook_xml()),
        (
            "xl/_rels/workbook.xml.rels",
            rel(&[
                ("rId1", "worksheet", "worksheets/sheet1.xml"),
                ("rId2", "worksheet", "worksheets/sheet2.xml"),
                ("rId3", "worksheet", "worksheets/sheet3.xml"),
                ("rId4", "sharedStrings", "sharedStrings.xml"),
            ]),
        ),
        ("xl/sharedStrings.xml", sst_xml()),
        ("xl/worksheets/sheet1.xml", sheet1_xml()),
        ("xl/worksheets/sheet2.xml", sheet2_xml()),
        ("xl/worksheets/sheet3.xml", sheet3_xml()),
        (
            "xl/worksheets/_rels/sheet1.xml.rels",
            rel(&[("rId1", "comments", "../comments1.xml"), ("rId2", "table", "../tables/table1.xml")]),
        ),
        ("xl/comments1.xml", comments_xml()),
        ("xl/tables/table1.xml", table_xml()),
    ])
}

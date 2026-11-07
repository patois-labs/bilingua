"""Writes the W4 (wave 4) inputs of bilingua/tests/locos for the LocOS goldens: small hand-made Office and DTP files
for the cases the W4 B-side list names (slide comments, `=` text cells, ODF styles and comment units, Visio slot
numbers and comments, IDML and ICML run tokens, an ICML file with CRLF line ends, a Word file with images). The
layouts follow the LocOS suites' own builders (verify_office_comments, verify_idml_filter, verify_icml_filter,
verify_output_a11y). Plain Python, no LocOS needed:

    python oracle/gen_locos_w4.py bilingua/tests/locos
    <local-tms>/.venv/Scripts/python.exe -I oracle/locos_golden.py pptx xlsx docx odt ods vsdx idml icml

A `<file>.opts.json` next to an input names the parse options of the golden (`{"parse": {"comments": true},
"format_options": {"pptx.translate_comments": true}}`): `parse` goes to LocOS's `parse`, `format_options` to
Bilingua's extract.
"""
import io
import json
import os
import sys
import zipfile

ROOT = sys.argv[1]
DATE = (2026, 10, 10, 9, 30, 0)


def write(fmt, name, data, opts=None):
    d = os.path.join(ROOT, fmt)
    os.makedirs(d, exist_ok=True)
    with open(os.path.join(d, name), "wb") as fh:
        fh.write(data)
    if opts is not None:
        with open(os.path.join(d, name + ".opts.json"), "w", encoding="utf-8", newline="\n") as fh:
            json.dump(opts, fh, indent=1)
            fh.write("\n")
    print(fmt, name, len(data))


def zip_of(members, stored_first=None):
    out = io.BytesIO()
    with zipfile.ZipFile(out, "w") as zf:
        if stored_first is not None:
            zi = zipfile.ZipInfo(stored_first[0], date_time=DATE)
            zi.compress_type = zipfile.ZIP_STORED
            zf.writestr(zi, stored_first[1])
        for name, data in members:
            zi = zipfile.ZipInfo(name, date_time=DATE)
            zi.compress_type = zipfile.ZIP_DEFLATED
            zf.writestr(zi, data)
    return out.getvalue()


def comments(fmt):
    return {"parse": {"comments": True}, "format_options": {f"{fmt}.translate_comments": True}}


# ------------------------------------------------------------------------------------------------ PPTX
P_NS = "http://schemas.openxmlformats.org/presentationml/2006/main"
A_NS = "http://schemas.openxmlformats.org/drawingml/2006/main"
R_NS = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
P188 = "http://schemas.microsoft.com/office/powerpoint/2018/8/main"
PKG = "http://schemas.openxmlformats.org/package/2006/relationships"
DECL = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'


def rels(items):
    body = "".join(f'<Relationship Id="{i}" Type="{t}" Target="{g}"/>' for i, t, g in items)
    return f'{DECL}<Relationships xmlns="{PKG}">{body}</Relationships>'


def pptx():
    ct = (
        f'{DECL}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        '<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>'
        '<Override PartName="/ppt/slides/slide1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>'
        '<Override PartName="/ppt/slides/slide2.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/>'
        '<Override PartName="/ppt/comments/comment1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.comments+xml"/>'
        '<Override PartName="/ppt/comments/modernComment_100_1.xml" ContentType="application/vnd.ms-powerpoint.comments+xml"/>'
        "</Types>"
    )
    pres = f'{DECL}<p:presentation xmlns:r="{R_NS}" xmlns:p="{P_NS}"><p:sldIdLst><p:sldId id="256" r:id="rId1"/><p:sldId id="257" r:id="rId2"/></p:sldIdLst></p:presentation>'

    def slide(paras):
        sp = f'<p:sp><p:nvSpPr><p:cNvPr id="2" name="Title"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/>{paras}</p:txBody></p:sp>'
        return f'{DECL}<p:sld xmlns:a="{A_NS}" xmlns:r="{R_NS}" xmlns:p="{P_NS}"><p:cSld><p:spTree>{sp}</p:spTree></p:cSld></p:sld>'

    classic = (
        f'{DECL}<p:cmLst xmlns:p="{P_NS}">'
        '<p:cm authorId="0" dt="2026-10-01T10:00:00.000" idx="1"><p:pos x="10" y="10"/><p:text>  Shorten this title.  </p:text></p:cm>'
        '<p:cm authorId="0" dt="2026-10-01T10:05:00.000" idx="2"><p:pos x="10" y="10"/><p:text>+1</p:text></p:cm>'
        "</p:cmLst>"
    )
    modern = (
        f'{DECL}<p188:cmLst xmlns:a="{A_NS}" xmlns:r="{R_NS}" xmlns:p188="{P188}">'
        '<p188:cm id="{C0000000-0000-0000-0000-000000000001}" authorId="{A0000000-0000-0000-0000-000000000001}" created="2026-10-01T10:00:00.000">'
        '<p188:txBody><a:bodyPr/><a:p><a:r><a:t>Is this </a:t></a:r><a:r><a:rPr b="1"/><a:t>chart</a:t></a:r><a:r><a:t> current?</a:t></a:r></a:p></p188:txBody>'
        "<p188:replyLst>"
        '<p188:reply id="{C0000000-0000-0000-0000-000000000002}" authorId="{A0000000-0000-0000-0000-000000000002}" created="2026-10-02T10:00:00.000">'
        "<p188:txBody><a:bodyPr/><a:p><a:r><a:t>Yes, updated today.</a:t></a:r></a:p></p188:txBody></p188:reply>"
        "</p188:replyLst></p188:cm></p188:cmLst>"
    )
    s1rels = rels([
        ("rIdC1", f"{R_NS}/comments", "../comments/comment1.xml"),
        ("rIdC2", "http://schemas.microsoft.com/office/2018/10/relationships/comments", "../comments/modernComment_100_1.xml"),
    ])
    return zip_of([
        ("[Content_Types].xml", ct),
        ("_rels/.rels", rels([("rId1", f"{R_NS}/officeDocument", "ppt/presentation.xml")])),
        ("ppt/presentation.xml", pres),
        ("ppt/_rels/presentation.xml.rels", rels([("rId1", f"{R_NS}/slide", "slides/slide1.xml"), ("rId2", f"{R_NS}/slide", "slides/slide2.xml")])),
        ("ppt/slides/slide1.xml", slide('<a:p><a:r><a:t>Quarterly results</a:t></a:r></a:p>')),
        ("ppt/slides/_rels/slide1.xml.rels", s1rels),
        ("ppt/slides/slide2.xml", slide('<a:p><a:r><a:t>Thank you</a:t></a:r></a:p>')),
        ("ppt/comments/comment1.xml", classic),
        ("ppt/comments/modernComment_100_1.xml", modern),
    ])


# ------------------------------------------------------------------------------------------------ XLSX
X_NS = 'xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"'


def xlsx():
    sst = (
        '<sst ' + X_NS + ' count="5" uniqueCount="5"><si><t>= Mandatory fields</t></si><si><t>Name</t></si>'
        "<si><t> =spaced</t></si><si><t>=Optional fields</t></si><si><t>Comments</t></si></sst>"
    )
    sheet = (
        f"<worksheet {X_NS}><sheetData>"
        '<row r="1"><c r="A1" t="s"><v>1</v></c><c r="B1" t="s"><v>4</v></c></row>'
        '<row r="2"><c r="A2" t="s"><v>0</v></c><c r="B2" t="inlineStr"><is><t>=inline look-alike</t></is></c></row>'
        '<row r="3"><c r="A3" t="s"><v>3</v></c><c r="B3" t="s"><v>2</v></c></row>'
        '<row r="4"><c r="A4"><f>1+1</f><v>2</v></c><c r="B4" t="inlineStr"><is><t>Last cell</t></is></c></row>'
        "</sheetData></worksheet>"
    )
    ct = (
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/>'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>'
        '<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>'
        '<Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/></Types>'
    )
    rt = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
    return zip_of([
        ("[Content_Types].xml", ct),
        ("_rels/.rels", rels([("rId1", f"{rt}/officeDocument", "xl/workbook.xml")])),
        ("xl/workbook.xml", f'<workbook {X_NS}><sheets><sheet name="Form" sheetId="1" r:id="rId1"/></sheets></workbook>'),
        ("xl/_rels/workbook.xml.rels", rels([("rId1", f"{rt}/worksheet", "worksheets/sheet1.xml"), ("rIdS", f"{rt}/sharedStrings", "sharedStrings.xml")])),
        ("xl/sharedStrings.xml", sst),
        ("xl/worksheets/sheet1.xml", sheet),
    ])


# ------------------------------------------------------------------------------------------------ ODF
ODF_NS = (
    'xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" '
    'xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" '
    'xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" '
    'xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" '
    'xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" '
    'xmlns:dc="http://purl.org/dc/elements/1.1/" '
    'xmlns:meta="urn:oasis:names:tc:opendocument:xmlns:meta:1.0" office:version="1.3"'
)
ANNOTATION = (
    "<office:annotation><dc:creator>Ann Reviewer</dc:creator><dc:date>2026-10-01T10:00:00</dc:date>"
    "<text:p>Rephrase this, please.</text:p><text:p>--</text:p><text:p>Second note paragraph.</text:p></office:annotation>"
)


def odf(body, mimetype):
    content = (
        f'<?xml version="1.0" encoding="UTF-8"?>\n<office:document-content {ODF_NS}>'
        '<office:automatic-styles><style:style style:name="P1" style:family="paragraph"><style:text-properties fo:font-weight="bold"/></style:style></office:automatic-styles>'
        f"<office:body>{body}</office:body></office:document-content>"
    )
    manifest = (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" manifest:version="1.3">'
        f'<manifest:file-entry manifest:full-path="/" manifest:media-type="{mimetype}"/>'
        '<manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/></manifest:manifest>'
    )
    return zip_of([("content.xml", content), ("META-INF/manifest.xml", manifest)], stored_first=("mimetype", mimetype))


ODT_BODY = (
    '<office:text><text:h text:style-name="Heading_20_1" text:outline-level="1">Getting started</text:h>'
    '<text:p text:style-name="P1">First paragraph' + ANNOTATION + " of the text.</text:p>"
    '<text:p text:style-name="Text_20_body">Second <text:span text:style-name="T1">styled</text:span> paragraph.</text:p></office:text>'
)
ODS_BODY = (
    '<office:spreadsheet><table:table table:name="S"><table:table-row>'
    '<table:table-cell office:value-type="string"><text:p text:style-name="P1">Revenue</text:p></table:table-cell>'
    '<table:table-cell office:value-type="float" office:value="12">'
    "<office:annotation><dc:creator>Bo</dc:creator><text:p>Rounded figure.</text:p></office:annotation><text:p>12</text:p></table:table-cell>"
    "</table:table-row></table:table></office:spreadsheet>"
)

# ------------------------------------------------------------------------------------------------ VSDX
V_NS = "http://schemas.microsoft.com/office/visio/2012/main"
PAGE_REL = "http://schemas.microsoft.com/visio/2010/relationships/page"


def vsdx():
    ct = (
        "<?xml version='1.0' encoding='utf-8' standalone='yes'?><Types xmlns='http://schemas.openxmlformats.org/package/2006/content-types'>"
        "<Default Extension='rels' ContentType='application/vnd.openxmlformats-package.relationships+xml'/><Default Extension='xml' ContentType='application/xml'/>"
        "<Override PartName='/visio/document.xml' ContentType='application/vnd.ms-visio.drawing.main+xml'/>"
        "<Override PartName='/visio/pages/pages.xml' ContentType='application/vnd.ms-visio.pages+xml'/>"
        "<Override PartName='/visio/pages/page1.xml' ContentType='application/vnd.ms-visio.page+xml'/>"
        "<Override PartName='/visio/pages/page2.xml' ContentType='application/vnd.ms-visio.page+xml'/></Types>"
    )
    pages = (
        f"<?xml version='1.0' encoding='utf-8' ?>\r\n<Pages xmlns='{V_NS}' xmlns:r='{R_NS}' xml:space='preserve'>"
        "<Page ID='4' NameU='Overview' Name='Overview'><PageSheet/><Rel r:id='rId2'/></Page>"
        "<Page ID='0' NameU='Page-1' Name='Process details'><PageSheet/><Rel r:id='rId1'/></Page></Pages>"
    )
    prels = (
        "<?xml version='1.0' encoding='utf-8' ?><Relationships xmlns='http://schemas.openxmlformats.org/package/2006/relationships'>"
        f"<Relationship Id='rId1' Type='{PAGE_REL}' Target='page1.xml'/><Relationship Id='rId2' Type='{PAGE_REL}' Target='page2.xml'/></Relationships>"
    )
    page1 = (
        f"<?xml version='1.0' encoding='utf-8' ?>\r\n<PageContents xmlns='{V_NS}' xmlns:r='{R_NS}' xml:space='preserve'><Shapes>"
        "<Shape ID='1' Type='Shape'><Section N='Property'><Row N='Owner'><Cell N='Value' V='Ops' U='STR'/><Cell N='Label' V='Process owner'/></Row>"
        "<Row N='Calc'><Cell N='Label' V='Computed' F='Sheet.5!Prop.X'/></Row></Section>"
        "<Text><cp IX='0'/>Review the <cp IX='1'/>request<cp IX='0'/> today</Text></Shape>"
        "<Shape ID='3' Type='Shape'><Text><fld IX='0'>10/10/2026</fld></Text></Shape>"
        "<Shape ID='4' Type='Shape'><Text>Approve</Text></Shape></Shapes></PageContents>"
    )
    page2 = (
        f"<?xml version='1.0' encoding='utf-8' ?>\r\n<PageContents xmlns='{V_NS}' xmlns:r='{R_NS}' xml:space='preserve'><Shapes>"
        "<Shape ID='6' Type='Shape'><Text>---</Text></Shape><Shape ID='7' Type='Shape'><Text>Start</Text></Shape></Shapes></PageContents>"
    )
    drels = (
        "<Relationships xmlns='http://schemas.openxmlformats.org/package/2006/relationships'>"
        "<Relationship Id='rId9' Type='http://schemas.microsoft.com/visio/2010/relationships/comments' Target='comments.xml'/></Relationships>"
    )
    cm = (
        f"<?xml version='1.0' encoding='utf-8' ?><Comments xmlns='{V_NS}'><AuthorList><AuthorEntry Name='Ann Reviewer' Initials='AR' ID='0'/></AuthorList>"
        "<CommentList><CommentEntry AuthorID='0' PageID='0' CommentID='0'>  </CommentEntry>"
        "<CommentEntry AuthorID='0' PageID='0' ShapeID='1' CommentID='1'>Move this shape left.</CommentEntry></CommentList></Comments>"
    )
    return zip_of([
        ("[Content_Types].xml", ct),
        ("_rels/.rels", "<Relationships xmlns='http://schemas.openxmlformats.org/package/2006/relationships'/>"),
        ("visio/document.xml", f"<VisioDocument xmlns='{V_NS}'/>"),
        ("visio/_rels/document.xml.rels", drels),
        ("visio/pages/pages.xml", pages),
        ("visio/pages/_rels/pages.xml.rels", prels),
        ("visio/pages/page1.xml", page1),
        ("visio/pages/page2.xml", page2),
        ("visio/comments.xml", cm),
    ])


# ------------------------------------------------------------------------------------------------ IDML / ICML
PKG_NS = "http://ns.adobe.com/AdobeInDesign/idml/1.0/packaging"
NORMAL = "CharacterStyle/$ID/[No character style]"
BOLD = "CharacterStyle/Bold"


def psr(inner, style="ParagraphStyle/$ID/NormalParagraphStyle"):
    return f'\t\t<ParagraphStyleRange AppliedParagraphStyle="{style}">\n{inner}\n\t\t</ParagraphStyleRange>'


def csr(inner, style=NORMAL, extra=""):
    return f'\t\t\t<CharacterStyleRange AppliedCharacterStyle="{style}"{extra}>{inner}</CharacterStyleRange>'


# a paragraph with a bold range in plain text (an `r` pair), one with a Properties block before its content (its
# indentation is kept when the range is rebuilt), and a hyperlink wrapper (a `g` pair)
DTP_BODY = "\n".join([
    psr(csr("<Content>Press </Content>") + "\n" + csr("<Content>Save</Content>", BOLD) + "\n" + csr("<Content> to keep the file.</Content>")),
    psr(csr('<Properties><Leading type="unit">12</Leading></Properties>\n\t\t\t\t<Content>Leading kept.</Content>')),
    psr(csr('<Content>Read the </Content><HyperlinkTextSource Self="h1" Name="guide"><Content>guide</Content></HyperlinkTextSource><Content> first.</Content>')),
])


def idml():
    story = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
        f'<idPkg:Story xmlns:idPkg="{PKG_NS}" DOMVersion="20.0">\n'
        '\t<Story Self="a" AppliedTOCStyle="n" TrackChanges="false">\n'
        '\t\t<StoryPreference OpticalMarginAlignment="false"/>\n'
        f"{DTP_BODY}\n"
        "\t</Story>\n"
        "</idPkg:Story>\n"
    )
    designmap = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
        '<?aid style="50" type="document" readerVersion="6.0" featureSet="257" product="20.0(73)" ?>\n'
        f'<Document xmlns:idPkg="{PKG_NS}" DOMVersion="20.0" Self="d">\n'
        '\t<idPkg:Graphic src="Resources/Graphic.xml"/>\n'
        '\t<idPkg:Spread src="Spreads/Spread_s1.xml"/>\n'
        '\t<idPkg:Story src="Stories/Story_a.xml"/>\n'
        "</Document>\n"
    )
    spread = f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n<idPkg:Spread xmlns:idPkg="{PKG_NS}" DOMVersion="20.0"><Spread Self="s1"></Spread></idPkg:Spread>\n'
    return zip_of(
        [
            ("designmap.xml", designmap),
            ("META-INF/container.xml", '<?xml version="1.0"?><container/>'),
            ("Resources/Graphic.xml", '<?xml version="1.0"?><Graphic>\n\t<Color Self="c"/>\n</Graphic>'),
            ("Spreads/Spread_s1.xml", spread),
            ("Stories/Story_a.xml", story),
        ],
        stored_first=("mimetype", "application/vnd.adobe.indesign-idml-package"),
    )


ICML_HEAD = (
    '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
    '<?aid style="50" type="snippet" readerVersion="6.0" featureSet="513" product="20.0(73)" ?>\n'
    '<?aid SnippetType="InCopyInterchange"?>\n'
    '<Document DOMVersion="20.0" Self="d">\n'
    '\t<RootCharacterStyleGroup Self="rcsg"><CharacterStyle Self="CharacterStyle/Bold" Name="Bold" FontStyle="Bold"/></RootCharacterStyleGroup>\n'
)


def icml(crlf=False):
    story = (
        '\t<Story Self="u1" AppliedTOCStyle="n" TrackChanges="false" StoryTitle="$ID/" AppliedNamedGrid="n">\n'
        '\t\t<StoryPreference OpticalMarginAlignment="false" FrameType="TextFrameType"/>\n'
        f"{DTP_BODY}\n\t</Story>\n"
    )
    text = ICML_HEAD + story + "</Document>\n"
    if crlf:
        text = text.replace("\n", "\r\n")
    return text.encode("utf-8")


# ------------------------------------------------------------------------------------------------ DOCX
W_NS = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"


def docx_images():
    def img(i, attrs, extra=""):
        return f'<w:r><w:drawing><wp:inline><wp:docPr id="{i}" {attrs}>{extra}</wp:docPr></wp:inline></w:drawing></w:r>'

    deco = (
        '<a:extLst><a:ext uri="{C183D7F6-B498-43B3-948B-1728B52AA6E4}">'
        '<adec:decorative xmlns:adec="http://schemas.microsoft.com/office/drawing/2017/decorative" val="1"/></a:ext></a:extLst>'
    )
    pic1 = img(1, 'name="Picture 1"')
    logo = img(2, 'name="Logo" descr="Company logo"')
    spacer = img(3, 'name="Spacer" descr="  "')
    ornament = img(4, 'name="Ornament"', deco)
    body = (
        f"<w:p><w:r><w:t>First paragraph</w:t></w:r>{pic1}{logo}</w:p>"
        f"<w:p>{spacer}</w:p>"
        f"<w:p><w:r><w:t>Second paragraph</w:t></w:r>{ornament}</w:p>"
    )
    ns = f'xmlns:w="{W_NS}" xmlns:r="{R_NS}" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="{A_NS}"'
    doc = f"{DECL}<w:document {ns}><w:body>{body}</w:body></w:document>"
    styles = (
        f'{DECL}<w:styles xmlns:w="{W_NS}"><w:docDefaults><w:rPrDefault><w:rPr><w:lang w:val="en-US"/></w:rPr></w:rPrDefault></w:docDefaults>'
        '<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style></w:styles>'
    )
    ct = (
        f'{DECL}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>'
        '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
        '<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>'
    )
    return zip_of([
        ("[Content_Types].xml", ct),
        ("_rels/.rels", rels([("rId1", f"{R_NS}/officeDocument", "word/document.xml")])),
        ("word/document.xml", doc),
        ("word/_rels/document.xml.rels", rels([("rId1", f"{R_NS}/styles", "styles.xml")])),
        ("word/styles.xml", styles),
    ])


write("pptx", "locos_comments_1.pptx", pptx(), comments("pptx"))
write("xlsx", "locos_formula_text_1.xlsx", xlsx())
write("odt", "locos_styles_comments_1.odt", odf(ODT_BODY, "application/vnd.oasis.opendocument.text"), comments("odf"))
write("ods", "locos_comments_1.ods", odf(ODS_BODY, "application/vnd.oasis.opendocument.spreadsheet"), comments("odf"))
write("vsdx", "locos_slots_1.vsdx", vsdx(), comments("vsdx"))
write("idml", "locos_runs_1.idml", idml())
write("icml", "locos_runs_1.icml", icml())
write("icml", "locos_crlf_1.icml", icml(crlf=True))
write("docx", "locos_images_1.docx", docx_images())

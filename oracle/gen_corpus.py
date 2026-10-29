"""Builds the made-up corpus in `corpus/` (stdlib only). Every company, product and sentence is invented.

    python gen_corpus.py

The files are committed, so this only needs to run when the corpus changes. Output is deterministic (fixed zip
time stamps).
"""

import io
import json
import zipfile
from pathlib import Path

OUT = Path(__file__).resolve().parent / "corpus"

# One short product sentence per primary language.
LANGS = [
    ("en", "Welcome to Lumen Bakery. Fresh bread is ready at six every morning."),
    ("es", "Bienvenido a Panadería Lumen. El pan fresco está listo a las seis de cada mañana."),
    ("fr", "Bienvenue à la Boulangerie Lumen. Le pain frais est prêt à six heures chaque matin."),
    ("it", "Benvenuto al Forno Lumen. Il pane fresco è pronto ogni mattina alle sei."),
    ("de", "Willkommen in der Bäckerei Lumen. Frisches Brot ist jeden Morgen um sechs Uhr fertig."),
    ("nl", "Welkom bij Bakkerij Lumen. Vers brood staat elke ochtend om zes uur klaar."),
    ("pt", "Bem-vindo à Padaria Lumen. O pão fresco fica pronto às seis horas todas as manhãs."),
    ("ja", "ルーメン・ベーカリーへようこそ。焼きたてのパンは毎朝六時にご用意しています。"),
    ("zh", "欢迎光临流明面包店。新鲜面包每天早上六点出炉。"),
    ("ko", "루멘 베이커리에 오신 것을 환영합니다. 갓 구운 빵은 매일 아침 여섯 시에 준비됩니다."),
    ("ar", "مرحبا بكم في مخبز لومن. الخبز الطازج جاهز كل صباح في السادسة."),
]


def w(rel: str, data) -> None:
    p = OUT / rel
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_bytes(data.encode("utf-8") if isinstance(data, str) else data)


def esc(s: str) -> str:
    return s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def zipbytes(files) -> bytes:
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
        for name, data in files:
            zi = zipfile.ZipInfo(name, (2026, 10, 28, 9, 0, 0))
            zi.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(zi, data)
    return buf.getvalue()


DECL = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
PKG = "http://schemas.openxmlformats.org/package/2006/relationships"
OD = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"


def rels(*items) -> str:
    body = "".join(f'<Relationship Id="{i}" Type="{t}" Target="{g}"{m}/>' for i, t, g, m in items)
    return f'{DECL}<Relationships xmlns="{PKG}">{body}</Relationships>'


# ---------------------------------------------------------------- DOCX
WNS = ('xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" '
       'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" '
       'xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape" '
       'xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" '
       'xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" '
       'xmlns:dc="http://purl.org/dc/elements/1.1/"')


def run(t, rpr=""):
    return f'<w:r>{"<w:rPr>" + rpr + "</w:rPr>" if rpr else ""}<w:t xml:space="preserve">{esc(t)}</w:t></w:r>'


def para(*runs, style=None, ppr=""):
    p = f'<w:pStyle w:val="{style}"/>' if style else ""
    pp = f"<w:pPr>{p}{ppr}</w:pPr>" if (p or ppr) else ""
    return f"<w:p>{pp}{''.join(runs)}</w:p>"


def docx(body: str, *, header=None, footnotes=None, comments=None, extra_rels=(), core_title=None) -> bytes:
    ct = ('<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
          '<Default Extension="xml" ContentType="application/xml"/>'
          '<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>'
          '<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>'
          '<Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>')
    drel = [("rId1", OD + "/styles", "styles.xml", "")]
    files = []
    if header:
        ct += '<Override PartName="/word/header1.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"/>'
        drel.append(("rId2", OD + "/header", "header1.xml", ""))
        files.append(("word/header1.xml", f'{DECL}<w:hdr {WNS}>{header}</w:hdr>'))
    if footnotes:
        ct += '<Override PartName="/word/footnotes.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"/>'
        drel.append(("rId3", OD + "/footnotes", "footnotes.xml", ""))
        files.append(("word/footnotes.xml", f'{DECL}<w:footnotes {WNS}>{footnotes}</w:footnotes>'))
    if comments:
        ct += '<Override PartName="/word/comments.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml"/>'
        drel.append(("rId4", OD + "/comments", "comments.xml", ""))
        files.append(("word/comments.xml", f'{DECL}<w:comments {WNS}>{comments}</w:comments>'))
    for e in extra_rels:
        drel.append(e)
    styles = (f'{DECL}<w:styles {WNS}><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>'
              '<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:pPr><w:outlineLvl w:val="0"/></w:pPr></w:style>'
              '<w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:pPr><w:outlineLvl w:val="1"/></w:pPr></w:style>'
              '<w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/></w:style>'
              '<w:style w:type="paragraph" w:styleId="ListBullet"><w:name w:val="List Bullet"/></w:style></w:styles>')
    core = (f'{DECL}<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" '
            'xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/">'
            f'<dc:title>{esc(core_title or "Lumen Bakery handbook")}</dc:title><dc:creator>Lumen</dc:creator></cp:coreProperties>')
    sect = '<w:sectPr>' + ('<w:headerReference w:type="default" r:id="rId2"/>' if header else "") + '</w:sectPr>'
    doc = f'{DECL}<w:document {WNS}><w:body>{body}{sect}</w:body></w:document>'
    return zipbytes([
        ("[Content_Types].xml", f'{DECL}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">{ct}</Types>'),
        ("_rels/.rels", rels(("rId1", OD + "/officeDocument", "word/document.xml", ""),
                              ("rId2", "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties", "docProps/core.xml", ""))),
        ("word/_rels/document.xml.rels", rels(*drel)),
        ("word/document.xml", doc),
        ("word/styles.xml", styles),
        ("docProps/core.xml", core),
        *files,
    ])


def make_docx():
    b = ""
    b += para(run("Lumen Bakery staff handbook"), style="Title")
    b += para(run("Opening hours"), style="Heading1")
    b += para(run("We open at "), run("six", "<w:b/>"), run(" and close at "), run("eight", "<w:i/>"), run(". Come early for the best rolls."))
    b += para(run("Wash your hands before you touch the bread."), style="ListBullet")
    b += para(run("Label every tray with the date."), style="ListBullet")
    b += para(run("See the "), '<w:hyperlink r:id="rId9"><w:r><w:rPr><w:u w:val="single"/></w:rPr><w:t>safety page</w:t></w:r></w:hyperlink>',
              run(" for details."))
    b += para(run("Name"), run("→"), '<w:r><w:tab/></w:r>', run("Value"), '<w:r><w:br/></w:r>', run("Second line after a break."))
    b += para(run("Page "), '<w:fldSimple w:instr=" PAGE "><w:r><w:t>1</w:t></w:r></w:fldSimple>', run(" of the handbook."))
    b += para(run("Recipes use metric units"), '<w:r><w:rPr><w:vertAlign w:val="superscript"/></w:rPr><w:footnoteReference w:id="2"/></w:r>', run("."))
    b += ('<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid>'
          f'<w:tr><w:tc><w:tcPr/>{para(run("Bread"))}</w:tc><w:tc><w:tcPr/>{para(run("Price"))}</w:tc></w:tr>'
          f'<w:tr><w:tc><w:tcPr/>{para(run("Sourdough loaf"))}</w:tc><w:tc><w:tcPr/>{para(run("4.50"))}</w:tc></w:tr></w:tbl>')
    b += para(run("Tracked: "), '<w:ins w:id="5" w:author="Sam" w:date="2026-10-01T00:00:00Z"><w:r><w:t>added words</w:t></w:r></w:ins>',
              '<w:del w:id="6" w:author="Sam" w:date="2026-10-01T00:00:00Z"><w:r><w:delText>removed words</w:delText></w:r></w:del>')
    b += para('<w:commentRangeStart w:id="0"/>', run("Ask the manager about holiday rota."), '<w:commentRangeEnd w:id="0"/>',
              '<w:r><w:commentReference w:id="0"/></w:r>')
    b += para(run("A text box follows."),
              '<w:r><mc:AlternateContent><mc:Choice Requires="wps"><w:drawing><wps:wsp><wps:txbx><w:txbxContent>'
              + para(run("Closed on public holidays")) +
              '</w:txbxContent></wps:txbx></wps:wsp></w:drawing></mc:Choice><mc:Fallback><w:pict><w:txbxContent>'
              + para(run("Closed on public holidays")) + '</w:txbxContent></w:pict></mc:Fallback></mc:AlternateContent></w:r>')
    b += para(run("Empty paragraph follows."))
    b += para()
    b += para(run("42"))
    b += para(run("Smart "), '<w:smartTag w:uri="urn:x" w:element="place"><w:r><w:t>Lisbon</w:t></w:r></w:smartTag>', run(" branch opens soon."))
    b += para('<w:sdt><w:sdtPr/><w:sdtContent><w:r><w:t>Content control text</w:t></w:r></w:sdtContent></w:sdt>')
    w("docx/features.docx", docx(
        b,
        header=para(run("Lumen Bakery - internal")),
        footnotes=('<w:footnote w:id="0"><w:p><w:r><w:t>sep</w:t></w:r></w:p></w:footnote>'
                   '<w:footnote w:id="2">' + para(run("1 kg of flour is 1000 g.")) + '</w:footnote>'),
        comments='<w:comment w:id="0" w:author="Ana" w:date="2026-10-02T00:00:00Z">' + para(run("Check with Ana first.")) + '</w:comment>',
        extra_rels=[("rId9", OD + "/hyperlink", "https://example.com/safety", ' TargetMode="External"')],
    ))
    # eleven languages
    body = para(run("Lumen"), style="Title")
    for code, text in LANGS:
        bidi = "<w:bidi/>" if code == "ar" else ""
        body += para(run(text, "<w:rtl/>" if code == "ar" else ""), ppr=bidi)
        lang_parts = text.split(". ") if code not in ("ja", "zh", "ko", "ar") else [text]
        body += para(run(lang_parts[0]), run(" "), run(lang_parts[-1], "<w:b/>"))
    w("docx/languages.docx", docx(body, core_title="Lumen in eleven languages"))
    # split runs and odd formatting
    body = para(run("Fresh "), run("bread ", "<w:lang w:val=\"en-US\"/>"), run("daily", "<w:noProof/>"), run(" at six."))
    body += para(run("Bold start", "<w:b/>"), run(" then plain, then "), run("italic end", "<w:i/>"))
    body += para('<w:r><w:rPr><w:rFonts w:ascii="Arial"/><w:color w:val="FF0000"/></w:rPr><w:t>Red Arial text</w:t></w:r>')
    body += para(run(" leading and trailing spaces "))
    body += para(run("Tom & Jerry <3 \"quotes\" and 'apostrophes'"))
    w("docx/runs.docx", docx(body))


# ---------------------------------------------------------------- XLSX
SML = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"


def make_xlsx():
    sst = [
        "<si><t>Item</t></si>", "<si><t>Price</t></si>", "<si><t>Sourdough loaf</t></si>",
        '<si><r><rPr><b/></rPr><t xml:space="preserve">Fresh </t></r><r><t>rolls</t></r></si>',
        "<si><t>2026</t></si>", "<si><t>Hidden remark</t></si>", "<si><t>Item</t></si>",
    ]
    for _, t in LANGS:
        sst.append(f"<si><t>{esc(t)}</t></si>")
    sst.append("<si><t>Line one\nLine two in one cell</t></si>")
    sst.append("<si><t>Tom &amp; Jerry &lt;3</t></si>")
    sst_xml = f'{DECL}<sst xmlns="{SML}" count="40" uniqueCount="{len(sst)}">{"".join(sst)}</sst>'
    rows = '<row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c></row>'
    rows += '<row r="2"><c r="A2" t="s"><v>2</v></c><c r="B2"><v>4.5</v></c></row>'
    rows += '<row r="3"><c r="A3" t="s"><v>3</v></c><c r="B3"><f>B2*2</f><v>9</v></c></row>'
    rows += '<row r="4"><c r="A4" t="s"><v>4</v></c><c r="B4" t="s"><v>0</v></c></row>'
    rows += '<row r="5" hidden="1"><c r="A5" t="s"><v>5</v></c></row>'
    rows += '<row r="6"><c r="A6" t="inlineStr"><is><t>Inline cell text</t></is></c><c r="B6" t="str"><f>A6&amp;"!"</f><v>Inline cell text!</v></c></row>'
    for i in range(len(LANGS)):
        r = 7 + i
        rows += f'<row r="{r}"><c r="A{r}" t="s"><v>{7 + i}</v></c></row>'
    r = 7 + len(LANGS)
    rows += f'<row r="{r}"><c r="A{r}" t="s"><v>{7 + len(LANGS)}</v></c><c r="B{r}" t="s"><v>{8 + len(LANGS)}</v></c></row>'
    sheet = f'{DECL}<worksheet xmlns="{SML}"><sheetData>{rows}</sheetData></worksheet>'
    sheet2 = (f'{DECL}<worksheet xmlns="{SML}"><sheetData><row r="1"><c r="A1" t="s"><v>6</v></c></row></sheetData></worksheet>')
    wb = (f'{DECL}<workbook xmlns="{SML}" xmlns:r="{OD}"><sheets><sheet name="Menu" sheetId="1" r:id="rId1"/>'
          '<sheet name="Archive" sheetId="2" state="hidden" r:id="rId2"/></sheets></workbook>')
    ct = ('<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>'
          '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>'
          '<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>'
          '<Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>'
          '<Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/>')
    w("xlsx/menu.xlsx", zipbytes([
        ("[Content_Types].xml", f'{DECL}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">{ct}</Types>'),
        ("_rels/.rels", rels(("rId1", OD + "/officeDocument", "xl/workbook.xml", ""))),
        ("xl/workbook.xml", wb),
        ("xl/_rels/workbook.xml.rels", rels(("rId1", OD + "/worksheet", "worksheets/sheet1.xml", ""),
                                            ("rId2", OD + "/worksheet", "worksheets/sheet2.xml", ""),
                                            ("rId3", OD + "/sharedStrings", "sharedStrings.xml", ""))),
        ("xl/sharedStrings.xml", sst_xml),
        ("xl/worksheets/sheet1.xml", sheet),
        ("xl/worksheets/sheet2.xml", sheet2),
    ]))


# ---------------------------------------------------------------- PPTX
PNS = ('xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" '
       'xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" '
       'xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"')


def sp(sid, name, paras, ph=None, descr=""):
    nv = f'<p:nvPr><p:ph type="{ph}"/></p:nvPr>' if ph else "<p:nvPr/>"
    d = f' descr="{esc(descr)}"' if descr else ""
    return (f'<p:sp><p:nvSpPr><p:cNvPr id="{sid}" name="{name}"{d}/><p:cNvSpPr/>{nv}</p:nvSpPr><p:spPr/>'
            f'<p:txBody><a:bodyPr/><a:lstStyle/>{paras}</p:txBody></p:sp>')


def ap(*runs, lvl=None):
    ppr = f'<a:pPr lvl="{lvl}"/>' if lvl is not None else ""
    return f"<a:p>{ppr}{''.join(runs)}</a:p>"


def ar(t, rpr='lang="en-US"'):
    return f'<a:r><a:rPr {rpr}/><a:t>{esc(t)}</a:t></a:r>'


def make_pptx():
    s1 = sp(2, "Title 1", ap(ar("Lumen Bakery launch")), ph="title")
    s1 += sp(3, "Body", ap(ar("Fresh bread at "), ar("six", 'lang="en-US" b="1"'), ar(" every morning.")) +
             ap(ar("Second bullet", 'lang="en-US"'), lvl=1) + ap(ar("2026")) + ap())
    s1 += sp(4, "Box", ap(ar("Text box content")))
    s2 = sp(2, "Title 2", ap(ar("Eleven languages")), ph="title")
    for i, (_, t) in enumerate(LANGS):
        s2 += sp(3 + i, f"L{i}", ap(ar(t)))
    s3 = sp(2, "Title 3", ap(ar("Alt text slide")), ph="title")
    s3 += ('<p:pic><p:nvPicPr><p:cNvPr id="9" name="Picture 8" descr="A loaf of bread on a table"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr>'
           '<p:blipFill/><p:spPr/></p:pic>')
    s3 += ('<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="10" name="Table 9"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm/>'
           '<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/table"><a:tbl><a:tblGrid><a:gridCol w="100"/></a:tblGrid>'
           '<a:tr h="10"><a:tc><a:txBody><a:bodyPr/><a:p>' + ar("Bread") + '</a:p></a:txBody></a:tc></a:tr>'
           '<a:tr h="10"><a:tc><a:txBody><a:bodyPr/><a:p>' + ar("Rye") + '</a:p></a:txBody></a:tc></a:tr>'
           '</a:tbl></a:graphicData></a:graphic></p:graphicFrame>')

    def sld(shapes):
        return (f'{DECL}<p:sld {PNS}><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>'
                f'{shapes}</p:spTree></p:cSld></p:sld>')
    notes = (f'{DECL}<p:notes {PNS}><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>'
             + sp(2, "Notes", ap(ar("Remember to mention the new oven.")), ph="body") + '</p:spTree></p:cSld></p:notes>')
    layout = (f'{DECL}<p:sldLayout {PNS}><p:cSld name="Title and Content"><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>'
              + sp(2, "Title", ap(ar("Click to edit Master title style")), ph="title")
              + sp(3, "Brand", ap(ar("Lumen Bakery - confidential"))) + '</p:spTree></p:cSld></p:sldLayout>')
    master = (f'{DECL}<p:sldMaster {PNS}><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>'
              + sp(2, "Title", ap(ar("Click to edit Master title style")), ph="title") + '</p:spTree></p:cSld></p:sldMaster>')
    P = "application/vnd.openxmlformats-officedocument.presentationml."
    ct = ('<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>'
          f'<Override PartName="/ppt/presentation.xml" ContentType="{P}presentation.main+xml"/>'
          + "".join(f'<Override PartName="/ppt/slides/slide{i}.xml" ContentType="{P}slide+xml"/>' for i in (1, 2, 3))
          + f'<Override PartName="/ppt/notesSlides/notesSlide1.xml" ContentType="{P}notesSlide+xml"/>'
          f'<Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="{P}slideLayout+xml"/>'
          f'<Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="{P}slideMaster+xml"/>')
    pres = (f'{DECL}<p:presentation {PNS}><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst>'
            '<p:sldIdLst><p:sldId id="256" r:id="rId2"/><p:sldId id="257" r:id="rId3"/><p:sldId id="258" r:id="rId4"/></p:sldIdLst></p:presentation>')
    PR = OD
    w("pptx/launch.pptx", zipbytes([
        ("[Content_Types].xml", f'{DECL}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">{ct}</Types>'),
        ("_rels/.rels", rels(("rId1", PR + "/officeDocument", "ppt/presentation.xml", ""))),
        ("ppt/presentation.xml", pres),
        ("ppt/_rels/presentation.xml.rels", rels(("rId1", PR + "/slideMaster", "slideMasters/slideMaster1.xml", ""),
                                                 ("rId2", PR + "/slide", "slides/slide1.xml", ""),
                                                 ("rId3", PR + "/slide", "slides/slide2.xml", ""),
                                                 ("rId4", PR + "/slide", "slides/slide3.xml", ""))),
        ("ppt/slides/slide1.xml", sld(s1)),
        ("ppt/slides/_rels/slide1.xml.rels", rels(("rId1", PR + "/slideLayout", "../slideLayouts/slideLayout1.xml", ""),
                                                  ("rId2", PR + "/notesSlide", "../notesSlides/notesSlide1.xml", ""))),
        ("ppt/slides/slide2.xml", sld(s2)),
        ("ppt/slides/_rels/slide2.xml.rels", rels(("rId1", PR + "/slideLayout", "../slideLayouts/slideLayout1.xml", ""))),
        ("ppt/slides/slide3.xml", sld(s3)),
        ("ppt/slides/_rels/slide3.xml.rels", rels(("rId1", PR + "/slideLayout", "../slideLayouts/slideLayout1.xml", ""))),
        ("ppt/notesSlides/notesSlide1.xml", notes),
        ("ppt/slideLayouts/slideLayout1.xml", layout),
        ("ppt/slideLayouts/_rels/slideLayout1.xml.rels", rels(("rId1", PR + "/slideMaster", "../slideMasters/slideMaster1.xml", ""))),
        ("ppt/slideMasters/slideMaster1.xml", master),
        ("ppt/slideMasters/_rels/slideMaster1.xml.rels", rels()),
    ]))


# ---------------------------------------------------------------- text formats
def make_html():
    langs = "\n".join(f'    <p lang="{c}">{esc(t)}</p>' for c, t in LANGS)
    w("html/page.html", f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Lumen Bakery - fresh bread</title>
<meta name="description" content="Fresh bread baked every morning.">
<style>.x {{ color: red; }}</style>
<script>var greeting = "do not translate me";</script>
</head>
<body>
  <!-- a comment that stays -->
  <header><h1>Lumen Bakery</h1><nav><a href="/menu" title="See the menu">Menu</a> | <a href="/about">About us</a></nav></header>
  <main>
    <h2>Opening hours</h2>
    <p>We open at <strong>six</strong> and close at <em>eight</em>.<br>Come early for the best rolls &amp; cakes.</p>
    <p>Price: 4&nbsp;EUR &mdash; per loaf. Use <code>bake --hot</code> for the oven.</p>
    <ul><li>Sourdough</li><li>Rye <span class="tag">new</span></li><li>Seeded &lt;loaf&gt;</li></ul>
    <table><tr><th>Bread</th><th>Price</th></tr><tr><td>Rye</td><td>3.80</td></tr></table>
    <img src="loaf.png" alt="A fresh loaf of bread" title="Loaf">
    <form><label for="q">Search</label><input id="q" type="text" placeholder="Search the menu" value="keep"><button type="submit">Go</button></form>
    <p translate="no">Lumen&reg;</p>
    <pre>  keep
    this   spacing
</pre>
    <p>Mixed <b>bold <i>and italic</i></b> text, then <a href="https://example.com/x?a=1&amp;b=2">a link</a>.</p>
    <div>Loose text in a div without a paragraph.</div>
    <select><option value="1">Small</option><option value="2">Large</option></select>
{langs}
  </main>
  <footer>&copy; 2026 Lumen Bakery</footer>
</body>
</html>
""")
    w("html/edge.html", """<!DOCTYPE HTML PUBLIC "-//W3C//DTD HTML 4.01//EN">
<HTML lang=en>
<HEAD><TITLE>Edge cases &amp; more</TITLE>
<meta property="og:title" content="Open graph title">
<meta name="keywords" content="bread, bakery">
<base href="/">
<!--[if IE]><p>Old browser text</p><![endif]-->
</HEAD>
<BODY>
<P CLASS=intro>Uppercase tags and <A HREF=/x.html TITLE=unquoted>unquoted attributes</A>.
<P>Second paragraph closed by the next one.
<noscript>Please enable scripts.</noscript>
<textarea name=t>Default text in a textarea</textarea>
<button type=button aria-label="Close the dialog">X</button>
<input type=submit value="Send order">
<svg width="10" height="10"><title>Loaf icon</title><text x="0" y="5">SVG text</text></svg>
<iframe src="x.html" title="Embedded menu"></iframe>
<p>Ruby: <ruby>漢<rt>kan</rt></ruby> and a <abbr title="Kilogram">kg</abbr> value.</p>
<p>XHTML break<br/>second line<br />third line.</p>
<p dir="rtl" lang="ar">مرحبا بكم في مخبز لومن.</p>
<p>日本語の段落です。パンは毎朝焼きます。</p>
<p>한국어 문단입니다. 빵은 매일 아침 구워집니다.</p>
<p class="notranslate">Marked notranslate by class</p>
<p>Text with <span translate="no">DoNotTranslate</span> inside.</p>
<table><thead><tr><th scope="col">Name</th></tr></thead><tbody><tr><td>Rye</td></tr></tbody><tfoot><tr><td>Total</td></tr></tfoot></table>
<dl><dt>Term</dt><dd>Definition text</dd></dl>
<blockquote>Quoted text<cite>Someone</cite></blockquote>
<p>A very long line of text that goes on and on, with several sentences. Here is the second one. And a third, with a question? Yes! Done.</p>
<p>&lt;script&gt; as text, 5 &gt; 3, &quot;quotes&quot;, caf&eacute;, &#169; sign.</p>
<script type="text/template"><p>Template text inside script</p></script>
<style>p::before { content: "css text"; }</style>
<a href="x"><img src="a.png" alt="Linked image alt"></a>
<label>Name <input name=n placeholder="Your name"></label>
<details><summary>More info</summary>Hidden details text.</details>
<span>Bare span text</span> trailing text after span.
</BODY>
</HTML>
""")
    w("html/structure.html", """<html><body>
<ul>
  <li>Parent item text
    <ul><li>Child one</li><li>Child two with <b>bold</b></li></ul>
    trailing parent text
  </li>
  <li><p>Paragraph inside a list item</p><p>Second paragraph in the item</p></li>
</ul>
<div>First line<br>Second line<br>Third line</div>
<div>Intro text <div>nested block text</div> outro text</div>
<td>Orphan cell text</td>
<p>Button inline: <button>Click me</button> and a <label>label text</label> and <select><option>Opt one</option></select> end.</p>
<a href="/card"><h3>Heading inside a link</h3><p>Paragraph inside a link</p></a>
<p>
   Multi-line
   paragraph   with
   irregular    spacing.
</p>
<h1>Heading with <span class="x">span</span> and <img src="i.png" alt="inline image alt"> image</h1>
<figure><img src="f.png" alt="Figure alt"><figcaption>Figure caption text</figcaption></figure>
<p>Sentence one. Sentence two! Sentence three? "Quoted sentence." Last one.</p>
<p>Emoji and symbols: ☕ © ™ and a non-breaking space.</p>
<address>Street 1, Town</address>
<center>Legacy centered text</center>
<font color="red">Legacy font text</font>
<p>Unclosed <b>bold and <i>italic</b> mismatch</i> tags.</p>
</body></html>
""")
    # Windows-1252 file with an accent and a euro sign, and a UTF-8 file with a BOM and CRLF line ends.
    w("html/cp1252.html", ("<html><head><meta http-equiv=\"Content-Type\" content=\"text/html; charset=windows-1252\"><title>Café menú</title></head>"
                           "<body><p>Café con leche cuesta 2,50 € hoy.</p></body></html>\r\n").encode("cp1252"))
    w("html/bom_crlf.html", b"\xef\xbb\xbf" + "<html>\r\n<body>\r\n<p>First line\r\nsecond line of the same paragraph.</p>\r\n<p>Another one.</p>\r\n</body>\r\n</html>\r\n".encode("utf-8"))
    w("html/fragment.html","<h2>Quick note</h2>\n<p>Unclosed paragraph with <b>bold\n<p>Second paragraph &#8212; numeric entity &#x263A; here.\n<ul><li>One<li>Two</ul>\n")


def make_md():
    langs = "\n\n".join(t for _, t in LANGS)
    w("md/handbook.md", f"""---
title: Lumen Bakery handbook
description: How we bake
---

# Lumen Bakery

Welcome to the **bakery**. We open at *six* and close at _eight_.

## Opening hours

- Monday to Friday: six to eight
- Weekends: seven to *one*
  - nested item with `code` inside

1. First step: mix the dough.
2. Second step: bake at 220 degrees.

> Fresh bread is the heart of the town.

| Bread | Price |
|-------|-------|
| Rye | 3.80 |
| Sourdough | 4.50 |

Visit [our menu](https://example.com/menu "Menu page") or see ![a loaf](loaf.png "Loaf").
Auto link: <https://example.com>. Line one with two trailing spaces
line two after a hard break.

```bash
echo "do not translate"
```

Term[^1] with a footnote.

[^1]: Footnote text for the term.

<div class="note">Raw HTML block</div>

{langs}
""")


def make_md_edge():
    w("md/edge.md", """Setext heading
==============

Second level
------------

A line ending with a backslash\\
next line. Escaped \\* star and \\_ underscore, and a literal &copy; entity.

* [ ] open task
* [x] done task
* Item with trailing text <span class="x">inline html</span> here

> Quote with a list:
>
> - inner one
> - inner two

| Left | Center | Right |
|:-----|:------:|------:|
| a | b | c |
| long text cell | **bold cell** | `code cell` |

Reference link [to the guide][guide] and shortcut [guide].

[guide]: https://example.com/guide "Guide title"

Intraword*emphasis*here and snake_case_name and 2 * 3 = 6.

~~~python
print("fence with tildes")
~~~

Term
: Definition style line

<details>
<summary>HTML summary</summary>

Markdown inside details.

</details>

1) Parenthesis list item
2) Another one

***Bold italic*** and __bold__ and _italic_.

日本語の段落です。**太字**も含みます。

العربية: مرحبا بكم في *المخبز*.

Final paragraph without newline""")


def make_json():
    d = {
        "app": {"title": "Lumen Bakery", "tagline": "Fresh bread every morning"},
        "menu": ["Sourdough", "Rye", {"name": "Seeded loaf", "price": 4.5}],
        "greeting": "Hello, {name}! You have {count, plural, one {# message} other {# messages}}.",
        "html": "Click <b>here</b> to <a href=\"/x\">order</a>.",
        "count": 12, "enabled": True, "nothing": None, "code": "SKU-001",
        "url": "https://example.com/menu", "empty": "",
        "unicode": {c: t for c, t in LANGS},
    }
    w("json/strings.json", json.dumps(d, ensure_ascii=False, indent=2) + "\n")
    w("json/array_root.json", json.dumps(["First", "Second", {"k": "Nested value"}, 7, "Third"], ensure_ascii=False, indent=4) + "\n")
    w("json/escapes.json", '{\n  "quote": "He said \\"hi\\"",\n  "newline": "Line one\\nLine two",\n  "tab": "A\\tB",\n  "uni": "caf\\u00e9 \\u65e5\\u672c",\n'
      '  "slash": "a\\/b",\n  "emoji": "Bread \\ud83c\\udf5e",\n  "key with space": "Spaced key value",\n  "": "Empty key value"\n}\n')
    w("json/bom_crlf.json", b"\xef\xbb\xbf" + b'{\r\n  "a": "Windows line ends",\r\n  "b": "Second"\r\n}\r\n')
    w("json/compact.json",json.dumps({"a": "One", "b": ["Two", "Three"], "c": {"d": "Four"}}, ensure_ascii=False))


def make_po():
    parts = ['''msgid ""
msgstr ""
"Project-Id-Version: lumen 1.0\\n"
"Language: es\\n"
"MIME-Version: 1.0\\n"
"Content-Type: text/plain; charset=UTF-8\\n"
"Plural-Forms: nplurals=2; plural=(n != 1);\\n"

#. Translator comment for the greeting
#: src/main.c:12
msgid "Welcome to Lumen Bakery"
msgstr ""

#: src/main.c:20
msgctxt "menu"
msgid "Open"
msgstr "Abrir"

#, fuzzy
msgid "Old fuzzy text"
msgstr "Texto antiguo"

msgid "One loaf"
msgid_plural "%d loaves"
msgstr[0] ""
msgstr[1] ""

msgid ""
"A long message that "
"continues on a second line.\\n"
"And a third line."
msgstr ""

msgid "Press <b>%s</b> to continue, %(name)s."
msgstr ""

#~ msgid "Obsolete"
#~ msgstr "Obsoleto"
''']
    for i, (c, t) in enumerate(LANGS):
        parts.append(f'\nmsgid "lang-{c}"\nmsgstr "{t.replace(chr(34), chr(92) + chr(34))}"\n')
    w("po/lumen.po", "".join(parts))


def make_subs():
    srt = []
    cues = [("Welcome to Lumen Bakery.", "We open at six."), ("<i>Fresh bread</i> every morning.",), ("Two lines here,", "kept apart."),
            ("<b>Bold</b> and <font color=\"#ff0000\">colour</font>.",)]
    for c, t in LANGS:
        cues.append((t,))
    t0 = 0
    for i, c in enumerate(cues, 1):
        a, b = t0, t0 + 3000
        t0 += 3500
        fm = lambda ms: f"{ms // 3600000:02d}:{ms // 60000 % 60:02d}:{ms // 1000 % 60:02d},{ms % 1000:03d}"
        srt.append(f"{i}\n{fm(a)} --> {fm(b)}\n" + "\n".join(c) + "\n")
    w("srt/lumen.srt", "\n".join(srt))
    vtt = ["WEBVTT - Lumen captions", "", "NOTE a comment block", ""]
    for i, c in enumerate(cues, 1):
        a, b = (i - 1) * 3500, (i - 1) * 3500 + 3000
        fm = lambda ms: f"{ms // 3600000:02d}:{ms // 60000 % 60:02d}:{ms // 1000 % 60:02d}.{ms % 1000:03d}"
        vtt.append(f"cue{i}")
        vtt.append(f"{fm(a)} --> {fm(b)} line:90% align:center")
        body = [x.replace("<font color=\"#ff0000\">", "<c.red>").replace("</font>", "</c>") for x in c]
        vtt += body
        vtt.append("")
    vtt.append("00:01:00.000 --> 00:01:02.000\n<v Baker>Voice tag speaker line.\n")
    w("vtt/lumen.vtt", "\n".join(vtt))


def make_txt():
    w("txt/notes.txt", "Lumen Bakery notes\n\nWe open at six. We close at eight.\nThis line wraps onto\nthe next line in the same paragraph.\n\n"
      + "\n\n".join(t for _, t in LANGS) + "\n\n   indented paragraph   \n\nLast line without newline")
    w("txt/crlf_bom.txt", b"\xef\xbb\xbf" + "Line one\r\nLine two\r\n\r\nParagraph two — with a dash.\r\n".encode("utf-8"))
    w("txt/lines.txt","One line\nAnother line\n\nAfter a blank line\n")


if __name__ == "__main__":
    make_docx(); make_xlsx(); make_pptx(); make_html(); make_md(); make_md_edge(); make_json(); make_po(); make_subs(); make_txt()
    print("corpus written to", OUT)

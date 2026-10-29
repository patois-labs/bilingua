"""Builds extra DOCX and XLSX files with python-docx (MIT) and openpyxl (MIT). Run with a Python that has both:

    <local-tms>\\.venv\\Scripts\\python.exe gen_office.py

Unlike gen_corpus.py (hand-written XML), these come from real Office-style templates, so they carry the odd parts
a hand-written file does not: theme, settings, fonts table, document defaults, many styles. All text is invented.
Output goes to `corpus/docx/` and `corpus/xlsx/`. See corpus/SOURCES.md.
"""

import sys
from pathlib import Path

sys.dont_write_bytecode = True
OUT = Path(__file__).resolve().parent / "corpus"

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


def fix_zip_times(path: Path) -> None:
    """Rewrites a zip with fixed time stamps so a rebuild gives the same bytes."""
    import zipfile
    import io

    src = zipfile.ZipFile(path)
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as z:
        for info in src.infolist():
            zi = zipfile.ZipInfo(info.filename, (2026, 10, 28, 9, 0, 0))
            zi.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(zi, src.read(info.filename))
    src.close()
    path.write_bytes(buf.getvalue())


def make_docx() -> None:
    import datetime
    from docx import Document
    from docx.enum.text import WD_BREAK
    from docx.oxml import OxmlElement
    from docx.oxml.ns import qn

    d = Document()
    d.core_properties.title = "Lumen Bakery quality manual"
    d.core_properties.subject = "Baking standards"
    d.core_properties.author = "Lumen"
    d.core_properties.created = datetime.datetime(2026, 10, 28)
    d.core_properties.modified = datetime.datetime(2026, 10, 28)
    d.core_properties.last_printed = datetime.datetime(2026, 10, 28)
    d.add_heading("Lumen Bakery quality manual", 0)
    d.add_heading("1. Ingredients", 1)
    p = d.add_paragraph("Use ")
    p.add_run("only fresh").bold = True
    p.add_run(" flour, ")
    r = p.add_run("never")
    r.italic = True
    r = p.add_run(" frozen dough")
    r.underline = True
    p.add_run(". Water must be ")
    r = p.add_run("filtered")
    r.font.superscript = True
    p.add_run(".")
    d.add_paragraph("Sourdough starter", style="List Bullet")
    d.add_paragraph("Rye flour from the local mill", style="List Bullet")
    d.add_paragraph("Mix the dough for ten minutes.", style="List Number")
    d.add_paragraph("Let it rest for an hour.", style="List Number")
    d.add_heading("2. Safety", 2)
    p = d.add_paragraph("Read the ")
    link = OxmlElement("w:hyperlink")
    link.set(qn("r:id"), d.part.relate_to("https://example.com/safety", "http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink", is_external=True))
    run = OxmlElement("w:r")
    t = OxmlElement("w:t")
    t.text = "safety sheet"
    run.append(t)
    link.append(run)
    p._p.append(link)
    p.add_run(" before you start.")
    p = d.add_paragraph("Line one")
    p.add_run().add_break(WD_BREAK.LINE)
    p.add_run("Line two after a break")
    p.add_run("\tand a tab.")
    d.add_page_break()
    d.add_paragraph("Text after a page break.")
    table = d.add_table(rows=3, cols=3)
    table.style = "Table Grid"
    for i, row in enumerate(table.rows):
        for j, cell in enumerate(row.cells):
            cell.text = f"Row {i + 1} column {j + 1}" if (i + j) % 2 == 0 else "Rye bread"
    table.cell(0, 0).merge(table.cell(0, 1))
    d.add_paragraph("")
    d.add_paragraph("Quoted text from the founder.", style="Quote")
    for code, text in LANGS:
        p = d.add_paragraph()
        r = p.add_run(text)
        if code == "ar":
            ppr = p._p.get_or_add_pPr()
            bidi = OxmlElement("w:bidi")
            ppr.append(bidi)
            rpr = r._r.get_or_add_rPr()
            rpr.append(OxmlElement("w:rtl"))
        if code in ("ja", "zh", "ko"):
            rpr = r._r.get_or_add_rPr()
            rf = OxmlElement("w:lang")
            rf.set(qn("w:eastAsia"), {"ja": "ja-JP", "zh": "zh-CN", "ko": "ko-KR"}[code])
            rpr.append(rf)
    sec = d.sections[0]
    sec.header.paragraphs[0].text = "Lumen Bakery - quality manual"
    sec.footer.paragraphs[0].text = "Confidential - do not copy"
    try:
        d.add_comment(runs=d.paragraphs[2].runs[0], text="Please check the flour grade.", author="Ana", initials="A")
    except Exception as e:  # older python-docx
        print("no comments:", e)
    p = d.add_paragraph("Total: ")
    fld = OxmlElement("w:fldSimple")
    fld.set(qn("w:instr"), " NUMPAGES ")
    r = OxmlElement("w:r")
    t = OxmlElement("w:t")
    t.text = "3"
    r.append(t)
    fld.append(r)
    p._p.append(fld)
    p.add_run(" pages.")
    path = OUT / "docx" / "manual.docx"
    path.parent.mkdir(parents=True, exist_ok=True)
    d.save(path)
    fix_zip_times(path)


def make_xlsx() -> None:
    from openpyxl import Workbook
    from openpyxl.cell.rich_text import CellRichText, TextBlock
    from openpyxl.cell.text import InlineFont
    from openpyxl.comments import Comment
    from openpyxl.worksheet.datavalidation import DataValidation
    from openpyxl.worksheet.table import Table

    wb = Workbook()
    ws = wb.active
    ws.title = "Products"
    ws.append(["Product", "Price", "Notes"])
    ws.append(["Sourdough loaf", 4.5, "Baked daily"])
    ws.append(["Rye bread", 3.8, "Dense and dark"])
    ws.append(["Seeded roll", 1.2, CellRichText("Try the ", TextBlock(InlineFont(b=True), "new"), " recipe")])
    ws.append(["Total", "=SUM(B2:B4)", "Formula above"])
    ws["C6"] = "Line one\nLine two in the same cell"
    ws["A7"] = "2026"
    ws["B7"] = "12 EUR"
    ws["C7"] = "=\"Computed: \"&A5"
    ws.merge_cells("A9:C9")
    ws["A9"] = "Merged heading across columns"
    ws["A2"].comment = Comment("Check the price with Ana.", "Ana")
    dv = DataValidation(type="list", formula1='"Yes,No,Maybe"', allow_blank=True)
    ws.add_data_validation(dv)
    dv.add("C2")
    ws.row_dimensions[3].hidden = True
    ws.column_dimensions["D"].hidden = True
    ws["D1"] = "Hidden column text"
    tab = Table(displayName="Products1", ref="A1:C4")
    ws.add_table(tab)
    ws.oddHeader.center.text = "Lumen products"
    ws.oddFooter.right.text = "Page &P"
    ws2 = wb.create_sheet("Languages")
    for i, (code, text) in enumerate(LANGS, 1):
        ws2.cell(row=i, column=1, value=code)
        ws2.cell(row=i, column=2, value=text)
    ws3 = wb.create_sheet("Archive")
    ws3.sheet_state = "hidden"
    ws3["A1"] = "Old price list"
    path = OUT / "xlsx" / "products.xlsx"
    path.parent.mkdir(parents=True, exist_ok=True)
    wb.properties.creator = "Lumen"
    wb.save(path)
    fix_zip_times(path)


if __name__ == "__main__":
    make_docx()
    make_xlsx()
    print("office files written to", OUT)

"""Writes the hand-made XLSX inputs of the LocOS-profile goldens (Track D group B), next to the captured ones:

    python oracle/locos_xlsx_fixtures.py
    <local-tms>/.venv/Scripts/python.exe oracle/locos_golden.py xlsx

The captured inputs come from LocOS's own suites; these add what they do not cover: pretty-printed parts (the text
lxml keeps after an element moves with it), hidden and very hidden sheets, sheet and workbook protection, hidden
rows and columns, every kind of formula, error and boolean values, a shared string used by two cells, rich strings
with one and with mixed run formats, phonetic runs, `x005F_` in a shared string, implicit and duplicate cell
addresses, text boxes (with an `mc:Fallback` copy) and no `docProps/core.xml` at all (the stamp makes one).
`locos_textbox_runs_1.xlsx` has text-box paragraphs with several runs, a break and a field.
"""

import os
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(os.path.dirname(HERE), "bilingua", "tests", "locos", "xlsx")

M = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
DECL = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'


def rels(items):
    body = "".join(f'\n  <Relationship Id="{i}" Type="{REL}/{t}" Target="{target}"/>' for i, t, target in items)
    return f'{DECL}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{body}\n</Relationships>'


def types(overrides):
    body = "".join(f'\n  <Override PartName="{p}" ContentType="application/vnd.openxmlformats-officedocument.{t}"/>' for p, t in overrides)
    return (
        f'{DECL}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">\n'
        '  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>\n'
        f'  <Default Extension="xml" ContentType="application/xml"/>{body}\n</Types>'
    )


def write(name, files):
    with zipfile.ZipFile(os.path.join(OUT, name), "w", zipfile.ZIP_DEFLATED) as zf:
        for n, d in files:
            zf.writestr(zipfile.ZipInfo(n, (2026, 11, 5, 12, 0, 0)), d.encode("utf-8"), zipfile.ZIP_DEFLATED)


def drawing(paragraphs, fallback=True):
    paras = "".join(f"\n            {p}" for p in paragraphs)
    choice = f"""
      <xdr:twoCellAnchor>
        <xdr:from><xdr:col>4</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from>
        <xdr:to><xdr:col>8</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>5</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>
        <xdr:sp macro="" textlink="">
          <xdr:nvSpPr><xdr:cNvPr id="2" name="TextBox 1"/><xdr:cNvSpPr txBox="1"/></xdr:nvSpPr>
          <xdr:spPr/>
          <xdr:txBody>
            <a:bodyPr/>
            <a:lstStyle/>{paras}
          </xdr:txBody>
        </xdr:sp>
        <xdr:clientData/>
      </xdr:twoCellAnchor>"""
    if fallback:
        body = f"""
  <mc:AlternateContent>
    <mc:Choice Requires="a14">{choice}
    </mc:Choice>
    <mc:Fallback>
      <xdr:twoCellAnchor>
        <xdr:from><xdr:col>4</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from>
        <xdr:to><xdr:col>8</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>5</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>
        <xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="2" name="TextBox 1"/><xdr:cNvSpPr txBox="1"/></xdr:nvSpPr><xdr:spPr/>
          <xdr:txBody><a:bodyPr/><a:p><a:r><a:t>Fallback copy</a:t></a:r></a:p></xdr:txBody></xdr:sp>
        <xdr:clientData/>
      </xdr:twoCellAnchor>
    </mc:Fallback>
  </mc:AlternateContent>"""
    else:
        body = choice
    return (
        f'{DECL}<xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" '
        'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" '
        'xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" '
        f'xmlns:a14="http://schemas.microsoft.com/office/drawing/2010/main">{body}\n</xdr:wsDr>'
    )


def walk_book():
    sst = f"""{DECL}<sst xmlns="{M}" count="10" uniqueCount="8">
  <si>
    <t>Shared twice</t>
  </si>
  <si>
    <r>
      <rPr>
        <b/>
      </rPr>
      <t xml:space="preserve">Bold </t>
    </r>
    <r>
      <rPr>
        <b/>
      </rPr>
      <t>run</t>
    </r>
  </si>
  <si><r><t xml:space="preserve">Plain </t></r><r><rPr><i/></rPr><t>italic</t></r></si>
  <si><t>Kanji text</t><rPh sb="0" eb="5"><t>kana</t></rPh><phoneticPr fontId="1"/></si>
  <si><t>under_x005F_score</t></si>
  <si><t>---</t></si>
  <si><t xml:space="preserve">  spaced  out  </t></si>
  <si><t>Hidden sheet text</t></si>
</sst>"""
    sheet1 = f"""{DECL}<worksheet xmlns="{M}" xmlns:r="{R}">
  <sheetPr><pageSetUpPr fitToPage="1"/></sheetPr>
  <dimension ref="A1:F9"/>
  <sheetViews>
    <sheetView tabSelected="1" workbookViewId="0">
      <selection activeCell="A1" sqref="A1"/>
    </sheetView>
  </sheetViews>
  <cols><col min="6" max="6" width="9" hidden="1" customWidth="1"/></cols>
  <sheetData>
    <row r="1">
      <c r="B1" t="s"><v>1</v></c>
      <c r="A1" t="s"><v>0</v></c>
      <c r="C1"><f t="shared" ref="C1:C2" si="0">A1&amp;"x"</f><v>Shared twicex</v></c>
      <c r="D1"><f t="array" ref="D1">SUM(E1:E2)</f><v>3</v></c>
      <c r="E1"><v>1</v></c>
      <c r="F1" t="s"><v>2</v></c>
    </row>
    <row r="2">
      <c r="A2" t="s"><v>3</v></c>
      <c r="C2" t="str"><f t="shared" si="0"/><v>Kanji textx</v></c>
      <c r="E2"><v>2</v></c>
      <c r="F2" t="e"><v>#N/A</v></c>
    </row>
    <row r="3" hidden="1">
      <c r="A3" t="s"><v>4</v></c>
      <c r="B3" t="b"><v>1</v></c>
      <c r="C3" t="str"><v>Plain str value</v></c>
    </row>
    <row>
      <c t="inlineStr"><is><r><rPr><u/></rPr><t>Inline </t></r><r><rPr><u/></rPr><t>rich</t></r></is></c>
      <c t="s"><v>5</v></c>
      <c t="s"><v>6</v></c>
    </row>
    <row r="5">
      <c r="A5" t="inlineStr"><is><t>First of a duplicate</t></is></c>
      <c r="A5" t="inlineStr"><is><t>Second of a duplicate</t></is></c>
      <c r="B5" t="s"><v>0</v></c>
      <c r="C5" t="str"><f>A5&amp;"!"</f><v>Second of a duplicate!</v></c>
    </row>
    <row r="9" spans="1:2"><c r="A9" t="inlineStr"><is><t>Last row</t></is></c><c r="B9"><v>4.5</v></c></row>
  </sheetData>
  <sheetProtection sheet="1" objects="1" scenarios="1"/>
  <pageMargins left="0.7" right="0.7" top="0.75" bottom="0.75" header="0.3" footer="0.3"/>
  <drawing r:id="rId1"/>
</worksheet>"""
    sheet2 = f"""{DECL}<worksheet xmlns="{M}">
  <sheetData>
    <row r="1"><c r="A1" t="s"><v>7</v></c></row>
  </sheetData>
</worksheet>"""
    sheet3 = f"""{DECL}<worksheet xmlns="{M}"><sheetPr/><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Very hidden text</t></is></c></row></sheetData></worksheet>"""
    workbook = f"""{DECL}<workbook xmlns="{M}" xmlns:r="{R}">
  <workbookProtection lockStructure="1"/>
  <bookViews><workbookView activeTab="0"/></bookViews>
  <sheets>
    <sheet name="Visible" sheetId="1" r:id="rId1"/>
    <sheet name="Hidden" sheetId="2" state="hidden" r:id="rId2"/>
    <sheet name="VeryHidden" sheetId="3" state="veryHidden" r:id="rId3"/>
  </sheets>
</workbook>"""
    paras = [
        '<a:p><a:r><a:rPr lang="en-US" sz="1100"/><a:t>Box line one</a:t></a:r></a:p>',
        '<a:p>\n              <a:pPr algn="ctr"/>\n              <a:r>\n                <a:t>Box line two</a:t>\n              </a:r>\n              <a:endParaRPr lang="en-US"/>\n            </a:p>',
        "<a:p><a:r><a:t>   </a:t></a:r></a:p>",
    ]
    return [
        ("[Content_Types].xml", types([
            ("/xl/workbook.xml", "spreadsheetml.sheet.main+xml"),
            ("/xl/worksheets/sheet1.xml", "spreadsheetml.worksheet+xml"),
            ("/xl/worksheets/sheet2.xml", "spreadsheetml.worksheet+xml"),
            ("/xl/worksheets/sheet3.xml", "spreadsheetml.worksheet+xml"),
            ("/xl/sharedStrings.xml", "spreadsheetml.sharedStrings+xml"),
            ("/xl/drawings/drawing1.xml", "drawing+xml"),
        ])),
        ("_rels/.rels", rels([("rId1", "officeDocument", "xl/workbook.xml")])),
        ("xl/workbook.xml", workbook),
        ("xl/_rels/workbook.xml.rels", rels([
            ("rId1", "worksheet", "worksheets/sheet1.xml"),
            ("rId2", "worksheet", "worksheets/sheet2.xml"),
            ("rId3", "worksheet", "worksheets/sheet3.xml"),
            ("rId4", "sharedStrings", "sharedStrings.xml"),
        ])),
        ("xl/sharedStrings.xml", sst),
        ("xl/worksheets/sheet1.xml", sheet1),
        ("xl/worksheets/_rels/sheet1.xml.rels", rels([("rId1", "drawing", "../drawings/drawing1.xml")])),
        ("xl/drawings/drawing1.xml", drawing(paras)),
        ("xl/worksheets/sheet2.xml", sheet2),
        ("xl/worksheets/sheet3.xml", sheet3),
    ]


def runs_book():
    sheet1 = f"""{DECL}<worksheet xmlns="{M}" xmlns:r="{R}"><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>Cell</t></is></c></row></sheetData><drawing r:id="rId1"/></worksheet>"""
    paras = [
        '<a:p><a:r><a:rPr sz="1100"/><a:t xml:space="preserve">Hello </a:t></a:r><a:r><a:rPr b="1" sz="1100"/><a:t>world</a:t></a:r><a:br><a:rPr sz="1100"/></a:br><a:r><a:t>again</a:t></a:r><a:endParaRPr lang="en-US"/></a:p>',
        '<a:p><a:r><a:t xml:space="preserve">Page </a:t></a:r><a:fld id="{B6F15528-21DE-4FAA-801E-634DDDAF4B2B}" type="slidenum"><a:t>1</a:t></a:fld></a:p>',
    ]
    workbook = f"""{DECL}<workbook xmlns="{M}" xmlns:r="{R}"><sheets><sheet name="S" sheetId="1" r:id="rId1"/></sheets></workbook>"""
    return [
        ("[Content_Types].xml", types([
            ("/xl/workbook.xml", "spreadsheetml.sheet.main+xml"),
            ("/xl/worksheets/sheet1.xml", "spreadsheetml.worksheet+xml"),
            ("/xl/drawings/drawing1.xml", "drawing+xml"),
        ])),
        ("_rels/.rels", rels([("rId1", "officeDocument", "xl/workbook.xml")])),
        ("xl/workbook.xml", workbook),
        ("xl/_rels/workbook.xml.rels", rels([("rId1", "worksheet", "worksheets/sheet1.xml")])),
        ("xl/worksheets/sheet1.xml", sheet1),
        ("xl/worksheets/_rels/sheet1.xml.rels", rels([("rId1", "drawing", "../drawings/drawing1.xml")])),
        ("xl/drawings/drawing1.xml", drawing(paras, fallback=False)),
    ]


def main() -> None:
    write("locos_walk_1.xlsx", walk_book())
    write("locos_textbox_runs_1.xlsx", runs_book())
    print("wrote", OUT)


if __name__ == "__main__":
    main()

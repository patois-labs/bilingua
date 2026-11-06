"""Writes the gen_*.docx inputs of bilingua/tests/locos/docx for the LocOS goldens (Track D group B): hand-made packages that reach the paths the
original corpus does not (hyperlinks, text boxes, tracked changes, fields, notes, tables, content controls,
markers, smart tags, wrappers, no styles part, pretty-printed XML, a Storyline table, alt text in a header).

    python oracle/gen_locos_docx.py bilingua/tests/locos/docx
    <local-tms>/.venv/Scripts/python.exe oracle/locos_golden.py docx
"""
import os
import sys
import zipfile

OUT = sys.argv[1]
W = "http://schemas.openxmlformats.org/wordprocessingml/2006/main"
R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
NS = (
    f'xmlns:w="{W}" xmlns:r="{R}" '
    'xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" '
    'xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" '
    'xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" '
    'xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" '
    'xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape" '
    'xmlns:v="urn:schemas-microsoft-com:vml"'
)
DECL = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
CT_MAIN = "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"
WML = "application/vnd.openxmlformats-officedocument.wordprocessingml."
STYLES = (
    '<w:docDefaults><w:rPrDefault><w:rPr><w:lang w:val="en-US" w:eastAsia="en-US" w:bidi="ar-SA"/></w:rPr></w:rPrDefault></w:docDefaults>'
    '<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>'
    '<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/></w:style>'
    '<w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/></w:style>'
    '<w:style w:type="paragraph" w:styleId="German"><w:name w:val="German text"/><w:rPr><w:lang w:val="de-DE"/></w:rPr></w:style>'
    '<w:style w:type="character" w:styleId="Emph"><w:name w:val="Emphasis"/><w:rPr><w:i/></w:rPr></w:style>'
)


def package(name, body, parts=(), styles=STYLES, core=True, pretty=False):
    """parts: (file under word/, reltype tail, content-type tail, root xml)."""
    ct = [
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>',
        '<Default Extension="xml" ContentType="application/xml"/>',
        f'<Override PartName="/word/document.xml" ContentType="{CT_MAIN}"/>',
    ]
    rels = []
    if styles is not None:
        ct.append(f'<Override PartName="/word/styles.xml" ContentType="{WML}styles+xml"/>')
        rels.append(f'<Relationship Id="rIdS" Type="{R}/styles" Target="styles.xml"/>')
    if core:
        ct.append('<Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>')
    for i, (fname, kind, ctype, _xml) in enumerate(parts):
        ct.append(f'<Override PartName="/word/{fname}" ContentType="{WML}{ctype}"/>')
        rels.append(f'<Relationship Id="rId{i + 10}" Type="{R}/{kind}" Target="{fname}"/>')
    rels.append('<Relationship Id="rIdH" Type="' + R + '/hyperlink" Target="https://example.com/" TargetMode="External"/>')
    pkg_rels = f'<Relationship Id="rId1" Type="{R}/officeDocument" Target="word/document.xml"/>'
    if core:
        pkg_rels += '<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>'
    sep = "\n  " if pretty else ""
    doc = DECL + f"<w:document {NS}>{sep}<w:body>{sep}" + sep.join(body) + f"{sep}<w:sectPr/>{sep}</w:body>\n</w:document>"
    path = os.path.join(OUT, name)
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        z.writestr("[Content_Types].xml", DECL + '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">' + "".join(ct) + "</Types>")
        z.writestr("_rels/.rels", DECL + '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">' + pkg_rels + "</Relationships>")
        z.writestr("word/document.xml", doc)
        z.writestr("word/_rels/document.xml.rels", DECL + '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">' + "".join(rels) + "</Relationships>")
        if styles is not None:
            z.writestr("word/styles.xml", DECL + f'<w:styles xmlns:w="{W}">{styles}</w:styles>')
        if core:
            z.writestr("docProps/core.xml", DECL + '<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/"><dc:title>Fixture</dc:title></cp:coreProperties>')
        for fname, _k, _c, xml in parts:
            z.writestr("word/" + fname, DECL + xml)


def p(*runs, ppr=""):
    return f"<w:p>{ppr}{''.join(runs)}</w:p>"


def r(text, rpr="", preserve=True):
    sp = ' xml:space="preserve"' if preserve and text != text.strip() else ""
    return f"<w:r>{rpr}<w:t{sp}>{text}</w:t></w:r>"


B = "<w:rPr><w:b/><w:sz w:val=\"24\"/></w:rPr>"
I = "<w:rPr><w:rStyle w:val=\"Emph\"/><w:i/></w:rPr>"

# 1. hyperlinks, a run split by a picture, bookmarks and proofing marks around runs
pic = '<w:r><w:drawing><wp:inline><wp:extent cx="1" cy="1"/><wp:docPr id="7" name="Pic 7" descr="A blue circle" title="Circle"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="0" name="c.png" descr="A blue circle"/><pic:cNvPicPr/></pic:nvPicPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>'
package(
    "gen_links_marks_1.docx",
    [
        p(r("Visit "), '<w:hyperlink r:id="rIdH" w:history="1">' + r("our site", B) + r(" today", I) + "</w:hyperlink>", r(" please.")),
        p('<w:bookmarkStart w:id="3" w:name="b1"/>', r("Marked "), '<w:proofErr w:type="spellStart"/>', r("wrod", B), '<w:proofErr w:type="spellEnd"/>', '<w:bookmarkEnd w:id="3"/>', r(" end.")),
        p(r("Before the picture "), pic, r(" and after it.", B)),
        p("<w:r><w:t>Line one</w:t><w:br/><w:t>line two</w:t><w:tab/><w:t>tabbed</w:t></w:r>"),
        p('<w:r><w:t>Only space runs</w:t></w:r><w:r><w:t xml:space="preserve">   </w:t></w:r>'),
        p(r("Heading text"), ppr='<w:pPr><w:pStyle w:val="Heading1"/><w:jc w:val="center"/></w:pPr>'),
        p(r("Title text"), ppr='<w:pPr><w:pStyle w:val="Title"/><w:spacing w:after="0"/></w:pPr>'),
        p(r("Deutsch im Stil"), ppr='<w:pPr><w:pStyle w:val="German"/></w:pPr>'),
    ],
)

# 2. tracked changes, complex fields, a simple field, a smart tag in a smart tag, a content control, bdo
fld = '<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>3</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>'
package(
    "gen_revisions_fields_1.docx",
    [
        p(r("Kept "), '<w:ins w:id="11" w:author="A" w:date="2026-01-01T00:00:00Z">' + r("inserted words", B) + "</w:ins>", '<w:del w:id="12" w:author="A"><w:r><w:delText>gone</w:delText></w:r></w:del>', r(" tail.")),
        p(r("Page "), fld, r(" of the book.")),
        p(r("Simple "), '<w:fldSimple w:instr=" DATE "><w:r><w:t>today</w:t></w:r></w:fldSimple>', r(" field.")),
        p(r("In "), '<w:smartTag w:uri="urn:x" w:element="place"><w:smartTagPr><w:attr w:name="a" w:val="b"/></w:smartTagPr><w:smartTag w:uri="urn:x" w:element="City">' + r("Paris", B) + "</w:smartTag></w:smartTag>", r(" today.")),
        p(r("Pick "), '<w:sdt><w:sdtPr><w:id w:val="99"/><w:text/></w:sdtPr><w:sdtContent>' + r("free text", I) + "</w:sdtContent></w:sdt>", r(" now.")),
        p(r("Choose "), '<w:sdt><w:sdtPr><w:dropDownList/></w:sdtPr><w:sdtContent>' + r("Option") + "</w:sdtContent></w:sdt>", r(" one.")),
        p(r("Name "), '<w:bdo w:val="ltr">' + r("Acme") + "</w:bdo>", r(" here.")),
        p('<w:moveTo w:id="21" w:author="A">' + r("moved here") + "</w:moveTo>", '<w:moveFrom w:id="22" w:author="A"><w:r><w:t>moved away</w:t></w:r></w:moveFrom>'),
    ],
)

# 3. tables: merged cells, a nested table, a block content control, paragraphs between tables
tc = lambda inner, pr="": f"<w:tc>{pr}{inner}</w:tc>"
nested = "<w:tbl><w:tr>" + tc(p(r("Inner A"))) + tc(p(r("Inner B"))) + "</w:tr></w:tbl>"
package(
    "gen_tables_1.docx",
    [
        p(r("Before the table.")),
        "<w:tbl><w:tblPr/><w:tr>" + tc(p(r("Top left")), '<w:tcPr><w:vMerge w:val="restart"/></w:tcPr>') + tc(p(r("Top right")) + nested + p(r("After nested"))) + "</w:tr>"
        + "<w:tr>" + tc(p(), "<w:tcPr><w:vMerge/></w:tcPr>") + tc(p(r("Bottom right"), r(" bold", B))) + "</w:tr></w:tbl>",
        '<w:sdt><w:sdtPr><w:id w:val="5"/></w:sdtPr><w:sdtContent>' + p(r("Inside a block control.")) + "</w:sdtContent></w:sdt>",
        p(r("After the table.")),
    ],
)

# 4. a text box (Choice and Fallback copies), notes, header and footer with a drawing, a comment
box = lambda t: f"<w:txbxContent>{p(r(t))}</w:txbxContent>"
textbox = (
    '<w:r><mc:AlternateContent><mc:Choice Requires="wps"><w:drawing><wp:anchor><wp:docPr id="2" name="Box"/><a:graphic><a:graphicData uri="x"><wps:wsp><wps:txbx>'
    + box("Box text")
    + '</wps:txbx></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></mc:Choice><mc:Fallback><w:pict><v:shape><v:textbox>'
    + box("Box text")
    + "</v:textbox></v:shape></w:pict></mc:Fallback></mc:AlternateContent></w:r>"
)
notes = (
    f'<w:footnotes {NS}><w:footnote w:type="separator" w:id="-1"><w:p><w:r><w:separator/></w:r></w:p></w:footnote>'
    f'<w:footnote w:id="1"><w:p><w:r><w:footnoteRef/></w:r>{r(" A footnote.")}</w:p></w:footnote></w:footnotes>'
)
header = f'<w:hdr {NS}>' + p(r("Header line"), '<w:r><w:drawing><wp:inline><wp:docPr id="9" name="Logo" descr="Company logo"/></wp:inline></w:drawing></w:r>') + "</w:hdr>"
footer = f'<w:ftr {NS}>' + p(r("Footer line")) + "</w:ftr>"
comments = f'<w:comments {NS}><w:comment w:id="0" w:author="R"><w:p><w:r><w:annotationRef/></w:r>{r("Check this.")}</w:p></w:comment></w:comments>'
package(
    "gen_parts_boxes_1.docx",
    [
        p(r("Anchor "), textbox, r(" paragraph.")),
        p(r("Noted"), '<w:r><w:footnoteReference w:id="1"/></w:r>', r(" text.")),
        p('<w:commentRangeStart w:id="0"/>', r("Commented"), '<w:commentRangeEnd w:id="0"/>', '<w:r><w:commentReference w:id="0"/></w:r>'),
    ],
    parts=[
        ("footer1.xml", "footer", "footer+xml", footer),
        ("header1.xml", "header", "header+xml", header),
        ("footnotes.xml", "footnotes", "footnotes+xml", notes),
        ("comments.xml", "comments", "comments+xml", comments),
    ],
)

# 5. no styles part, no core properties, pretty-printed XML
package(
    "gen_no_styles_pretty_1.docx",
    [p(r("First line")), p(r("Second "), r("bold", B)), p(r("  spaced  "))],
    styles=None,
    core=False,
    pretty=True,
)

# 6. a Storyline "Export to Word" table
row = lambda a, b, c: "<w:tr>" + tc(p(r(a))) + tc(p(r(b))) + tc(p(r(c) if c else "")) + "</w:tr>"
package(
    "gen_storyline_1.docx",
    [
        p(r("Scene 1"), ppr='<w:pPr><w:pStyle w:val="Heading1"/></w:pPr>'),
        "<w:tbl>" + row("ID", "Original Text", "Translation") + row("1", "Welcome to the course", "Welcome to the course") + row("2", "Click next", "Click next") + row("3", "Empty one", "") + "</w:tbl>",
    ],
)
print("written")

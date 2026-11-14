"""Writes the made-up OpenDocument oracle corpus: python tools/gen_corpus.py OUT_DIR

Four packages (odt, ods, odp, odg) with every feature the filter treats: spans, links, tabs, line breaks, extra
spaces, bookmarks, footnotes, comments, tracked changes, frames, tables, lists, headers and footers, string / number /
formula / date cells, repeated cells, notes, shapes, and the 11 test languages including Arabic (right to left).
Deterministic: fixed time stamps. Nothing here is from a real document.
"""
import io, os, sys, zipfile

NS = (
    'xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" '
    'xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" '
    'xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" '
    'xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" '
    'xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" '
    'xmlns:presentation="urn:oasis:names:tc:opendocument:xmlns:presentation:1.0" '
    'xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" '
    'xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0" '
    'xmlns:xlink="http://www.w3.org/1999/xlink" '
    'xmlns:dc="http://purl.org/dc/elements/1.1/" '
    'xmlns:meta="urn:oasis:names:tc:opendocument:xmlns:meta:1.0" '
    'office:version="1.3"'
)
HDR = '<?xml version="1.0" encoding="UTF-8"?>\n'
LINES = [
    ("en", "Welcome to the harbor lantern show"),
    ("es", "Bienvenidos al espectáculo del faro"),
    ("fr", "Bienvenue au spectacle de la lanterne"),
    ("it", "Benvenuti allo spettacolo del faro"),
    ("de", "Willkommen zur Laternenshow am Hafen"),
    ("nl", "Welkom bij de lantaarnshow in de haven"),
    ("pt", "Bem-vindos ao espetáculo da lanterna"),
    ("ja", "港のランタンショーへようこそ"),
    ("zh", "欢迎观看港口灯笼表演"),
    ("ko", "항구 등불 쇼에 오신 것을 환영합니다"),
    ("ar", "مرحبا بكم في عرض فانوس الميناء"),
]


def meta(title):
    return (HDR + f'<office:document-meta {NS}><office:meta><dc:title>{title}</dc:title>'
            '<dc:language>en-US</dc:language><meta:generator>Bilingua corpus generator</meta:generator>'
            '</office:meta></office:document-meta>')


def manifest(mime, names):
    e = f'<manifest:file-entry manifest:full-path="/" manifest:media-type="{mime}"/>'
    for n in names:
        e += f'<manifest:file-entry manifest:full-path="{n}" manifest:media-type="text/xml"/>'
    return (HDR + '<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0" '
            f'manifest:version="1.3">{e}</manifest:manifest>')


def package(path, mime, content, styles, title):
    names = ["content.xml", "styles.xml", "meta.xml"]
    buf = io.BytesIO()
    t = (2026, 11, 4, 9, 0, 0)
    with zipfile.ZipFile(buf, "w") as z:
        zi = zipfile.ZipInfo("mimetype", date_time=t)
        zi.compress_type = zipfile.ZIP_STORED
        z.writestr(zi, mime)
        for n, d in (("content.xml", content), ("styles.xml", styles), ("meta.xml", meta(title)),
                     ("META-INF/manifest.xml", manifest(mime, names))):
            zi = zipfile.ZipInfo(n, date_time=t)
            zi.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(zi, d)
    open(path, "wb").write(buf.getvalue())


def styles(masters="", extra=""):
    return (HDR + f'<office:document-styles {NS}><office:styles>'
            '<style:default-style style:family="paragraph"><style:paragraph-properties style:writing-mode="lr-tb"/>'
            '<style:text-properties fo:language="en" fo:country="US" style:language-asian="ja" '
            'style:country-asian="JP" style:language-complex="ar" style:country-complex="SA"/></style:default-style>'
            '<style:style style:name="Standard" style:family="paragraph"/>'
            '<style:style style:name="Code" style:family="text"><style:text-properties fo:language="zxx" fo:country="none"/></style:style>'
            + extra + f'</office:styles><office:master-styles>{masters}</office:master-styles></office:document-styles>')


def odt(out):
    paras = "".join(f'<text:p text:style-name="P{i}">{t}</text:p>' for i, (_, t) in enumerate(LINES))
    c = (HDR + f'<office:document-content {NS}><office:automatic-styles>'
         '<style:style style:name="T1" style:family="text"><style:text-properties fo:font-weight="bold"/></style:style>'
         '</office:automatic-styles><office:body><office:text>'
         '<text:tracked-changes><text:changed-region text:id="ct1"><text:deletion><office:change-info>'
         '<dc:creator>Ann</dc:creator></office:change-info><text:p>Words that were deleted</text:p></text:deletion>'
         '</text:changed-region></text:tracked-changes>'
         '<text:h text:style-name="Heading_20_1" text:outline-level="1">Lantern festival guide</text:h>'
         '<text:p text:style-name="Standard">Press <text:span text:style-name="T1">Start</text:span> to open the '
         'gate at <text:a xlink:href="https://harbor.example/gate">the east pier</text:a>.<text:bookmark text:name="b1"/>'
         '<text:note text:id="ftn1" text:note-class="footnote"><text:note-citation>1</text:note-citation><text:note-body>'
         '<text:p>The east pier closes at nine.</text:p></text:note-body></text:note></text:p>'
         '<text:p>Two<text:s text:c="3"/>gaps,<text:tab/>a tab and<text:line-break/>a new line.</text:p>'
         '<text:p>Before<office:annotation><dc:creator>Bo</dc:creator><text:p>Check this wording</text:p>'
         '</office:annotation> after <text:change-start text:change-id="ct1"/>the change<text:change-end text:change-id="ct1"/>.</text:p>'
         '<text:h text:outline-level="2">Visitors by day</text:h>'
         '<table:table table:name="Table1"><table:table-row>'
         '<table:table-cell office:value-type="string"><text:p>Day</text:p></table:table-cell>'
         '<table:table-cell office:value-type="string"><text:p>Visitors</text:p></table:table-cell></table:table-row>'
         '<table:table-row><table:table-cell office:value-type="string"><text:p>Saturday</text:p></table:table-cell>'
         '<table:table-cell office:value-type="float" office:value="1200"><text:p>1200</text:p></table:table-cell></table:table-row>'
         '</table:table>'
         '<text:list><text:list-item><text:p>Bring a lantern</text:p></text:list-item>'
         '<text:list-item><text:p>Wear <text:span text:style-name="T1">warm</text:span> clothes</text:p></text:list-item></text:list>'
         '<text:p>Caption<draw:frame draw:name="F1"><draw:text-box><text:p>Text inside a frame</text:p></draw:text-box></draw:frame></text:p>'
         '<text:p>   </text:p><text:p>2026</text:p>'
         '<text:p><text:span text:style-name="Code">code_sample()</text:span> is not translated as code</text:p>'
         + paras.replace('text:style-name="P', 'text:style-name="Lang') +
         '</office:text></office:body></office:document-content>')
    s = styles('<style:master-page style:name="Standard"><style:header><text:p>Lantern guide header</text:p></style:header>'
               '<style:footer><text:p>Page <text:page-number>1</text:page-number> of the guide</text:p></style:footer>'
               '</style:master-page>')
    package(os.path.join(out, "guide.odt"), "application/vnd.oasis.opendocument.text", c, s, "Lantern festival guide")


def ods(out):
    cells = "".join(f'<table:table-row><table:table-cell office:value-type="string"><text:p>{t}</text:p></table:table-cell>'
                    f'<table:table-cell office:value-type="float" office:value="{i}"><text:p>{i}</text:p></table:table-cell></table:table-row>'
                    for i, (_, t) in enumerate(LINES))
    c = (HDR + f'<office:document-content {NS}><office:body><office:spreadsheet>'
         '<table:table table:name="Sales"><table:table-row>'
         '<table:table-cell office:value-type="string"><text:p>Product</text:p></table:table-cell>'
         '<table:table-cell office:value-type="string"><text:p>Total <text:span>due</text:span></text:p></table:table-cell>'
         '<table:table-cell table:number-columns-repeated="3" office:value-type="string"><text:p>Repeated label</text:p></table:table-cell>'
         '</table:table-row><table:table-row>'
         '<table:table-cell office:value-type="string"><text:p>Paper lanterns</text:p></table:table-cell>'
         '<table:table-cell table:formula="of:=SUM([.B3:.B9])" office:value-type="float" office:value="42"><text:p>42</text:p></table:table-cell>'
         '<table:table-cell table:formula="of:=CONCAT(&quot;a&quot;;&quot;b&quot;)" office:value-type="string" office:string-value="ab"><text:p>ab</text:p></table:table-cell>'
         '<table:table-cell office:value-type="date" office:date-value="2026-10-10"><text:p>Oct 10, 2026</text:p></table:table-cell>'
         '<table:table-cell office:value-type="string"><text:p>With a note</text:p><office:annotation><text:p>Cell comment text</text:p></office:annotation></table:table-cell>'
         '</table:table-row></table:table>'
         f'<table:table table:name="Languages">{cells}</table:table>'
         '</office:spreadsheet></office:body></office:document-content>')
    package(os.path.join(out, "sales.ods"), "application/vnd.oasis.opendocument.spreadsheet", c, styles(), "Sales")


def odp(out):
    pages = ""
    for i, (_, t) in enumerate(LINES[:4]):
        pages += (f'<draw:page draw:name="Slide {i + 1}" draw:master-page-name="Default">'
                  f'<draw:frame presentation:class="title"><draw:text-box><text:p>{t}</text:p></draw:text-box></draw:frame>'
                  '<draw:frame presentation:class="outline"><draw:text-box><text:list><text:list-item>'
                  '<text:p>Revenue <text:span>up</text:span> 20%</text:p></text:list-item></text:list></draw:text-box></draw:frame>'
                  '<presentation:notes><draw:frame><draw:text-box><text:p>Speak slowly on this slide</text:p></draw:text-box></draw:frame>'
                  '</presentation:notes></draw:page>')
    rtl = LINES[10][1]
    pages += (f'<draw:page draw:name="Slide 5" draw:master-page-name="Default"><draw:frame presentation:class="title">'
              f'<draw:text-box><text:p>{rtl}</text:p></draw:text-box></draw:frame></draw:page>')
    c = (HDR + f'<office:document-content {NS}><office:automatic-styles>'
         '<style:style style:name="gr1" style:family="graphic"><style:paragraph-properties style:writing-mode="lr-tb"/></style:style>'
         f'</office:automatic-styles><office:body><office:presentation>{pages}</office:presentation></office:body></office:document-content>')
    m = ('<style:master-page style:name="Default"><draw:frame presentation:class="title"><draw:text-box>'
         '<text:p>Click to edit the title</text:p></draw:text-box></draw:frame></style:master-page>')
    package(os.path.join(out, "slides.odp"), "application/vnd.oasis.opendocument.presentation", c, styles(m), "Slides")


def odg(out):
    c = (HDR + f'<office:document-content {NS}><office:body><office:drawing>'
         '<draw:page draw:name="page1"><draw:custom-shape><text:p>Start here</text:p></draw:custom-shape>'
         '<draw:rect><text:p>Decision <text:span>point</text:span></text:p></draw:rect>'
         '<draw:ellipse><text:p>Finish</text:p></draw:ellipse></draw:page></office:drawing></office:body></office:document-content>')
    package(os.path.join(out, "flow.odg"), "application/vnd.oasis.opendocument.graphics", c, styles(), "Flow")


if __name__ == "__main__":
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    odt(out)
    ods(out)
    odp(out)
    odg(out)

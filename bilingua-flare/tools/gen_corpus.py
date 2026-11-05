"""Writes the made-up Flare corpus (stdlib only). Run from anywhere:

    python tools/gen_corpus.py OUT_DIR

Lumen Bakery is a fictional company; no real text. Three projects: the LocOS suite's own project (help_en), a project
with a topic in each of eleven languages (languages) and a project with lists, tables, nested MadCap inline elements,
entities, notranslate, keywords, a BOM and CRLF, typed variables and a mixed-title TOC (rich).
"""
import io
import os
import sys
import zipfile

NS = 'xmlns:MadCap="http://www.madcapsoftware.com/Schemas/MadCap.xsd"'
DECL = '<?xml version="1.0" encoding="utf-8"?>\n'

TOPIC = (
    DECL
    + f'<html {NS} lang="en-us" xml:lang="en-us">\n'
    "    <head><title>Printing guide</title>\n"
    '    <link href="../Resources/Stylesheets/Styles.css" rel="stylesheet" type="text/css" />\n'
    "    </head>\n"
    "    <body>\n"
    '        <h1>Print with <MadCap:variable name="General.ProductName" /></h1>\n'
    '        <p MadCap:conditions="Default.PrintOnly">Install <MadCap:variable name="General.ProductName">Acme Writer</MadCap:variable> before you print, don\'t skip it.</p>\n'
    '        <p>Read the <MadCap:glossaryTerm glossTerm="Glossary.Term0">spooler</MadCap:glossaryTerm> notes and see <MadCap:xref href="Other.htm">Other topic</MadCap:xref>.<MadCap:keyword term="Printing;Printing:Color;\\:Colon start" /></p>\n'
    '        <p>Text for <MadCap:conditionalText MadCap:conditions="Default.Online">online readers only</MadCap:conditionalText> and everyone.</p>\n'
    '        <p>Brand line: <MadCap:snippetText src="../Resources/Snippets/Brand.flsnp" /> ends here.</p>\n'
    '        <MadCap:snippetBlock src="../Resources/Snippets/Outer.flsnp" />\n'
    "        <MadCap:dropDown>\n"
    "            <MadCap:dropDownHead>\n"
    "                <MadCap:dropDownHotspot>More details</MadCap:dropDownHotspot>\n"
    "            </MadCap:dropDownHead>\n"
    "            <MadCap:dropDownBody>\n"
    "                <p>Hidden body text.</p>\n"
    "            </MadCap:dropDownBody>\n"
    "        </MadCap:dropDown>\n"
    '        <p><img src="../Resources/Images/logo.png" alt="Company logo" /> Spacing&#160;kept.</p>\n'
    "    </body>\n"
    "</html>\n"
)
OUTER = DECL + f"<html {NS}>\n<body>\n<p>Outer snippet text.</p>\n" '<MadCap:snippetBlock src="Inner.flsnp" />\n</body>\n</html>\n'
INNER = (
    DECL
    + f'<html {NS}>\n<body>\n<p>Inner snippet with <MadCap:conditionalText MadCap:conditions="Default.Print">print words</MadCap:conditionalText>.</p>\n</body>\n</html>\n'
)
BRAND = DECL + f"<html {NS}>\n<body><p>Acme makes writing tools</p></body>\n</html>\n"
FLVAR = (
    DECL
    + "<CatapultVariableSet>\n"
    '  <Variable Name="ProductName" Comment="Do not abbreviate" EvaluatedDefinition="Acme Writer">Acme Writer</Variable>\n'
    '  <Variable Name="Tagline" EvaluatedDefinition="Write &amp; ship">Write &amp; ship</Variable>\n'
    '  <Variable Name="Year" EvaluatedDefinition="2026">2026</Variable>\n'
    '  <Variable Name="Updated" Type="DateTime" Format="MMMM dd, yyyy" EvaluatedDefinition="October 10, 2026">October 10, 2026</Variable>\n'
    "</CatapultVariableSet>\n"
)
FLTOC = (
    DECL
    + '<CatapultToc Version="1">\n'
    '  <TocEntry Title="Getting started" Link="/Content/Topic.htm">\n'
    '    <TocEntry Title="[%=System.LinkedTitle%]" Link="/Content/Intl.htm" />\n'
    f'    <TocEntry Title="Guide to [%=General.ProductName%] printing" Link="/Content/Topic.htm" MadCap:conditions="Default.PrintOnly" {NS} />\n'
    "  </TocEntry>\n"
    "</CatapultToc>\n"
)
FLGLO = (
    DECL
    + "<CatapultGlossary>\n"
    '  <GlossaryEntry glossTerm="Glossary.Term0">\n'
    "    <Terms>\n      <Term>spooler</Term>\n    </Terms>\n"
    '    <Definition Link="">A queue that holds print jobs.</Definition>\n'
    "  </GlossaryEntry>\n"
    "</CatapultGlossary>\n"
)
FLCTS = DECL + '<CatapultConditionTagSet>\n  <ConditionTag Name="PrintOnly" BackgroundColor="#ff0000" />\n</CatapultConditionTagSet>\n'
FLPRJ = DECL + '<CatapultProject Version="1" xml:lang="en-us" />\n'
PNG = b"\x89PNG\r\n\x1a\n" + bytes(range(256)) * 2
CSS = b"p { color: #333; }\n"

LANGS = {
    "en": ("Bake the bread", "Mix the flour, water and salt, then rest the dough."),
    "ar": ("اخبز الخبز", "اخلط الدقيق والماء ثم اترك العجين."),
    "ja": ("パンを焼く", "小麦粉と水と塩を混ぜて生地を休ませます。"),
    "zh": ("烘烤面包", "将面粉、水和盐混合，然后醒面。"),
    "ko": ("빵 굽기", "밀가루와 물과 소금을 섞고 반죽을 쉬게 하세요."),
    "ru": ("Испечь хлеб", "Смешайте муку, воду и соль."),
    "de": ("Brot backen", "Mehl, Wasser und Salz mischen und den Teig ruhen lassen."),
    "fr": ("Cuire le pain", "Mélangez la farine, l'eau et le sel, puis laissez reposer la pâte."),
    "es": ("Hornear el pan", "Mezcle la harina, el agua y la sal y deje reposar la masa."),
    "hi": ("रोटी बनाएं", "आटा, पानी और नमक मिलाएं।"),
    "th": ("อบขนมปัง", "ผสมแป้ง น้ำ และเกลือ"),
}


def lang_topic(code, head, body):
    rtl = ' dir="rtl"' if code == "ar" else ""
    return (
        DECL
        + f'<html {NS} lang="{code}" xml:lang="{code}"{rtl}>\n<head><title>{head}</title></head>\n<body>\n'
        f'<h1>{head}</h1>\n<p>{body} <MadCap:variable name="General.ProductName" /></p>\n'
        f'<p><MadCap:conditionalText MadCap:conditions="Default.Online">{body}</MadCap:conditionalText></p>\n'
        f'<MadCap:keyword term="{head};{head}:{body[:6]}" />\n</body>\n</html>\n'
    )


RICH = (
    "﻿"
    + DECL.replace("\n", "\r\n")
    + f'<html {NS} lang="en"><head><title>Oven care</title></head>\r\n<body>\r\n'
    "<h2>Daily care</h2>\r\n"
    "<ul><li>Wipe the <b>door</b> glass.</li><li>Empty the <i>crumb</i> tray &amp; check it.</li></ul>\r\n"
    "<table><tr><th>Part</th><th>Task</th></tr><tr><td>Fan</td><td>Dust it weekly.</td></tr></table>\r\n"
    '<p>See <MadCap:xref href="a.htm"><MadCap:conditionalText MadCap:conditions="Default.Print">the <b>print</b> guide</MadCap:conditionalText></MadCap:xref> for <span translate="no">OVN-100</span> units.</p>\r\n'
    '<p>Caf&eacute; r&#233;sum&#233; &lt;tag&gt; and <code>code_sample()</code> stay.</p>\r\n'
    '<p>Call <a href="tel:1" title="Phone the bakery">us</a> <MadCap:snippetText src="s.flsnp"></MadCap:snippetText> now.</p>\r\n'
    '<MadCap:keyword term="Ovens:Cleaning;Safety" />\r\n'
    '<MadCap:popup><MadCap:popupHead><MadCap:popupTitle>Tip</MadCap:popupTitle></MadCap:popupHead><MadCap:popupBody><p>Never use water.</p></MadCap:popupBody></MadCap:popup>\r\n'
    "</body></html>\r\n"
)
RICH_FLVAR = (
    DECL
    + "<CatapultVariableSet>\n"
    '  <Variable Name="Hours" EvaluatedDefinition="Open daily">Open daily</Variable>\n'
    '  <Variable Name="Phone" Type="Text">+1 555 0100 ext. A</Variable>\n'
    '  <Variable Name="Count">42</Variable>\n'
    '  <Variable Name="Empty" />\n'
    "</CatapultVariableSet>\n"
)
RICH_FLTOC = (
    DECL
    + "<CatapultToc>\n"
    '  <TocEntry Title="Oven care &amp; safety">\n'
    '    <TocEntry Title="  Daily [%=Count%] checks  " />\n'
    '    <TocEntry Title="[%=A%][%=B%]" />\n'
    '    <TocEntry Title="オーブン" />\n'
    "  </TocEntry>\n"
    "</CatapultToc>\n"
)
RICH_FLGLO = (
    DECL
    + "<CatapultGlossary>\n"
    '  <GlossaryEntry><Terms><Term>proofing</Term></Terms><Definition>Letting dough rise <b>before</b> baking.</Definition></GlossaryEntry>\n'
    '  <GlossaryEntry><Terms><Term>crumb</Term></Terms><Definition>The soft inside of a loaf.</Definition></GlossaryEntry>\n'
    "</CatapultGlossary>\n"
)


def build(members, comment=b""):
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as zf:
        zf.comment = comment
        for name, data in members:
            zi = zipfile.ZipInfo(name, date_time=(2026, 10, 1, 9, 30, 0))
            zi.compress_type = zipfile.ZIP_STORED if name.endswith(".png") else zipfile.ZIP_DEFLATED
            zf.writestr(zi, data)
    return buf.getvalue()


def enc(s):
    return s.encode("utf-8")


def main(out):
    os.makedirs(out, exist_ok=True)
    help_en = [
        ("Content/", b""),
        ("Content/Topic.htm", enc(TOPIC)),
        ("Content/Resources/Snippets/Outer.flsnp", enc(OUTER)),
        ("Content/Resources/Snippets/Inner.flsnp", enc(INNER)),
        ("Content/Resources/Snippets/Brand.flsnp", enc(BRAND)),
        ("Content/Resources/Stylesheets/Styles.css", CSS),
        ("Content/Resources/Images/logo.png", PNG),
        ("Project/VariableSets/General.flvar", enc(FLVAR)),
        ("Project/TOCs/Master.fltoc", enc(FLTOC)),
        ("Project/Glossaries/Glossary.flglo", enc(FLGLO)),
        ("Project/ConditionTagSets/Default.flcts", enc(FLCTS)),
        ("Help.flprj", enc(FLPRJ)),
    ]
    langs = [("Help.flprj", enc(FLPRJ)), ("Project/VariableSets/General.flvar", enc(FLVAR))]
    for code, (h, b) in LANGS.items():
        langs.append((f"Content/{code}/Bread.htm", enc(lang_topic(code, h, b))))
    rich = [
        ("Help.flprj", enc(FLPRJ)),
        ("Content/Oven.htm", enc(RICH)),
        ("Project/VariableSets/Shop.flvar", enc(RICH_FLVAR)),
        ("Project/TOCs/Oven.fltoc", enc(RICH_FLTOC)),
        ("Project/Glossaries/Bake.flglo", enc(RICH_FLGLO)),
        ("Content/Resources/Images/logo.png", PNG),
    ]
    for name, members, comment in (("help_en", help_en, b"flare fixture"), ("languages", langs, b""), ("rich", rich, b"")):
        with open(os.path.join(out, name + ".flprjzip"), "wb") as fh:
            fh.write(build(members, comment))


if __name__ == "__main__":
    main(sys.argv[1])

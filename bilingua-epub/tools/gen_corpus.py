"""Writes the made-up EPUB corpus (stdlib only). Run from anywhere:

    python tools/gen_corpus.py OUT_DIR

Lumen Bakery is a fictional company; no real text. Four books: the LocOS suite's own book (guide), an EPUB 2 book
with a UTF-16 chapter and a Windows-1252 chapter (legacy2), eleven languages in eleven chapters (languages), and a
book with lists, tables, inline markup, entities, notranslate and link/image attributes (rich).
"""
import io
import os
import sys
import zipfile

CONTAINER = """<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>
"""


def opf(version, title, lang, manifest, spine, desc="", nav=True, ncx=True):
    items = []
    if nav:
        items.append('<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>')
    if ncx:
        items.append('<item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>')
    items += manifest
    refs = "".join(f'<itemref idref="{i}"/>' for i in spine)
    d = f"<dc:description>{desc}</dc:description>" if desc else ""
    return f"""<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="{version}" unique-identifier="uid" xml:lang="{lang}">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="uid">urn:uuid:aaaaaaaa-1111-2222-3333-444444444444</dc:identifier>
    <dc:title>{title}</dc:title>
    <dc:creator>Lumen Bakery Press</dc:creator>
    <dc:language>{lang}</dc:language>
    {d}
  </metadata>
  <manifest>
    {"".join(items)}
  </manifest>
  <spine{' toc="ncx"' if ncx else ''}>{refs}</spine>
</package>
"""


def xhtml(title, body, lang="en", decl='<?xml version="1.0" encoding="UTF-8"?>\n'):
    return f"""{decl}<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" lang="{lang}" xml:lang="{lang}">
<head><title>{title}</title></head>
<body>
{body}
</body>
</html>
"""


def nav(entries):
    lis = "\n".join(f'<li><a href="{h}">{t}</a></li>' for h, t in entries)
    return xhtml("Contents", f'<nav xmlns:epub="http://www.idpf.org/2007/ops" epub:type="toc"><h2>Contents</h2><ol>\n{lis}\n</ol></nav>')


def ncx(title, entries, lang="en"):
    pts = "".join(
        f'<navPoint id="p{i}" playOrder="{i}"><navLabel><text>{t}</text></navLabel><content src="{h}"/></navPoint>'
        for i, (h, t) in enumerate(entries, 1)
    )
    return f"""<?xml version="1.0" encoding="UTF-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1" xml:lang="{lang}">
  <head><meta name="dtb:uid" content="urn:uuid:aaaaaaaa-1111-2222-3333-444444444444"/></head>
  <docTitle><text>{title}</text></docTitle>
  <navMap>{pts}</navMap>
</ncx>
"""


def write(path, members, mimetype=b"application/epub+zip"):
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as z:
        z.writestr(zipfile.ZipInfo("mimetype", date_time=(2026, 11, 4, 9, 0, 0)), mimetype)
        for name, data in members:
            zi = zipfile.ZipInfo(name, date_time=(2026, 11, 4, 9, 0, 0))
            zi.compress_type = zipfile.ZIP_DEFLATED
            z.writestr(zi, data if isinstance(data, bytes) else data.encode("utf-8"))
    with open(path, "wb") as fh:
        fh.write(buf.getvalue())


def guide(out):
    ch1 = """<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml"
  xmlns:epub="http://www.idpf.org/2007/ops" lang="en" xml:lang="en">
<head><title>Chapter One</title><link rel="stylesheet" href="../style.css"/></head>
<body>
<h1>Chapter One</h1>
<p>Hello <b>bold</b> world, you have {count} items and %1$s left.</p>
<p><img src="../img/cover.png" alt="A red cover"/></p>
</body>
</html>
"""
    ch2 = xhtml("Chapter Two", "<p>Second chapter text.</p>").replace("<!DOCTYPE html>\n", "")
    entries = [("text/ch1.xhtml", "Start here"), ("text/chapter%202.xhtml", "Next steps")]
    members = [
        ("META-INF/container.xml", CONTAINER),
        ("OEBPS/content.opf", opf("3.0", "The Little Guide", "en", [
            '<item id="c2" href="text/chapter%202.xhtml" media-type="application/xhtml+xml"/>',
            '<item id="c1" href="text/ch1.xhtml" media-type="application/xhtml+xml"/>',
            '<item id="css" href="style.css" media-type="text/css"/>',
            '<item id="img" href="img/cover.png" media-type="image/png"/>',
        ], ["c1", "c2"], "A short book about &amp; for testing.")),
        ("OEBPS/nav.xhtml", nav(entries)),
        ("OEBPS/toc.ncx", ncx("The Little Guide", entries)),
        ("OEBPS/text/ch1.xhtml", ch1),
        ("OEBPS/text/chapter 2.xhtml", ch2),
        ("OEBPS/style.css", "body { font-family: 'F'; }\n"),
        ("OEBPS/img/cover.png", b"\x89PNG\r\n\x1a\n" + b"\x00" * 64),
    ]
    write(os.path.join(out, "guide.epub"), members)


def legacy2(out):
    ch_u16 = xhtml("Quinze minutes", "<p>Preheat the oven to 220 degrees.</p><p>Fold the dough twice.</p>").replace(
        'encoding="UTF-8"', 'encoding="UTF-16"').encode("utf-16")
    ch_1252 = xhtml("Café notes", "<p>The café serves crème brûlée daily.</p>",
                    decl='<?xml version="1.0" encoding="windows-1252"?>\n').encode("cp1252")
    ch_plain = xhtml("Welcome", "<h1>Welcome to the bakery</h1><p>We open at six.</p>")
    entries = [("c1.xhtml", "Welcome"), ("c2.xhtml", "Quinze minutes"), ("c3.xhtml", "Café notes")]
    members = [
        ("META-INF/container.xml", CONTAINER),
        ("OEBPS/content.opf", b"\xef\xbb\xbf" + opf("2.0", "Bakery Notes", "en", [
            '<item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/>',
            '<item id="c2" href="c2.xhtml" media-type="application/xhtml+xml"/>',
            '<item id="c3" href="c3.xhtml" media-type="application/xhtml+xml"/>',
        ], ["c1", "c2", "c3"], "Notes from the ovens.", nav=False).encode("utf-8")),
        ("OEBPS/toc.ncx", ncx("Bakery Notes", entries)),
        ("OEBPS/c1.xhtml", ch_plain),
        ("OEBPS/c2.xhtml", ch_u16),
        ("OEBPS/c3.xhtml", ch_1252),
    ]
    write(os.path.join(out, "legacy2.epub"), members)


LANGS = [
    ("en", "The quick brown fox."), ("es", "El veloz zorro marrón."), ("fr", "Le renard brun rapide."),
    ("it", "La rapida volpe marrone."), ("de", "Der schnelle braune Fuchs."), ("nl", "De snelle bruine vos."),
    ("pt", "A rápida raposa marrom."), ("ja", "素早い茶色の狐。"),
    ("zh", "敏捷的棕色狐狸。"), ("ko", "빠른 갈색 여우."),
    ("ar", "الثعلب البني السريع."),
]


def languages(out):
    manifest, spine, entries, members = [], [], [], []
    for i, (lg, t) in enumerate(LANGS, 1):
        dirattr = ' dir="rtl"' if lg == "ar" else ""
        body = f'<h1>{t}</h1><p{dirattr}>{t} <em>{t}</em></p>'
        members.append((f"OEBPS/l{i}.xhtml", xhtml(t, body, lang=lg)))
        manifest.append(f'<item id="l{i}" href="l{i}.xhtml" media-type="application/xhtml+xml"/>')
        spine.append(f"l{i}")
        entries.append((f"l{i}.xhtml", t))
    members = [
        ("META-INF/container.xml", CONTAINER),
        ("OEBPS/content.opf", opf("3.0", "Eleven languages", "en", manifest, spine, "One fox, eleven ways.")),
        ("OEBPS/nav.xhtml", nav(entries)),
        ("OEBPS/toc.ncx", ncx("Eleven languages", entries)),
    ] + members
    write(os.path.join(out, "languages.epub"), members)


def rich(out):
    body = """<h1>Rich <i>markup</i></h1>
<p>Fish &amp; chips, caf&#233; &#x2014; &lt;tags&gt; and <code>do_not_translate()</code> inline.</p>
<ul><li>First item</li><li>Second <b>bold</b> item</li><li><a href="https://example.com/" title="Visit the site">Linked item</a></li></ul>
<table><tr><th>Loaf</th><th>Price</th></tr><tr><td>Rye</td><td>4.50</td></tr></table>
<p translate="no">Lumen Bakery(R) stays.</p>
<p>Line one<br/>line two with <span class="x">a span</span> and <a href="#n1" aria-label="Go to the note">a note</a>.</p>
<p><img src="a.png" alt="A loaf of rye bread" title="Fresh rye"/><img src="b.png" alt="12345"/></p>
<pre>keep   this
as is</pre>
<blockquote><p>Quoted text in a block.</p></blockquote>
<p id="n1">Footnote text.</p>"""
    entries = [("c1.xhtml", "Rich markup")]
    members = [
        ("META-INF/container.xml", CONTAINER),
        ("OEBPS/content.opf", opf("3.0", "Rich Book", "en", [
            '<item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/>',
            '<item id="font" href="f.woff" media-type="font/woff"/>'], ["c1"], "Markup of every kind.")),
        ("OEBPS/nav.xhtml", nav(entries)),
        ("OEBPS/toc.ncx", ncx("Rich Book", entries)),
        ("OEBPS/c1.xhtml", xhtml("Rich markup", body)),
        ("OEBPS/f.woff", bytes(range(256))),
    ]
    write(os.path.join(out, "rich.epub"), members)


if __name__ == "__main__":
    out = sys.argv[1]
    os.makedirs(out, exist_ok=True)
    guide(out)
    legacy2(out)
    languages(out)
    rich(out)
    print("wrote", sorted(os.listdir(out)))

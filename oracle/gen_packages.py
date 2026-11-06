"""Builds the made-up CAT packages in `corpus/mqxlz/` and `corpus/sdlppx/` (stdlib only).

    python gen_packages.py

Both formats are zips of XLIFF files, so they are written here with fixed time stamps (the output is byte for byte
the same on every run). Every company, product and sentence is invented (Lumen Bakery); the XML shapes follow what
memoQ and Trados Studio write (the namespaces, `mq:` and `sdl:` attributes, segment definitions), as the Localization
OS filters read them. A `.sdlppx` holds exactly one target SDLXLIFF here, because LocOS opens a package with several
targets only through its project upload (Bilingua lists every target).
"""

import io
import zipfile
from pathlib import Path

OUT = Path(__file__).resolve().parent / "corpus"
STAMP = (2026, 11, 6, 9, 0, 0)


def zip_of(members, comment=b""):
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as z:
        for name, body in members:
            zi = zipfile.ZipInfo(name, STAMP)
            zi.compress_type = zipfile.ZIP_DEFLATED
            zi.external_attr = 0o644 << 16
            z.writestr(zi, body.encode("utf-8") if isinstance(body, str) else body)
        z.comment = comment
    return buf.getvalue()


def mqxliff(original, src, tgt, rows):
    """rows: (id, status, source xml, target xml or None, locked)."""
    tus = []
    for i, status, s, t, locked in rows:
        lock = ' mq:locked="locked"' if locked else ""
        tus.append(
            f'      <trans-unit id="{i}" mq:status="{status}" mq:segmentguid="7e1d0c00-0000-4000-8000-{i:012d}"{lock} xml:space="preserve">\n'
            f'        <source xml:space="preserve">{s}</source>\n'
            + (f'        <target xml:space="preserve">{t}</target>\n' if t is not None else "")
            + "      </trans-unit>"
        )
    return (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        '<xliff xmlns="urn:oasis:names:tc:xliff:document:1.2" xmlns:mq="MQXliff" version="1.2">\n'
        f'  <file original="{original}" source-language="{src}" target-language="{tgt}" datatype="x-memoq">\n'
        "    <body>\n" + "\n".join(tus) + "\n    </body>\n  </file>\n</xliff>\n"
    )


def sdlxliff(original, src, tgt, rows):
    """rows: (id, source xml, target xml or None, seg attributes). One segment per unit."""
    tus = []
    for n, (i, s, t, attrs) in enumerate(rows, 1):
        seg_src = f'<mrk mtype="seg" mid="{n}">{s}</mrk>'
        tgt_xml = f'<target><mrk mtype="seg" mid="{n}">{t}</mrk></target>' if t is not None else ""
        tus.append(
            f'    <trans-unit id="{i}"><source>{s}</source><seg-source>{seg_src}</seg-source>{tgt_xml}'
            f'<sdl:seg-defs><sdl:seg id="{n}"{attrs}/></sdl:seg-defs></trans-unit>'
        )
    bs = chr(92)
    return (
        '<?xml version="1.0" encoding="utf-8"?><xliff xmlns:sdl="http://sdl.com/FileTypes/SdlXliff/1.0" '
        'xmlns="urn:oasis:names:tc:xliff:document:1.2" version="1.2" sdl:version="1.0">\n'
        f'<file original="C:{bs}Projects{bs}lumen{bs}{src}{bs}{original}" datatype="x-sdlfilterframework2" source-language="{src}" target-language="{tgt}">\n'
        '  <header>\n    <file-info xmlns="http://sdl.com/FileTypes/SdlXliff/1.0"><value key="SDL:FileId">5c0de000-1111-4222-8333-944455556666</value></file-info>\n'
        '    <tag-defs><tag id="pt1"><bpt name="b" word-end="false" can-hide="true">&lt;b&gt;</bpt><ept name="b" word-end="false">&lt;/b&gt;</ept><fmt id="1"/></tag>'
        '<tag id="ph2"><ph name="br" word-end="false">&lt;br/&gt;</ph></tag></tag-defs>\n  </header>\n  <body>\n'
        + "\n".join(tus) + "\n  </body>\n</file>\n</xliff>\n"
    )


def sdlproj(name, guid, src, tgt):
    return (
        '<?xml version="1.0" encoding="utf-8"?>\n'
        '<PackageProject xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:xsd="http://www.w3.org/2001/XMLSchema" '
        f'Guid="{guid}" PackageGuid="5c0de000-aaaa-4bbb-8ccc-{guid[-12:]}" PackageName="{name}-2026-11-06" PackageType="ProjectPackage" '
        f'PackageCreatedAt="2026-11-06T09:00:00Z" PackageCreatedBy="PM" Comment="" Version="4.0.0.0" Name="{name}">\n'
        "  <LanguageDirections>\n"
        f'    <LanguageDirection Guid="11111111-2222-4333-8444-555555555555" SettingsBundleGuid="aaaaaaaa-0000-4000-8000-000000000001" TargetLanguageCode="{tgt}" SourceLanguageCode="{src}" />\n'
        "  </LanguageDirections>\n</PackageProject>\n"
    )


def w(rel, data):
    p = OUT / rel
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_bytes(data)


def main():
    # memoQ: one XLIFF, and a package with three (ar, ja, zh targets are one file each) plus a non-XLIFF member.
    de = mqxliff("menu.docx", "en-US", "de-DE", [
        (1, "NotStarted", 'Press <bpt id="1" ctype="bold">{}</bpt>Order<ept id="1">{}</ept> to start.', "", False),
        (2, "ManuallyConfirmed", "Fresh bread is ready at six.", "Frisches Brot ist um sechs fertig.", False),
        (3, "PartiallyEdited", "Closed on Sundays.", "Sonntags geschlossen.", True),
        (4, "MachineTranslated", 'Use {0} loaves &amp; <ph id="2">&lt;br/&gt;</ph>rolls.', "Entwurf.", False),
        (5, "NotStarted", "Gluten free options are marked.", None, False),
    ])
    w("mqxlz/memoq_demo.mqxlz", zip_of([("menu.docx.mqxliff", de)]))
    ar = mqxliff("hours.docx", "en-US", "ar-SA", [
        (1, "NotStarted", "Welcome to Lumen Bakery.", "", False),
        (2, "NotStarted", "Open every day from six in the morning.", None, False)])
    ja = mqxliff("hours.docx", "en-US", "ja-JP", [
        (1, "ManuallyConfirmed", "Welcome to Lumen Bakery.", "ルーメン・ベーカリーへようこそ。", False),
        (2, "NotStarted", "Open every day from six in the morning.", "", False)])
    zh = mqxliff("hours.docx", "en-US", "zh-CN", [
        (1, "NotStarted", "Welcome to Lumen Bakery.", "", False),
        (2, "NotStarted", "Open every day from six in the morning.", "", False)])
    w("mqxlz/memoq_languages.mqxlz", zip_of([
        ("hours.ar.mqxliff", ar), ("hours.ja.mqxliff", ja), ("hours.zh.mqxliff", zh), ("readme.txt", "Lumen Bakery handoff\n")]))

    # Trados: one target SDLXLIFF per package, with the source file and a report beside it.
    de_rows = [
        ("lum-0001", 'Open the <g id="pt1">Orders</g> page. Then click Save.', None, ""),
        ("lum-0002", 'Line one<x id="ph2"/>line two &amp; more.', None, ""),
        ("lum-0003", "Locked legal notice.", "Gesperrter Hinweis.", ' locked="true" conf="Translated"'),
        ("lum-0004", "Approved tagline.", "Genehmigter Slogan.", ' conf="ApprovedTranslation" origin="interactive"'),
        ("lum-0005", "Gluten free options are marked.", None, ""),
    ]
    w("sdlppx/trados_demo.sdlppx", zip_of([
        ("lumen.sdlproj", sdlproj("lumen", "5c0de000-0000-4000-8000-0123456789ab", "en-US", "de-DE")),
        ("en-US/menu.docx", "source placeholder, not a real DOCX"),
        ("de-DE/menu.docx.sdlxliff", sdlxliff("menu.docx", "en-US", "de-DE", de_rows)),
        ("Reports/analysis.txt", "Analysis report\n"),
    ]))
    ja_rows = [
        ("lum-0001", "Welcome to Lumen Bakery.", "ルーメン・ベーカリーへようこそ。", ' conf="Translated"'),
        ("lum-0002", "Fresh bread is ready at six every morning.", None, ""),
        ("lum-0003", 'Order from the <g id="pt1">menu</g> today.', None, ""),
    ]
    w("sdlppx/trados_ja.sdlppx", zip_of([
        ("lumen-ja.sdlproj", sdlproj("lumen-ja", "5c0de000-0000-4000-8000-0123456789cd", "en-US", "ja-JP")),
        ("ja-JP/hours.docx.sdlxliff", sdlxliff("hours.docx", "en-US", "ja-JP", ja_rows)),
    ]))


main()

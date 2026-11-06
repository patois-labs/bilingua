"""Writes the hostile PPTX decks of the LocOS-profile goldens (Track D, the pptx port) and their extra mappings.

    python oracle/gen_locos_pptx.py bilingua/tests/locos/pptx

then oracle/locos_golden.py pptx (with the LocOS venv) writes their goldens."""

import json
import sys
import zipfile

A = "http://schemas.openxmlformats.org/drawingml/2006/main"
P = "http://schemas.openxmlformats.org/presentationml/2006/main"
R = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
RT = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/"
PR = "http://schemas.openxmlformats.org/package/2006/relationships"
DECL = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n"

CT = DECL + (
    '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
    '<Default Extension="xml" ContentType="application/xml"/>'
    '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
    '<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>'
    "</Types>"
)


def rels(items):
    out = DECL + f'<Relationships xmlns="{PR}">'
    for rid, typ, target, *ext in items:
        t = typ if typ.startswith("http") else RT + typ
        mode = ' TargetMode="External"' if ext else ""
        out += f'<Relationship Id="{rid}" Type="{t}" Target="{target}"{mode}/>'
    return out + "</Relationships>"


def pres(ids):
    s = "".join(f'<p:sldId id="{256 + i}" r:id="{rid}"/>' for i, rid in enumerate(ids))
    return DECL + f'<p:presentation xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"><p:sldIdLst>{s}</p:sldIdLst></p:presentation>'


def write(path, members):
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as z:
        for name, data in members:
            z.writestr(zipfile.ZipInfo(name, (2026, 1, 1, 0, 0, 0)), data, zipfile.ZIP_DEFLATED)


def deck(slides, extra=(), pres_ids=None, pres_rels=None):
    members = [("[Content_Types].xml", CT), ("_rels/.rels", rels([("rId1", "officeDocument", "ppt/presentation.xml")]))]
    ids = pres_ids or [f"rId{i + 1}" for i in range(len(slides))]
    members.append(("ppt/presentation.xml", pres(ids)))
    members.append(
        ("ppt/_rels/presentation.xml.rels", rels(pres_rels or [(f"rId{i + 1}", "slide", f"slides/slide{i + 1}.xml") for i in range(len(slides))]))
    )
    for i, (xml, srels) in enumerate(slides):
        members.append((f"ppt/slides/slide{i + 1}.xml", xml))
        if srels is not None:
            members.append((f"ppt/slides/_rels/slide{i + 1}.xml.rels", rels(srels)))
    members.extend(extra)
    return members


def sld(body, attrs=""):
    return DECL + (
        f'<p:sld xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"{attrs}><p:cSld><p:spTree>'
        '<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>'
        f"{body}</p:spTree></p:cSld></p:sld>"
    )


def sp(paras, cid=2, descr=None, title=None):
    d = f' descr="{descr}"' if descr is not None else ""
    t = f' title="{title}"' if title is not None else ""
    return (
        f'<p:sp><p:nvSpPr><p:cNvPr id="{cid}" name="S{cid}"{d}{t}/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr/>'
        f"<p:txBody><a:bodyPr/><a:lstStyle/>{paras}</p:txBody></p:sp>"
    )


def main(out):
    # 1. white space tails, a run without rPr, unknown children between runs, a comment in a paragraph, a field
    #    only paragraph, a namespace declared on a run, literal tokens in the source, a hidden slide by "false",
    #    duplicate and dangling sldIds, punctuation-only and padded alt text, a merged-by-vMerge cell.
    p1 = (
        "\n    <a:p>\n      <a:pPr algn=\"ctr\"/>\n      <a:r>\n        <a:rPr lang=\"en-US\" b=\"1\"/>\n        <a:t>Bold start</a:t>\n      </a:r>\n"
        "      <a:r><a:t> plain middle</a:t></a:r>\n      <a:extLst/>\n      <a:br/>\n"
        "      <a:r xmlns:a14=\"http://schemas.microsoft.com/office/drawing/2010/main\"><a:rPr lang=\"en-US\"><a:extLst><a:ext uri=\"x\"><a14:hiddenFill/></a:ext></a:extLst></a:rPr><a:t>ns run</a:t></a:r>\n"
        "      <a:endParaRPr lang=\"en-US\"/>\n    </a:p>\n"
        "    <a:p><a:r><a:rPr lang=\"en-US\"/><a:t>Literal ⟦q5⟧ token</a:t></a:r><!-- note --><a:r><a:rPr i=\"1\"/><a:t>after comment</a:t></a:r></a:p>\n"
        "    <a:p><a:fld id=\"{1}\" type=\"slidenum\"><a:t>3</a:t></a:fld></a:p>\n"
        "    <a:p><a:r><a:rPr lang=\"en-US\"/><a:t>   </a:t></a:r></a:p>\n"
        "    <a:p><a:r><a:rPr/><a:t></a:t></a:r><a:r><a:rPr/><a:t>Two</a:t></a:r></a:p>\n"
    )
    tbl = (
        '<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="7" name="T" descr="  Padded alt  "/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm/>'
        f'<a:graphic><a:graphicData uri="{A}/table"><a:tbl><a:tr h="1"><a:tc><a:txBody><a:bodyPr/><a:p><a:r><a:t>Top</a:t></a:r></a:p></a:txBody></a:tc></a:tr>'
        '<a:tr h="1"><a:tc vMerge="true"><a:txBody><a:bodyPr/><a:p><a:r><a:t>merged away</a:t></a:r></a:p></a:txBody></a:tc></a:tr></a:tbl></a:graphicData></a:graphic></p:graphicFrame>'
    )
    s1 = sld(sp(p1, 2, descr="--!!--", title="Shape title") + tbl)
    s2 = sld(sp("<a:p><a:r><a:rPr lang=\"en-US\"/><a:t>Hidden by false</a:t></a:r></a:p>"), ' show="false"')
    write(
        f"{out}/hostile_tails.pptx",
        deck(
            [(s1, []), (s2, [])],
            pres_ids=["rId1", "rId9", "rId1", "rId2", "rId3"],
            pres_rels=[("rId1", "slide", "slides/slide1.xml"), ("rId2", "slide", "slides/slide2.xml"), ("rId3", "slide", "../../outside.xml"), ("rId9", "slide", "https://example.com/x.xml", "ext")],
        ),
    )

    # 2. DrawingML as the default namespace of the text body (new elements are written without a prefix), an
    #    RTL target on paragraphs that have no pPr and runs that have no rPr.
    body = (
        f'<p:sp><p:nvSpPr><p:cNvPr id="2" name="D"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr/>'
        f'<p:txBody xmlns="{A}"><bodyPr/><p><r><t>Default one</t></r><br/><r><t>two</t></r></p><p><r><t>Single</t></r><endParaRPr/></p></p:txBody></p:sp>'
    )
    write(f"{out}/hostile_default_ns.pptx", deck([(sld(body), [])]))

    # 3. Notes shared by two slides, a chart referenced twice, a slide whose rels name a notes part that is not in
    #    the package, two notes rels (only the first counts), a SmartArt drawing whose cached text differs.
    C = "http://schemas.openxmlformats.org/drawingml/2006/chart"
    DGM = "http://schemas.openxmlformats.org/drawingml/2006/diagram"
    DSP = "http://schemas.microsoft.com/office/drawing/2008/diagram"
    chartref = f'<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="9" name="C"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm/><a:graphic><a:graphicData uri="{C}"><c:chart xmlns:c="{C}" r:id="rId5"/></a:graphicData></a:graphic></p:graphicFrame>'
    dgmref = f'<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="8" name="G"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm/><a:graphic><a:graphicData uri="{DGM}"><dgm:relIds xmlns:dgm="{DGM}" r:dm="rId3"/></a:graphicData></a:graphic></p:graphicFrame>'
    sa = sld(sp("<a:p><a:r><a:t>Slide A</a:t></a:r></a:p>") + dgmref + chartref + chartref)
    sb = sld(sp("<a:p><a:r><a:t>Slide B</a:t></a:r></a:p>") + chartref)
    notes = DECL + f'<p:notes xmlns:a="{A}" xmlns:r="{R}" xmlns:p="{P}"><p:cSld><p:spTree>{sp("<a:p><a:r><a:t>Shared notes</a:t></a:r></a:p>")}</p:spTree></p:cSld></p:notes>'
    notes2 = notes.replace("Shared notes", "Second notes")
    chart = DECL + f'<c:chartSpace xmlns:c="{C}" xmlns:a="{A}"><c:chart><c:title><c:tx><c:rich><a:bodyPr/><a:p><a:r><a:t>Chart title</a:t></a:r></a:p></c:rich></c:tx></c:title><c:txPr><a:bodyPr/><a:p><a:pPr/><a:endParaRPr/></a:p></c:txPr></c:chart></c:chartSpace>'
    data = DECL + (
        f'<dgm:dataModel xmlns:dgm="{DGM}" xmlns:a="{A}"><dgm:ptLst>'
        '<dgm:pt modelId="{1}"><dgm:t><a:bodyPr/><a:p><a:r><a:t>Same</a:t></a:r><a:r><a:rPr b="1"/><a:t> two</a:t></a:r></a:p></dgm:t></dgm:pt>'
        '<dgm:pt modelId="{2}"><dgm:t><a:bodyPr/><a:p><a:r><a:t>Data text</a:t></a:r></a:p></dgm:t></dgm:pt>'
        '<dgm:pt modelId="{3}"><dgm:t><a:bodyPr/><a:p><a:r><a:t>Third</a:t></a:r></a:p></dgm:t></dgm:pt>'
        f'</dgm:ptLst><dgm:extLst><a:ext uri="x"><dsp:dataModelExt xmlns:dsp="{DSP}" relId="rId4"/></a:ext></dgm:extLst></dgm:dataModel>'
    )
    drawing = DECL + (
        f'<dsp:drawing xmlns:dsp="{DSP}" xmlns:a="{A}"><dsp:spTree>'
        '<dsp:sp modelId="{1}"><dsp:txBody><a:bodyPr/><a:p><a:r><a:rPr lang="en-US"/><a:t>Same</a:t></a:r><a:r><a:t> two</a:t></a:r></a:p></dsp:txBody></dsp:sp>'
        '<dsp:sp modelId="{2}"><dsp:txBody><a:bodyPr/><a:p><a:r><a:t>Cached differs</a:t></a:r></a:p></dsp:txBody></dsp:sp>'
        '<dsp:sp modelId="{3}"><dsp:txBody><a:bodyPr/><a:p><a:r><a:t>Third</a:t></a:r></a:p><a:p><a:r><a:t>extra</a:t></a:r></a:p></dsp:txBody></dsp:sp>'
        '</dsp:spTree></dsp:drawing>'
    )
    ra = [("rId2", "notesSlide", "../notesSlides/notesSlide1.xml"), ("rId6", "notesSlide", "../notesSlides/notesSlide2.xml"), ("rId3", "diagramData", "../diagrams/data1.xml"), ("rId4", "http://schemas.microsoft.com/office/2007/relationships/diagramDrawing", "../diagrams/drawing1.xml"), ("rId5", "chart", "../charts/chart1.xml")]
    rb = [("rId1", "notesSlide", "../notesSlides/missing.xml"), ("rId2", "notesSlide", "../notesSlides/notesSlide1.xml"), ("rId5", "chart", "/ppt/charts/./chart1.xml")]
    extra = [
        ("ppt/notesSlides/notesSlide1.xml", notes),
        ("ppt/notesSlides/notesSlide2.xml", notes2),
        ("ppt/charts/chart1.xml", chart),
        ("ppt/diagrams/data1.xml", data),
        ("ppt/diagrams/drawing1.xml", drawing),
    ]
    write(f"{out}/hostile_shared.pptx", deck([(sa, ra), (sb, rb)], extra))

    # extra mappings (oracle/locos_golden.py builds each of them as well): reordered, dropped, unknown and
    # malformed tokens, new lines, control characters, empty targets, Arabic-Indic digits in a token
    O, Z = "⟦", "⟧"
    maps = {
        "hostile_tails.pptx": {
            "0": f"{O}x4{Z}{O}r3{Z}ns{O}/r3{Z} first {O}r1{Z}B{O}/r1{Z}",
            "1": "no tokens\nsecond line\r\nthird",
            "4": "",
            "5": f"bad {O}x9{Z} {O}r7{Z} {O}/x4{Z} {O}g1{Z} {O}r١{Z}one{O}/r1{Z} ctrl\x01\x0bend",
            "6": f"{O}x1{Z} {O}x2{Z}",
            "7": "Title & <\"'>",
            "8": "",
        },
        "hostile_shared.pptx": {"1": f"{O}r2{Z}Two{O}/r2{Z}{O}r1{Z}One{O}/r1{Z}", "2": "Data", "3": "Drei", "4": "Titel", "6": "Notizen"},
        "hostile_default_ns.pptx": {"0": f"{O}r2{Z}zwei{O}/r2{Z}\n{O}r1{Z}eins{O}/r1{Z}", "1": "Eins"},
    }
    for name, m in maps.items():
        builds = [{"name": f"hostile{suffix}", "locale": loc, "map": m} for suffix, loc in (("", None), ("-ar-SA", "ar-SA"), ("-de-DE", "de-DE"))]
        with open(f"{out}/{name}.maps.json", "w", encoding="utf-8", newline="\n") as fh:
            json.dump(builds, fh, ensure_ascii=False, indent=1)
            fh.write("\n")


if __name__ == "__main__":
    main(sys.argv[1])

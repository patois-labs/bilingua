"""Writes the XLSX column-mode and comment-unit goldens: run with the Localization OS venv Python, read-only.

    <local-tms>/.venv/Scripts/python.exe -I oracle/locos_xlsx_modes.py

LocOS's XLSX filter has two modes the generic golden harness (`locos_golden.py`, which calls `parse(data)` with no
options) does not reach: column mode (`columns=`, target columns) and comment units (`comments=True`). This script
builds a few hand-written workbooks, lets LocOS parse and build them under those options, and writes, for each case
into `bilingua-xlsx/tests/data/modes/`:

    <case>.xlsx                 the input workbook
    <case>.json                 {"parse": {...}, "units": [{"p_index", "text", "style", "note"}], "error": bool,
                                 "builds": [{"name", "locale", "map", "file"}]}
    <case>.<build>.xlsx         what LocOS builds

`bilingua-xlsx/src/locos/tests_modes.rs` extracts the same workbooks, sets the same targets and compares. Nothing in
the LocOS tree is written; its data dir is a throwaway.
"""

import io
import json
import os
import sys
import tempfile
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(os.path.dirname(HERE), "bilingua-xlsx", "tests", "data", "modes")
LOCOS = os.environ.get("LOCOS_DIR") or r"C:\Users\brent\code\local-tms"
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-xlsx-modes-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

SML = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
RNS = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
PKG = "http://schemas.openxmlformats.org/package/2006/relationships"
DECL = '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\n'
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/"
TC_REL = "http://schemas.microsoft.com/office/2017/10/relationships/threadedComment"
TC_NS = "http://schemas.microsoft.com/office/spreadsheetml/2018/threadedcomments"


def rels(items):
    body = "".join(f'<Relationship Id="{i}" Type="{t}" Target="{g}"/>' for i, t, g in items)
    return f'{DECL}<Relationships xmlns="{PKG}">{body}</Relationships>'


def workbook(sheets, sst=None, extra=None, calc_chain=False, core=True):
    """sheets: [(name, xml body, [(rid, rel type, target)])]; returns the package bytes."""
    files = {}
    ct = [
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>',
        '<Default Extension="xml" ContentType="application/xml"/>',
        '<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>',
    ]
    sheet_list, wrels = "", []
    for n, (name, body, srels) in enumerate(sheets, 1):
        sheet_list += f'<sheet name="{name}" sheetId="{n}" r:id="rId{n}"/>'
        wrels.append((f"rId{n}", REL + "worksheet", f"worksheets/sheet{n}.xml"))
        files[f"xl/worksheets/sheet{n}.xml"] = f'{DECL}<worksheet xmlns="{SML}" xmlns:r="{RNS}">{body}</worksheet>'
        if srels:
            files[f"xl/worksheets/_rels/sheet{n}.xml.rels"] = rels(srels)
        ct.append(f'<Override PartName="/xl/worksheets/sheet{n}.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>')
    if sst is not None:
        items = "".join(f"<si><t>{s}</t></si>" for s in sst)
        files["xl/sharedStrings.xml"] = f'{DECL}<sst xmlns="{SML}" count="{len(sst)}" uniqueCount="{len(sst)}">{items}</sst>'
        wrels.append(("rIdSst", REL + "sharedStrings", "sharedStrings.xml"))
        ct.append('<Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/>')
    if calc_chain:
        files["xl/calcChain.xml"] = f'{DECL}<calcChain xmlns="{SML}"><c r="B2" i="1"/></calcChain>'
        wrels.append(("rIdCalc", REL + "calcChain", "calcChain.xml"))
        ct.append('<Override PartName="/xl/calcChain.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.calcChain+xml"/>')
    files["xl/workbook.xml"] = f'{DECL}<workbook xmlns="{SML}" xmlns:r="{RNS}"><sheets>{sheet_list}</sheets></workbook>'
    files["xl/_rels/workbook.xml.rels"] = rels(wrels)
    root_rels = [("rId1", REL + "officeDocument", "xl/workbook.xml")]
    if core:
        root_rels.append(("rId2", PKG.rsplit("/", 1)[0] + "/metadata/core-properties", "docProps/core.xml"))
        files["docProps/core.xml"] = (
            f'{DECL}<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" '
            'xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Book</dc:title></cp:coreProperties>'
        )
        ct.append('<Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>')
    files["_rels/.rels"] = rels(root_rels)
    for k, v in (extra or {}).items():
        files[k] = v
    files["[Content_Types].xml"] = f'{DECL}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">{"".join(ct)}</Types>'
    order = ["[Content_Types].xml", "_rels/.rels"] + [k for k in files if k not in ("[Content_Types].xml", "_rels/.rels")]
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as zf:
        for k in order:
            zi = zipfile.ZipInfo(k, date_time=(2026, 10, 28, 9, 30, 0))
            zi.compress_type = zipfile.ZIP_DEFLATED
            zf.writestr(zi, files[k])
    return buf.getvalue()


def s(ref, text):
    return f'<c r="{ref}" t="inlineStr"><is><t>{text}</t></is></c>'


def sh(ref, i):
    return f'<c r="{ref}" t="s"><v>{i}</v></c>'


def n(ref, v):
    return f'<c r="{ref}"><v>{v}</v></c>'


def row(r, *cells, attrs=""):
    return f'<row r="{r}"{attrs}>{"".join(cells)}</row>'


def sheet(*rows, dim=None):
    d = f'<dimension ref="{dim}"/>' if dim else ""
    return f"{d}<sheetData>{''.join(rows)}</sheetData>"


# ------------------------------------------------------------------ cases

CASES = []


def case(name, data, parse, builds):
    CASES.append((name, data, parse, builds))


def all_map(units, prefix="DE ", keep=()):
    return {str(u["p_index"]): (u["text"] if u["text"] in keep else prefix + u["text"]) for u in units}


# T1: header row, gaps, shared strings, a filled target kept, a second sheet that is left alone
t1 = workbook(
    [
        (
            "Strings",
            sheet(
                row(1, s("A1", "en-US"), s("B1", "de-DE"), s("C1", "Other")),
                row(2, sh("A2", 0), n("C2", 7)),
                row(4, s("A4", "Bye"), attrs=' spans="1:1"'),
                row(5, s("A5", "12345"), s("B5", "x")),
                row(6, s("A6", "Keep me"), s("B6", "Schon da")),
                row(7, sh("A7", 0), '<c r="B7"/>'),
                row(8, s("A8", "=SUM(A1)"), s("B8", "formula text")),
                dim="A1:C8",
            ),
            [],
        ),
        ("Plain", sheet(row(1, s("A1", "No header here")), row(2, s("A2", "Second line"))), []),
    ],
    sst=["Hello"],
    calc_chain=True,
)
SPEC1 = {"columns": "source=en-US,target.de-DE=de-DE", "source_locale": "en-US", "target_locales": ["de-DE"]}
case(
    "table_basic",
    t1,
    SPEC1,
    [
        ("de", "de-DE", lambda u: all_map(u, keep=("Keep me",))),
        ("stamp-only", "de-DE", lambda u: {}),
        ("rtl", "ar-SA", lambda u: all_map(u, prefix="AR ")),
        ("nolocale", None, lambda u: all_map(u)),
        ("partial", "de-DE", lambda u: {str(x["p_index"]): "Teil " + x["text"] for x in u[:2]}),
    ],
)

# T2: key and context notes by header name, a numeric target column past the last one
t2 = workbook(
    [
        (
            "Sheet1",
            sheet(
                row(1, s("A1", "ID"), s("B1", "Text"), s("C1", "Notes")),
                row(2, s("A2", "k1"), s("B2", "Save"), s("C2", "Button label")),
                row(3, s("A3", "k2"), s("B3", "Cancel &amp; close"), s("C3", "")),
                row(4, s("A4", "k3"), n("B4", 5), s("C4", "numbers are not units")),
                row(5, s("A5", "k4"), s("B5", "Print"), s("C5", "Menu")),
            ),
            [],
        )
    ]
)
case(
    "table_notes",
    t2,
    {"columns": "source=Text,target=4,key=ID,context=Notes,maxlen=3", "source_locale": "en", "target_locales": ["de"]},
    [("de", "de", lambda u: all_map(u, prefix="")) , ("de-x", "de", lambda u: {str(x["p_index"]): x["text"].upper() + " <&>" for x in u})],
)

# T3: formula targets (a shared formula master), a number and a boolean already in the target column
t3 = workbook(
    [
        (
            "Sheet1",
            sheet(
                row(1, s("A1", "en-US"), s("B1", "de-DE")),
                row(2, s("A2", "Hello"), '<c r="B2" t="str"><f t="shared" ref="B2:B4" si="0">UPPER(A2)</f><v>HELLO</v></c>'),
                row(3, s("A3", "Bye"), '<c r="B3" t="str"><f t="shared" si="0"/><v>BYE</v></c>'),
                row(4, s("A4", "Yes"), '<c r="B4" t="str"><f t="shared" si="0"/><v>YES</v></c>'),
                row(5, s("A5", "Number"), n("B5", 42)),
                row(6, s("A6", "Bool"), '<c r="B6" t="b"><v>1</v></c>'),
            ),
            [],
        )
    ],
    calc_chain=True,
)
case(
    "table_formula",
    t3,
    SPEC1,
    [("de", "de-DE", lambda u: all_map(u)), ("one", "de-DE", lambda u: {"1": "Tschuess"})],
)

# T4: roles found from the header names, no named columns
t4 = workbook(
    [
        (
            "Sheet1",
            sheet(
                row(1, s("A1", "Key"), s("B1", "Source"), s("C1", "Translation"), s("D1", "Comment")),
                row(2, s("A2", "a"), s("B2", "One line"), s("D2", "first")),
                row(3, s("A3", "b"), s("B3", "Two lines"), s("C3", "Zwei"), s("D3", "second")),
            ),
            [],
        )
    ]
)
case("table_auto", t4, {"columns": "header=auto", "source_locale": "en", "target_locales": ["de"]}, [("de", "de", lambda u: all_map(u))])

# T5: two target languages
t5 = workbook(
    [
        (
            "Sheet1",
            sheet(
                row(1, s("A1", "en"), s("B1", "de"), s("C1", "fr")),
                row(2, s("A2", "Hello"), s("B2", "Hallo")),
                row(3, s("A3", "Thanks")),
            ),
            [],
        )
    ]
)
case(
    "table_two_targets",
    t5,
    {"columns": "source=en,target.de=de,target.fr=fr", "source_locale": "en", "target_locales": ["de-DE", "fr-FR"]},
    [("fr", "fr-FR", lambda u: all_map(u, prefix="FR ")), ("de", "de-DE", lambda u: all_map(u, prefix="DE "))],
)

# T6: a mapping that fits no sheet
case("table_nomatch", t1, {"columns": "source=Nope", "source_locale": "en", "target_locales": []}, [])

# C1: classic notes (a bold label run, runs that split the text, a number-only note), a threaded comment with a
# reply and the placeholder note Excel writes for it, a second sheet's note
PLACEHOLDER = "[Threaded comment]\nYour version of Excel allows you to read this threaded comment."
comments1 = (
    f'{DECL}<comments xmlns="{SML}"><authors><author>Ann</author><author>tc={{T1}}</author></authors><commentList>'
    '<comment ref="A1" authorId="0"><text><t>12345</t></text></comment>'
    '<comment ref="B2" authorId="0"><text><r><rPr><b/><sz val="9"/></rPr><t>Ann:</t></r>'
    '<r><rPr><sz val="9"/></rPr><t xml:space="preserve">\nCheck the </t></r><r><t>total</t></r><r><t xml:space="preserve"> now.</t></r></text></comment>'
    f'<comment ref="C3" authorId="1"><text><t>{PLACEHOLDER}</t></text></comment>'
    '<comment ref="D4" authorId="0"><text><t xml:space="preserve">  Padded note  </t></text></comment>'
    "</commentList></comments>"
)
threaded1 = (
    f'{DECL}<ThreadedComments xmlns="{TC_NS}">'
    '<threadedComment ref="C3" dT="2026-10-01T10:00:00.00" personId="{AAAA0000-0000-0000-0000-000000000000}" id="{T1}">'
    "<text> Who owns this line? </text></threadedComment>"
    '<threadedComment ref="C3" dT="2026-10-01T11:00:00.00" personId="{AAAA0000-0000-0000-0000-000000000001}" id="{T2}" parentId="{T1}">'
    "<text>Finance does.</text></threadedComment></ThreadedComments>"
)
comments2 = f'{DECL}<comments xmlns="{SML}"><authors><author>Bob</author></authors><commentList><comment ref="a1" authorId="0"><text><t>Second sheet note</t></text></comment></commentList></comments>'
c1 = workbook(
    [
        (
            "Budget",
            sheet(row(1, s("A1", "Quarterly budget")), row(2, n("B2", 42)), row(3, s("C3", "Owner")), row(4, s("D4", "Padded"))),
            [("rId1", REL + "comments", "../comments1.xml"), ("rId2", TC_REL, "../threadedComments/threadedComment1.xml")],
        ),
        ("Other", sheet(row(1, s("A1", "Other sheet"))), [("rId1", REL + "comments", "../comments2.xml")]),
    ],
    extra={
        "xl/comments1.xml": comments1,
        "xl/comments2.xml": comments2,
        "xl/threadedComments/threadedComment1.xml": threaded1,
    },
)
case(
    "comments_basic",
    c1,
    {"comments": True},
    [
        ("de", "de-DE", lambda u: all_map(u)),
        ("comments-only", None, lambda u: {str(x["p_index"]): "DE " + x["text"] for x in u if x["style"] == "comment"}),
        ("stamp-only", "de-DE", lambda u: {}),
        ("cells-only", "de-DE", lambda u: {str(x["p_index"]): "DE " + x["text"] for x in u if x["style"] != "comment"}),
        ("same-text", None, lambda u: {str(x["p_index"]): x["text"] for x in u}),
    ],
)
# the same workbook read without comments: no comment key, comment parts untouched
case("comments_off", c1, {}, [("de", "de-DE", lambda u: all_map(u))])


def main():
    os.makedirs(OUT, exist_ok=True)
    filt = get_filter("xlsx")
    for name, data, parse, builds in CASES:
        with open(os.path.join(OUT, name + ".xlsx"), "wb") as fh:
            fh.write(data)
        info = {"parse": parse, "units": [], "error": False, "builds": []}
        try:
            res = filt.parse(data, **parse)
        except Exception as exc:  # recorded: the Rust side expects an error too
            info["error"] = True
            info["message"] = f"{type(exc).__name__}: {exc}"
            res = None
        if res is not None:
            sk = {u["p_index"]: u for u in res.skeleton["units"]}
            info["units"] = [
                {"p_index": u.p_index, "text": u.text, "style": u.style or "", "note": sk[u.p_index].get("note", "")}
                for u in res.units
            ]
            table = res.skeleton.get("table")
            for bname, locale, fn in builds:
                mapping = fn(info["units"])
                if table:
                    out = filt.build_localized(data, mapping, locale, table=table)
                else:
                    out = filt.build_localized(data, mapping, locale)
                fname = f"{name}.{bname}.xlsx"
                with open(os.path.join(OUT, fname), "wb") as fh:
                    fh.write(out)
                info["builds"].append({"name": bname, "locale": locale, "map": mapping, "file": fname})
        with open(os.path.join(OUT, name + ".json"), "w", encoding="utf-8") as fh:
            json.dump(info, fh, ensure_ascii=False, indent=1)
        print(name, "units:", len(info["units"]), "builds:", len(info["builds"]), "error" if info["error"] else "")


if __name__ == "__main__":
    main()

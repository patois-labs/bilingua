"""Writes the Storyline bilingual-table fixtures and their LocOS golden (read-only use of the LocOS filters):

    C:\\Users\\brent\\code\\local-tms\\.venv\\Scripts\\python.exe tests/tools/gen_storyline.py

tests/data/storyline_export.docx   an Articulate Storyline 360 "Export to Word" layout (made up): project table,
                                   scene headings, ID | Original Text | Translation rows, a merged slide label
                                   row, a repeated header row and an empty Translation cell
tests/data/storyline_styleguide.docx  a style guide whose prose is longer than its Source | Translation term
                                   table: translated as an ordinary document
tests/data/storyline_golden.json   the unit texts LocOS gives for both
"""

import json
import os
import sys
import tempfile
from pathlib import Path

import docx

HERE = Path(__file__).resolve().parent
DATA = HERE.parent / "data"
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="storyline-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms"))
from app.filters.base import get_filter  # noqa: E402


def export():
    d = docx.Document()
    d.add_paragraph("Translation export", style="Title")
    info = d.add_table(rows=2, cols=2)
    info.cell(0, 0).text, info.cell(0, 1).text = "Project", "Lumen Bakery onboarding"
    info.cell(1, 0).text, info.cell(1, 1).text = "Language", "English"
    d.add_paragraph("Scene 1: Welcome", style="Heading 1")
    t = d.add_table(rows=1, cols=3)
    for c, h in zip(t.rows[0].cells, ["ID", "Original Text", "Translation"]):
        c.text = h
    rows = [
        ("6Xyz1", "Welcome to the bakery.", "Welcome to the bakery."),
        (None, "Slide 1.1 Opening", None),
        ("6Xyz2", "Wash your hands before every shift.", "Wash your hands before every shift."),
        ("ID", "Original Text", "Translation"),
        ("6Xyz3", "Ovens are hot.\nUse the gloves.", "Ovens are hot.\nUse the gloves."),
        ("6Xyz4", "This row was never filled.", ""),
    ]
    for rid, src, tr in rows:
        r = t.add_row().cells
        if rid is None:
            r[0].merge(r[2]).text = src
            continue
        r[0].text, r[1].text = rid, src
        r[2].text = ""
        if tr:
            lines = tr.split("\n")
            r[2].paragraphs[0].text = lines[0]
            for line in lines[1:]:
                r[2].add_paragraph(line)
    d.add_paragraph("Scene 2: Closing", style="Heading 1")
    t2 = d.add_table(rows=2, cols=3)
    for c, h in zip(t2.rows[0].cells, ["ID", "Original Text", "Translation:"]):
        c.text = h
    t2.rows[1].cells[0].text, t2.rows[1].cells[1].text, t2.rows[1].cells[2].text = "6Abc1", "Thank you!", "Thank you!"
    return d


def styleguide():
    d = docx.Document()
    d.add_paragraph("Style guide", style="Title")
    for i in range(6):
        d.add_paragraph(f"Paragraph {i} of the style guide explains tone, spelling and the words we never use in customer mail.")
    t = d.add_table(rows=3, cols=2)
    for (a, b), row in zip([("Source", "Translation"), ("loaf", "pain"), ("crust", "croute")], t.rows):
        row.cells[0].text, row.cells[1].text = a, b
    return d


def main():
    f = get_filter("docx")
    out = {}
    for name, make in [("storyline_export.docx", export), ("storyline_styleguide.docx", styleguide)]:
        path = DATA / name
        make().save(path)
        data = path.read_bytes()
        pr = f.parse(data)
        warn = [w.get("code") for w in pr.skeleton.get("file_warnings", [])]
        out[name] = {"units": [u.text for u in pr.units], "warnings": warn}
    (DATA / "storyline_golden.json").write_text(json.dumps(out, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    print(json.dumps(out, ensure_ascii=False, indent=1))


main()

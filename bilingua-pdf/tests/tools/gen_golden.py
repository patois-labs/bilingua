"""Records what Localization OS makes of every fixture: `tests/data/locos_golden.json`.

    C:\\Users\\brent\\code\\local-tms\\.venv\\Scripts\\python.exe -I gen_golden.py DATA_DIR

Per file: the LocOS units (`PdfFilter.parse`, OCR stubbed so a scanned page gives the marker `<OCR page N>`), the
raw PDFium text of every page (`get_text_range`, the input LocOS splits), which pages LocOS sends to OCR, the
page count, or the exception LocOS raises. LocOS is imported read-only and its data directory is a throwaway.
"""

import json
import os
import sys
import tempfile

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="bilingua-pdf-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

import pypdfium2 as pdfium  # noqa: E402

from app.filters import pdf_filter  # noqa: E402
from app.services import ocr  # noqa: E402

DATA = sys.argv[1]
calls = []


def fake_ocr(png, provider=None):
    calls.append(len(png))
    return f"<OCR page {len(calls)}>"


ocr.ocr_image = fake_ocr
f = pdf_filter.PdfFilter()
out = {}
for name in sorted(os.listdir(DATA)):
    if not name.lower().endswith(".pdf"):
        continue
    data = open(os.path.join(DATA, name), "rb").read()
    rec = {}
    calls.clear()
    try:
        res = f.parse(data)
        rec["units"] = [u.text for u in res.units]
        rec["ocr_pages"] = [m["page"] for m in res.skeleton["pages"] if m["ocr"]]
        doc = pdfium.PdfDocument(data)
        pages = []
        for i in range(len(doc)):
            page = doc[i]
            tp = page.get_textpage()
            pages.append(tp.get_text_range())
            tp.close()
            page.close()
        doc.close()
        rec["pages"] = pages
    except Exception as e:  # noqa: BLE001
        rec["error"] = type(e).__name__
        rec["message"] = str(e)[:200]
    out[name] = rec

with open(os.path.join(DATA, "locos_golden.json"), "w", encoding="utf-8", newline="\n") as fh:
    json.dump(out, fh, ensure_ascii=False, indent=1, sort_keys=True)
    fh.write("\n")
print(f"{len(out)} files")

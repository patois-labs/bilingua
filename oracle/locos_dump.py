"""Run with the Localization OS venv Python (read-only use of its filters).

    python locos_dump.py OUT.json FILE [FILE ...]

Writes {file: {"units": [text, ...]} or {"error": ...}} where text is the unit text as LocOS makes it, with
inline-code tokens left in place. Nothing in the LocOS tree is written; the data dir is a throwaway.
"""

import json
import os
import sys
import tempfile

# The Localization OS checkout (the oracle). Default: a `local-tms` folder next to the repository.
LOCOS = os.environ.get("LOCOS_DIR", os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "local-tms"))
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-oracle-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

EXT = {"docm": "docx", "dotx": "docx", "dotm": "docx", "htm": "html", "markdown": "md", "text": "txt"}


def main() -> None:
    out_path, files = sys.argv[1], sys.argv[2:]
    res = {}
    for f in files:
        ext = f.rsplit(".", 1)[-1].lower()
        ext = EXT.get(ext, ext)
        try:
            filt = get_filter(ext)
            if filt is None:
                res[f] = {"error": f"no LocOS filter for {ext}"}
                continue
            with open(f, "rb") as fh:
                data = fh.read()
            pr = filt.parse(data)
            opened = None
            try:
                if ext == "docx":
                    import docx

                    docx.Document(f)
                elif ext in ("xlsx", "xlsm"):
                    import warnings

                    import openpyxl

                    with warnings.catch_warnings():
                        warnings.simplefilter("ignore")
                        openpyxl.load_workbook(f).close()
            except Exception as e:  # noqa: BLE001
                opened = f"{type(e).__name__}: {str(e)[:150]}"
            res[f] = {"filter": filt.fmt, "open_error": opened, "units": [
                {"i": u.p_index, "text": u.text, "style": u.style} for u in pr.units]}
        except Exception as e:  # noqa: BLE001
            res[f] = {"error": f"{type(e).__name__}: {str(e)[:200]}"}
    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(res, fh, ensure_ascii=False)


main()

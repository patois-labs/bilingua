"""Compares Bilingua (examples/dump.rs), LocOS (read only) and Okapi okf_xml on Okapi's own XML / ITS test files.

Run with the LocOS venv Python (it imports app.filters read only; nothing is written in the LocOS tree):

    C:\\Users\\brent\\code\\local-tms\\.venv\\Scripts\\python.exe okapi_compare.py OUT.json [DIR_OR_FILE ...]

Text is normalised as okapi-oracle does: NFC, NBSP to space, white space collapsed and stripped, every inline code
(a LocOS token, or one Okapi code) replaced by one marker. Okapi runs with the default okf_xml configuration; the
documents carry their own ITS rules, and linked rules files (xlink:href) are never fetched by LocOS or Bilingua.
"""

import json
import os
import re
import subprocess
import sys
import tempfile
import unicodedata
from concurrent.futures import ThreadPoolExecutor

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
OKAPI = r"C:\Users\brent\code\okapi-oracle\okapi_dump.py"
RES = r"C:\Users\brent\code\okapi-oracle\resources\okapi-src\okapi\filters\its\src\test\resources"
BL_DUMP = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "..", "target", "release", "examples", "dump.exe")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-xml-okapi-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)
from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

MARK = "\u00a4"
TOKEN = re.compile("\u27e6/?[a-z]\\d+\u27e7")


def norm(s: str) -> str:
    s = unicodedata.normalize("NFC", s).replace("\u00a0", " ")
    return " ".join(s.split())


def locos_units(path):
    try:
        with open(path, "rb") as fh:
            data = fh.read()
        pr = get_filter("xml").parse(data)
        return [norm(TOKEN.sub(MARK, u.text)) for u in pr.units]
    except Exception as e:  # noqa: BLE001
        return {"error": f"{type(e).__name__}: {str(e)[:120]}"}


def bilingua_units(paths):
    out = subprocess.run([BL_DUMP, *paths], capture_output=True, text=True, encoding="utf-8")
    res = {}
    for line in out.stdout.splitlines():
        j = json.loads(line)
        if "error" in j:
            res[j["file"]] = {"error": j["error"][:120]}
        else:
            res[j["file"]] = [norm(TOKEN.sub(MARK, u["text"])) for u in j["units"]]
    return res


def okapi_units(path):
    p = subprocess.run([sys.executable, "-I", OKAPI, path, "--fc", "okf_xml"], capture_output=True, text=True, encoding="utf-8")
    if p.returncode != 0:
        return {"error": (p.stderr or "failed").strip()[-120:]}
    units = []
    for line in p.stdout.splitlines():
        if line.startswith("{"):
            j = json.loads(line)
            if j.get("translate", "yes") != "no" or True:
                units.append(j)
    return [norm(u["text"]) for u in units if u.get("translate") != "no" and norm(u["text"])]


def collect(args):
    files = []
    for a in args or [RES]:
        if os.path.isfile(a):
            files.append(a)
            continue
        for root, _d, names in os.walk(a):
            for n in names:
                low = n.lower()
                if not low.endswith(".xml"):
                    continue
                if "rules" in low and "/expected/" not in root.replace("\\", "/"):
                    continue
                if "expected" in root.replace("\\", "/").split("/"):
                    continue
                files.append(os.path.join(root, n))
    return sorted(files)


def main():
    out_path, rest = sys.argv[1], sys.argv[2:]
    files = collect(rest)
    bl = {}
    for i in range(0, len(files), 40):
        bl.update(bilingua_units(files[i:i + 40]))
    with ThreadPoolExecutor(max_workers=6) as ex:
        ok = list(ex.map(okapi_units, files))
    rows = []
    for f, o in zip(files, ok):
        rows.append({"file": os.path.relpath(f, RES), "bilingua": bl.get(f), "locos": locos_units(f), "okapi": o})
    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(rows, fh, ensure_ascii=False, indent=1)
    n = len(rows)
    same_locos = sum(1 for r in rows if r["bilingua"] == r["locos"])
    same_okapi = sum(1 for r in rows if r["bilingua"] == r["okapi"])
    err_okapi = sum(1 for r in rows if isinstance(r["okapi"], dict))
    print(f"files {n}; Bilingua == LocOS {same_locos}; Bilingua == Okapi {same_okapi}; Okapi errors {err_okapi}")


if __name__ == "__main__":
    main()

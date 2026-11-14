"""Compares Bilingua (examples/dita_dump.rs output), LocOS (read only) and Okapi on Okapi's own DITA and DocBook files.

Run with the LocOS venv Python (it imports app.filters read only; nothing is written in the LocOS tree):

    cargo run -p bilingua-dita --release --example dita_dump -- dump FILES... > bl.jsonl       (on the M5)
    C:\\Users\\brent\\code\\local-tms\\.venv\\Scripts\\python.exe okapi_compare.py bl.jsonl OUT.json

Files: Okapi's `xmlstream` test resources (*.dita, filter okf_xmlstream-dita, the DITA configuration Okapi ships)
and the two DocBook samples in `its` (okf_xml-docbook and plain okf_xml). The Bilingua dump names the files by base
name, so it can be made on another machine. Text is normalised as okapi-oracle does: NFC, NBSP to space, white space
collapsed and stripped, every inline code (a LocOS token, or one Okapi code) replaced by one marker.
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
RES = r"C:\Users\brent\code\okapi-oracle\resources\okapi-src\okapi\filters"
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-dita-okapi-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)
from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

MARK = "\u00a4"
TOKEN = re.compile("\u27e6/?[a-z]\\d+\u27e7")


def norm(s):
    s = unicodedata.normalize("NFC", s).replace("\u00a0", " ")
    return " ".join(s.split())


def files():
    out = []
    xs = os.path.join(RES, "xmlstream", "src", "test", "resources")
    out += [(os.path.join(xs, n), "dita", "okf_xmlstream-dita") for n in sorted(os.listdir(xs)) if n.endswith(".dita")]
    its = os.path.join(RES, "its", "src", "test", "resources")
    for n in ("docbook-emphasis-example.xml", "docbook-footnote-example.xml"):
        out.append((os.path.join(its, n), "docbook", "okf_xml-docbook"))
        out.append((os.path.join(its, n), "docbook", "okf_xml"))
    return out


def locos(path, fmt):
    try:
        with open(path, "rb") as fh:
            return [norm(TOKEN.sub(MARK, u.text)) for u in get_filter(fmt).parse(fh.read()).units]
    except Exception as e:  # noqa: BLE001
        return {"error": f"{type(e).__name__}: {str(e)[:120]}"}


def okapi(path, fc):
    p = subprocess.run([sys.executable, "-I", OKAPI, path, "--fc", fc], capture_output=True, text=True, encoding="utf-8")
    if p.returncode != 0:
        return {"error": (p.stderr or "failed").strip()[-120:]}
    units = [json.loads(line) for line in p.stdout.splitlines() if line.startswith("{")]
    return [norm(u["text"]) for u in units if u.get("translate") != "no" and norm(u["text"])]


def score(a, b):
    """Units of `a` found in `b` (as a multiset), over the larger count."""
    if isinstance(a, dict) or isinstance(b, dict):
        return None
    pool = list(b)
    hit = 0
    for x in a:
        if x in pool:
            pool.remove(x)
            hit += 1
    return hit / max(len(a), len(b), 1)


def main():
    bl_path, out_path = sys.argv[1], sys.argv[2]
    bl = {}
    for line in open(bl_path, encoding="utf-8"):
        j = json.loads(line)
        name = os.path.basename(j["file"])
        bl[name] = {"error": j["error"][:120]} if "error" in j else [norm(TOKEN.sub(MARK, u["text"])) for u in j["units"]]
    fl = files()
    with ThreadPoolExecutor(8) as ex:
        ok = list(ex.map(lambda f: okapi(f[0], f[2]), fl))
    rows = []
    for (path, fmt, fc), o in zip(fl, ok):
        name = os.path.basename(path)
        lo = locos(path, fmt)
        b = bl.get(name, {"error": "not dumped"})
        rows.append({"file": name, "okapi_filter": fc, "bilingua": b, "locos": lo, "okapi": o,
                     "bl_eq_locos": b == lo, "bl_vs_okapi": score(b, o), "locos_vs_okapi": score(lo, o)})
    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(rows, fh, ensure_ascii=False, indent=1)
    for fc in sorted({r["okapi_filter"] for r in rows}):
        rs = [r for r in rows if r["okapi_filter"] == fc]
        sc = [r["bl_vs_okapi"] for r in rs if r["bl_vs_okapi"] is not None]
        nb = sum(len(r["bilingua"]) for r in rs if isinstance(r["bilingua"], list))
        no = sum(len(r["okapi"]) for r in rs if isinstance(r["okapi"], list))
        exact = sum(1 for r in rs if isinstance(r["okapi"], list) and r["bilingua"] == r["okapi"])
        print(f"{fc}: {len(rs)} files, Bilingua = LocOS on {sum(r['bl_eq_locos'] for r in rs)}, units Bilingua {nb} / Okapi {no}, "
              f"identical unit lists {exact}, mean unit match vs Okapi {100 * sum(sc) / max(len(sc), 1):.1f} %")


if __name__ == "__main__":
    main()

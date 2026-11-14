r"""Compares the SVG text units of LocOS (= Bilingua, proven by the golden test) with Okapi okf_xml on the oracle corpus.

    C:\Users\brent\code\local-tms\.venv\Scripts\python.exe okapi_compare.py CORPUS_SVG_DIR

Okapi (C:\Users\brent\code\okapi-oracle, read only) extracts every text node with its default XML rules (no SVG
rules exist in okf_xml); LocOS takes text inside text, tspan and textPath, plus title and desc. Units are compared
as normalised text (NFC, whitespace collapsed, inline-code markers dropped).
"""

import json
import os
import re
import subprocess
import sys
import tempfile
import unicodedata

LOCOS = r"C:\Users\brent\code\local-tms"
OKAPI = r"C:\Users\brent\code\okapi-oracle\okapi_dump.py"
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-okc-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)
cwd = os.getcwd()
os.chdir(LOCOS)
from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

F = get_filter("image")


def norm(t):
    t = unicodedata.normalize("NFC", t.replace("\u00a4", ""))
    return re.sub(r"\s+", " ", t.replace("\u00a0", " ")).strip()


def locos_units(data):
    return [norm(u.text) for u in F._parse_svg(data).units]


def okapi_units(path):
    p = subprocess.run([sys.executable, "-I", OKAPI, path, "--fc", "okf_xml", "--ie", ""], capture_output=True, text=True, encoding="utf-8")
    if p.returncode != 0:
        return None, p.stderr.strip()[:120]
    out = []
    for line in p.stdout.splitlines():
        if line.startswith("{"):
            j = json.loads(line)
            if "text" in j:
                out.append(norm(j["text"]))
    return [u for u in out if u], ""


d = sys.argv[1]
tot = [0, 0, 0, 0]
for name in sorted(os.listdir(d)):
    path = os.path.join(d, name)
    data = open(path, "rb").read()
    try:
        ours = [u for u in locos_units(data) if u]
    except Exception as e:  # noqa: BLE001
        ours = None
        eo = str(e)[:80]
    ok, err = okapi_units(path)
    if ours is None or ok is None:
        print(f"{name:16} LocOS {'ERR ' + eo if ours is None else len(ours)}  Okapi {'ERR ' + err if ok is None else len(ok)}")
        continue
    so, sk = set(ours), set(ok)
    both = so & sk
    only_l, only_o = [u for u in ours if u not in sk], [u for u in ok if u not in so]
    tot[0] += len(ours); tot[1] += len(ok); tot[2] += len([u for u in ours if u in sk]); tot[3] += 1
    print(f"{name:16} LocOS {len(ours):2}  Okapi {len(ok):2}  common {len([u for u in ours if u in sk]):2}  only LocOS {only_l[:3]}  only Okapi {only_o[:4]}")
print(f"files {tot[3]}: LocOS units {tot[0]}, Okapi units {tot[1]}, LocOS units Okapi also has {tot[2]} ({100 * tot[2] / max(tot[0], 1):.0f} %)")

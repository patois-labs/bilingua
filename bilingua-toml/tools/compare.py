"""Compare Bilingua's TOML units with the Localization OS filter on the same files (read-only use of LocOS).

    python compare.py [EXTRA_FILE ...]

Builds and runs the bilingua-toml `dump` example and `oracle/locos_dump.py` (LocOS venv python), then prints one line
per file: unit counts and whether every unit text is identical.
"""
import json, os, subprocess, sys, tempfile

sys.stdout.reconfigure(encoding="utf-8")  # unit texts are not all in the console code page
from pathlib import Path

CORPUS = Path(__file__).resolve().parents[2] / "oracle" / "corpus" / "toml"
HERE = Path(__file__).resolve().parent
WS = HERE.parents[1]  # the bilingua workspace
LOCOS = Path(os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms"))
PY = LOCOS / ".venv" / "Scripts" / "python.exe"
files = sorted(str(p) for p in CORPUS.glob("*.toml")) + sys.argv[1:]
tmp = tempfile.mkdtemp()
a, b = os.path.join(tmp, "bl.json"), os.path.join(tmp, "lo.json")
subprocess.run(["cargo", "run", "-q", "-p", "bilingua-toml", "--example", "dump", "--", a, *files], cwd=WS, check=True)
subprocess.run([str(PY), str(WS / "oracle" / "locos_dump.py"), b, *files], check=True)
bl, lo = json.load(open(a, encoding="utf-8")), json.load(open(b, encoding="utf-8"))
for v in lo.values():
    if "units" in v:
        v["units"] = [u["text"] if isinstance(u, dict) else u for u in v["units"]]
bad = 0
for f in files:
    x, y = bl[f], lo[f]
    if "units" in x and "units" in y:
        same = x["units"] == y["units"]
        print(f"{'same' if same else 'DIFF'}  {Path(f).name}: bilingua {len(x['units'])} units, locos {len(y['units'])}")
        if not same:
            bad += 1
            for i, (p, q) in enumerate(zip(x["units"], y["units"])):
                if p != q:
                    print("   first difference at", i, repr(p), repr(q)); break
    else:
        same = ("error" in x) == ("error" in y)
        print(f"{'same' if same else 'DIFF'}  {Path(f).name}: bilingua {x.get('error', 'ok')[:60]!r} locos {str(y.get('error', 'ok'))[:60]!r}")
        bad += not same

# Translated builds: the same "DE " + text translation through the LocOS filter must give the same bytes.
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-compare-")
sys.dont_write_bytecode = True
sys.path.insert(0, str(LOCOS))
from app.filters import get_filter  # noqa: E402
flt = get_filter("toml")
for f in files:
    if "merged" not in bl[f]:
        continue
    data = open(f, "rb").read()
    if "units" not in lo[f]:  # LocOS refused the file (already reported above)
        continue
    r = flt.parse(data)
    out = flt.build(data, {str(u.p_index): "DE " + u.text for u in r.units}).decode("utf-8")
    ok = out == bl[f]["merged"]
    print(f"{'same' if ok else 'DIFF'}  {Path(f).name}: translated build bytes")
    bad += not ok
sys.exit(1 if bad else 0)

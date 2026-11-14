"""Compares Bilingua's units with the LocOS filter and with Okapi (okf_openoffice) on a folder of ODF files.

    python tools/compare_oracles.py FILES_DIR [--bilingua-bin PATH] > report.txt

Needs: the example binary (`cargo build --release -p bilingua-odf --example odf_dump`), the LocOS venv python
(path in LOCOS_PY, default C:/Users/brent/code/local-tms/.venv/Scripts/python.exe) and the Okapi oracle in
C:/Users/brent/code/okapi-oracle. LocOS and Okapi are only read.
Texts are normalised as the oracle does: inline codes become one marker, NFC, NBSP to space, white space collapsed.
"""
import json, os, re, subprocess, sys, tempfile, unicodedata

here = os.path.dirname(os.path.abspath(__file__))
d = sys.argv[1]
bin_ = os.environ.get("ODF_DUMP", os.path.join(os.environ.get("CARGO_TARGET_DIR", "target"), "release", "examples", "odf_dump.exe"))
locos_py = os.environ.get("LOCOS_PY", "C:/Users/brent/code/local-tms/.venv/Scripts/python.exe")
locos_dump = os.path.join(here, "..", "..", "oracle", "locos_dump.py")
okapi = "C:/Users/brent/code/okapi-oracle/okapi_dump.py"
CODE = "\u00a4"
files = sorted(f for f in os.listdir(d) if f.rsplit(".", 1)[-1] in ("odt", "ods", "odp", "odg", "ott", "ots", "otp", "otg"))


def norm(t):
    t = re.sub("\u27e6/?[a-z]?[0-9]+\u27e7", CODE, t)
    t = unicodedata.normalize("NFC", t).replace("\u00a0", " ")
    return re.sub(r"\s+", " ", t).strip()


def run(cmd):
    return subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8").stdout


paths = [os.path.join(d, f) for f in files]
bil = {}
for line in run([bin_] + paths).splitlines():
    j = json.loads(line)
    bil[os.path.basename(j["file"])] = [norm(u["text"]) for u in j.get("units", [])]
out = tempfile.mktemp(suffix=".json")
run([locos_py, "-I", locos_dump, out] + paths)
loc = {}
for k, v in json.load(open(out, encoding="utf-8")).items():
    loc[os.path.basename(k)] = [norm(u["text"]) for u in v.get("units", [])]
tot = {"bil": 0, "loc": 0, "oka": 0}
same_loc = 0
all_b = all_both = all_o = 0
for f in files:
    ok = []
    for line in run(["python", "-I", okapi, os.path.join(d, f), "--fc", "okf_openoffice"]).splitlines():
        try:
            u = json.loads(line)
        except Exception:
            continue
        if "text" in u and u.get("translate", "yes") != "no":
            ok.append(norm(u["text"]))
    ok = [t for t in ok if t]
    b, l = bil.get(f, []), loc.get(f, [])
    sm = sum(1 for x, y in zip(b, l) if x == y) if len(b) == len(l) else -1
    same_loc += (b == l)
    om = len(set(b) & set(ok))
    all_b += len(b); all_both += om; all_o += len(ok)
    print(f"{f}: bilingua {len(b)}  locos {len(l)}  okapi {len(ok)}  bilingua==locos {b == l}  "
          f"in-both-with-okapi {om} (only bilingua {len(set(b) - set(ok))}, only okapi {len(set(ok) - set(b))})")
    for t in sorted(set(b) - set(ok))[:6]:
        print("    only bilingua:", t[:100])
    for t in sorted(set(ok) - set(b))[:6]:
        print("    only okapi:   ", t[:100])
print(f"bilingua == locos on {same_loc} of {len(files)} files")
print(f"units: bilingua {all_b}, okapi {all_o}; bilingua units also found in okapi {all_both} ({100*all_both/max(all_b,1):.1f}%)")

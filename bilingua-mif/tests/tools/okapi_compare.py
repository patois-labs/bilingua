"""Compares Bilingua's mif units with Okapi okf_mif's on Okapi's own test resources.

    python okapi_compare.py OKAPI_RESOURCES_DIR MIF_DUMP_EXE [file ...]

Both sides are normalised as bilingua/oracle/run.py does (NFC, NBSP to space, white space collapsed, codes as one
marker). Prints per file: Bilingua units, Okapi units, Bilingua units found among Okapi's, and the Okapi units
Bilingua has no unit for, by restype. Needs the Okapi oracle checkout (OKAPI_ORACLE).
"""

import collections
import json
import os
import re
import subprocess
import sys
import unicodedata

ORACLE = os.environ.get("OKAPI_ORACLE", r"C:\Users\brent\code\okapi-oracle")


def norm(s):
    s = unicodedata.normalize("NFC", s).replace("\u00a0", " ")
    return re.sub(r"\s+", " ", s).strip()


def main():
    res, exe = sys.argv[1], sys.argv[2]
    files = sys.argv[3:] or sorted(f for f in os.listdir(res) if f.endswith(".mif"))
    tot = collections.Counter()
    for f in files:
        p = os.path.join(res, f)
        mine = [norm(json.loads(l)["text"]) for l in subprocess.run([exe, p], capture_output=True, text=True, encoding="utf-8").stdout.splitlines()]
        out = subprocess.run([sys.executable, os.path.join(ORACLE, "okapi_dump.py"), p, "--fc", "okf_mif"], capture_output=True, text=True, encoding="utf-8")
        okapi = [json.loads(l) for l in out.stdout.splitlines() if l.startswith("{")]
        otexts = collections.Counter(norm(u["text"]) for u in okapi if u.get("translate") != "no")
        mc = collections.Counter(mine)
        found = sum(min(n, otexts[t]) for t, n in mc.items())
        missing = collections.Counter()
        left = otexts.copy()
        for t, n in mc.items():
            left[t] -= min(n, left[t])
        for u in okapi:
            t = norm(u["text"])
            if left.get(t, 0) > 0:
                left[t] -= 1
                missing[u.get("restype") or "-"] += 1
        tot["mine"] += len(mine)
        tot["okapi"] += len(okapi)
        tot["found"] += found
        print(f"{f}: bilingua {len(mine)}, okapi {len(okapi)}, bilingua units found in okapi {found}, okapi-only by restype {dict(missing)}")
        if out.returncode:
            print("  okapi error:", out.stderr[:200])
    print("TOTAL", dict(tot), "match of Bilingua units: %.1f%%" % (100.0 * tot["found"] / max(1, tot["mine"])))


main()

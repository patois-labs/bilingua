"""Records what Okapi okf_dtd (1.48.0, via okapi-oracle) makes of the Okapi test files and the corpus.

    python gen_okapi.py OUT.json FILE [FILE ...]
"""
import json
import subprocess
import sys

ORACLE = r"C:\Users\brent\code\okapi-oracle\okapi_dump.py"
out, files = sys.argv[1], sys.argv[2:]
res = {}
for f in files:
    p = subprocess.run([sys.executable, ORACLE, f, "--fc", "okf_dtd"], capture_output=True)
    if p.returncode != 0:
        res[f.replace("\\", "/").split("/")[-1]] = {"error": p.stderr.decode("utf-8", "replace")[:200]}
        continue
    units = [json.loads(l) for l in p.stdout.decode("utf-8").splitlines() if l.strip()]
    res[f.replace("\\", "/").split("/")[-1]] = {"units": [{"name": u["resname"], "text": u["text"], "codes": len(u["codes"])} for u in units]}
with open(out, "w", encoding="utf-8") as fh:
    json.dump(res, fh, ensure_ascii=False, indent=1)
print({k: (len(v["units"]) if "units" in v else v["error"]) for k, v in res.items()})

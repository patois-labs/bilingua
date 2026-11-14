"""Writes tests/data/oracle_units.json: the units the LocOS filters give for every oracle corpus file and every Okapi
test package in tests/data/okapi (read-only use of LocOS; also "comments": true runs). Run with the LocOS venv python:

    python tools/gen_oracle_units.py LOCOS_DIR CORPUS_DIR OKAPI_DIR OUT.json
"""
import json, os, sys, tempfile

locos, corpus, okapi, out = sys.argv[1:5]
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-oracle-")
sys.dont_write_bytecode = True
sys.path.insert(0, locos)
from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

res = {}
for tag, d in (("corpus", os.path.join(corpus, "odf")), ("okapi", okapi)):
    for name in sorted(os.listdir(d)):
        if name.rsplit(".", 1)[-1] not in ("odt", "ods", "odp", "odg", "ott", "ots", "otp", "otg"):
            continue
        data = open(os.path.join(d, name), "rb").read()
        f = get_filter(name.rsplit(".", 1)[-1])
        ent = {"units": [[u.p_index, u.text, u.style] for u in f.parse(data).units]}
        try:
            ent["comments"] = [[u.p_index, u.text] for u in f.parse(data, comments=True).units if u.p_index >= 1000000]
        except TypeError:
            pass
        res[f"{tag}/{name}"] = ent
open(out, "w", encoding="utf-8").write(json.dumps(res, ensure_ascii=False, indent=1))
print(len(res), "files")

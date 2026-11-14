"""Writes tests/data/oracle_units.json: the units the LocOS epub filter gives for every corpus file (read-only use of
LocOS). Run with the LocOS venv python:

    python tools/gen_oracle_units.py LOCOS_DIR CORPUS_DIR OUT.json
"""
import json, os, sys, tempfile

locos, corpus, out = sys.argv[1:4]
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-oracle-")
sys.dont_write_bytecode = True
sys.path.insert(0, locos)
from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

res = {}
for name in sorted(os.listdir(corpus)):
    if not name.endswith(".epub"):
        continue
    data = open(os.path.join(corpus, name), "rb").read()
    try:
        res[name] = [[u.text, u.style] for u in get_filter("epub").parse(data).units]
    except Exception as exc:  # a refusal is part of the answer
        res[name] = {"error": str(exc)}
open(out, "w", encoding="utf-8").write(json.dumps(res, ensure_ascii=False, indent=1))

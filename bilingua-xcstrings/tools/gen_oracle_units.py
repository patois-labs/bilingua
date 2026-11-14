"""Writes tests/data/oracle_units.json: the units the LocOS filters give for every corpus file (read-only use of
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
for kind in ("xcstrings", "xcloc"):
    for name in sorted(os.listdir(os.path.join(corpus, kind))):
        data = open(os.path.join(corpus, kind, name), "rb").read()
        res[f"{kind}/{name}"] = [u.text for u in get_filter(kind).parse(data).units]
open(out, "w", encoding="utf-8").write(json.dumps(res, ensure_ascii=False, indent=1))

"""Writes tests/data/locos_golden.json from the LocOS Qt Linguist filter (read only): for every tests/data/*.ts
and tests/data/okapi/*.ts the units LocOS extracts, and for the UTF-8 ones the text LocOS builds when every unit
is translated to "T:" + text, for no target and for four target languages.

    <local-tms venv python> -I gen_golden.py
"""
import json
import os
import sys
import tempfile

sys.stdout.reconfigure(encoding="utf-8")
LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)
from app import config  # noqa: E402
from app.filters.base import get_filter  # noqa: E402

config.MAX_DOCUMENT_SEGMENTS = 10**6
HERE = os.path.dirname(os.path.abspath(__file__))
DATA = os.path.join(HERE, "..", "data")
LOCALES = [None, "pl-PL", "ar-SA", "ja", "de-DE"]


def main():
    f = get_filter("ts")
    out = {}
    for sub in ("", "okapi"):
        d = os.path.join(DATA, sub)
        for name in sorted(os.listdir(d)):
            if not name.endswith(".ts"):
                continue
            data = open(os.path.join(d, name), "rb").read()
            pr = f.parse(data)
            ent = {"units": [[u.p_index, u.text] for u in pr.units]}
            plain = b"utf-8" in data[:100].lower()
            if plain and b"\r\n" not in data and sub == "":
                tr = {str(u.p_index): "T:" + u.text for u in pr.units}
                ent["merges"] = {
                    (loc or ""): (f.build(data, tr) if loc is None else f.build_localized(data, tr, loc)).decode("utf-8")
                    for loc in LOCALES
                }
            out[(sub + "/" if sub else "") + name] = ent
    with open(os.path.join(DATA, "locos_golden.json"), "w", encoding="utf-8", newline="\n") as fh:
        json.dump(out, fh, ensure_ascii=False, indent=0)
    print(len(out), "files")


main()

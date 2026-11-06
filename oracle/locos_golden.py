"""Writes the LocOS-profile goldens (Track D): run with the Localization OS venv Python, read-only.

    <local-tms>/.venv/Scripts/python.exe oracle/locos_golden.py [FORMAT ...]

For every input under `bilingua/tests/locos/<format>/` (any file not ending in `.golden.json`), LocOS's filter
parses it and builds it under a few mappings; the result goes next to the input as `<file>.golden.json`:

    {"format": "txt", "units": [{"p_index": 0, "text": "...", "style": ""}],
     "builds": [{"name": "none", "locale": null, "map": {}, "out_b64": "..."}, ...]}

`bilingua/tests/locos_golden.rs` reads them, extracts the same file through the facade under the LocOS profile,
turns the units into LocOS text with `profile::tokens` (as the adapter does) and compares, then sets each mapping
as targets and compares the merged bytes. Nothing in the LocOS tree is written; its data dir is a throwaway.
"""

import base64
import json
import os
import re
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.join(os.path.dirname(HERE), "bilingua", "tests", "locos")
LOCOS = os.environ.get("LOCOS_DIR") or r"C:\Users\brent\code\local-tms"
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-golden-")
os.environ.pop("LOC_TMS_BILINGUA_FORMATS", None)
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

TOKEN = re.compile("(\u27e6/?[a-z]?\\d+\u27e7)")
# Locales the localized builds use, per format (default: de-DE). PO also tries a CLDR plural language.
LOCALES = {"po": ["de-DE", "pl-PL", "ar-EG", "ja-JP"], "vtt": ["de-DE", "fr"], "arb": ["de-DE", "pl-PL"], "ftl": ["de-DE", "pl-PL"]}


def pseudo(text: str, tag: str) -> str:
    """The text with every run outside the tokens changed, tokens kept in place."""
    parts = TOKEN.split(text)
    out = []
    for i, p in enumerate(parts):
        if i % 2 == 1 or not p.strip():
            out.append(p)
        elif tag == "prefix":
            out.append("\u00dc " + p)
        else:
            out.append(p + " & <\"'>")
    return "".join(out)


def golden(fmt: str, path: str) -> dict:
    with open(path, "rb") as fh:
        data = fh.read()
    filt = get_filter(fmt)
    pr = filt.parse(data)
    units = [{"p_index": u.p_index, "text": u.text, "style": u.style or ""} for u in pr.units]
    builds = []

    def add(name, locale, mapping):
        try:
            if locale is None:
                out = filt.build(data, mapping)
            else:
                out = filt.build_localized(data, mapping, locale)
            builds.append({"name": name, "locale": locale, "map": mapping, "out_b64": base64.b64encode(out).decode()})
        except Exception as exc:  # recorded, the Rust side skips it
            builds.append({"name": name, "locale": locale, "map": mapping, "error": f"{type(exc).__name__}: {exc}"})

    add("none", None, {})
    add("prefix", None, {str(u["p_index"]): pseudo(u["text"], "prefix") for u in units})
    for loc in LOCALES.get(fmt, ["de-DE"]):
        half = {str(u["p_index"]): pseudo(u["text"], "escape") for k, u in enumerate(units) if k % 2 == 0}
        add(f"escape-{loc}", loc, half)
        add(f"none-{loc}", loc, {})
    return {"format": fmt, "units": units, "builds": builds}


def main() -> None:
    want = set(sys.argv[1:])
    for fmt in sorted(os.listdir(ROOT)):
        d = os.path.join(ROOT, fmt)
        if not os.path.isdir(d) or (want and fmt not in want):
            continue
        for name in sorted(os.listdir(d)):
            if name.endswith(".golden.json"):
                continue
            g = golden(fmt, os.path.join(d, name))
            with open(os.path.join(d, name + ".golden.json"), "w", encoding="utf-8", newline="\n") as fh:
                json.dump(g, fh, ensure_ascii=False, indent=1)
                fh.write("\n")
            print(fmt, name, len(g["units"]), "units", sum("error" in b for b in g["builds"]), "build errors")


if __name__ == "__main__":
    main()

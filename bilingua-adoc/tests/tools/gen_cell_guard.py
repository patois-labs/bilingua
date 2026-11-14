"""Builds tests/data/cell_guard_norm.json: for each CELL_GUARD case of tests/locos_golden.rs, the digests of the LocOS
builds with the #788 differences taken out (every `{empty}` that starts a line, every backslash right before `|`).

The golden test takes the same two things out of the Bilingua build and compares: so a CELL_GUARD case may differ
from LocOS only by the new cell escapes and the `{empty}` guard, nothing else.

Run with the Localization OS venv Python (read only use of its filters; nothing is written there):

    C:\\Users\\brent\\code\\local-tms\\.venv\\Scripts\\python.exe gen_cell_guard.py
"""

import importlib.util
import json
import os
import re

HERE = os.path.dirname(os.path.abspath(__file__))
TESTS = os.path.dirname(HERE)

spec = importlib.util.spec_from_file_location("gen_golden", os.path.join(HERE, "gen_golden.py"))
g = importlib.util.module_from_spec(spec)
spec.loader.exec_module(g)


def norm(b):
    """Drops `{empty}` at a line start and `\\` before `|` (the same rule as `norm` in locos_golden.rs)."""
    s = b.decode("utf-8")
    out, i = [], 0
    while i < len(s):
        if s.startswith("{empty}", i) and (i == 0 or s[i - 1] in "\r\n"):
            i += 7
            continue
        if s[i] == "\\" and s[i + 1 : i + 2] == "|":
            i += 1
            continue
        out.append(s[i])
        i += 1
    return "".join(out).encode("utf-8")


def main():
    src = open(os.path.join(TESTS, "locos_golden.rs"), encoding="utf-8").read()
    body = src[src.index("const CELL_GUARD:") :]
    names = re.findall(r'"(random-\d+)"', body[: body.index("];")])
    cases = {c["name"]: c for c in json.load(open(os.path.join(TESTS, "data", "locos_golden.json"), encoding="utf-8"))}
    f = g.get_filter("adoc")
    out = {}
    for name in names:
        c = cases[name]
        data, units = bytes.fromhex(c["hex"]), c["units"]
        rec = {}

        def mapping(label):
            fn = g.MAPS[label]
            return {str(i): fn(i, t) for i, t in enumerate(units) if fn(i, t) is not None}

        for label, d in c["builds"].items():
            b = f.build(data, mapping(label))
            assert g.digest(b) == d, (name, label, "LocOS no longer matches the golden")
            rec[label] = g.digest(norm(b))
        for lc, d in c.get("localized", {}).items():
            if lc.endswith("|map"):
                continue
            b = f.build_localized(data, mapping(c["localized"][lc + "|map"]), lc)
            assert g.digest(b) == d, (name, lc, "LocOS no longer matches the golden")
            rec[lc] = g.digest(norm(b))
        out[name] = rec
    with open(os.path.join(TESTS, "data", "cell_guard_norm.json"), "w", encoding="utf-8", newline="\n") as fh:
        json.dump(out, fh, indent=0, sort_keys=True)
    print(len(out), "cases")


if __name__ == "__main__":
    main()

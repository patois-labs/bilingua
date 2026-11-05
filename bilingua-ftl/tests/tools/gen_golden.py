"""Builds tests/data/locos_golden.json: random Fluent files and what the LocOS Fluent filter makes of them.

Run with the Localization OS venv Python (read only use of its filters; nothing is written there):

    <LocOS venv python> -B gen_golden.py OUT.json [CORPUS_FILE ...]

For each file the golden holds the input bytes (hex), the units LocOS parses (text and style, or the error class),
the bytes it builds with three mappings (every unit, every third unit, plain text), the localized builds for
several target locales, and the same with terms translated. Deterministic (seed 1104).
"""

import json
import os
import random
import sys
import tempfile

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-ftl-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

WORDS = [
    "Save", "Cancel", "Welcome back", "Hello, { $name }!", "{ $n } items", "Text { -brand } text", "{ $only }",
    "---", "Caf\u00e9", "\u65e5\u672c\u8a9e", "\u0645\u0631\u062d\u0628\u0627", "\ud55c\uad6d\uc5b4", "a { \"{\" } b",
    "{ NUMBER($n, minimumFractionDigits: 2) }", "{ msg.attr } there", "emoji \U0001f600", "  padded  ", "x",
    "{ $a } and { $b }", "100%", "tab\there", "\u00a0nbsp", "{ \"[\" }bracket", "ends with brace { \"}\" }",
]
IDS = ["hello", "title", "login-button", "a", "x-y_z", "Msg9", "dup", "emails"]
LOCALES = ["pl-PL", "ar", "ja", "cy", "fr"]


def pattern(rng, indent="    "):
    kind = rng.random()
    if kind < 0.55:
        return rng.choice(WORDS)
    if kind < 0.75:
        return "\n" + indent + rng.choice(WORDS) + "\n" + indent + rng.choice(WORDS)
    if kind < 0.85:
        return rng.choice(WORDS) + "\n" + indent + "  " + rng.choice(WORDS) + "\n" + indent + rng.choice(WORDS)
    return "\n" + indent + rng.choice(WORDS) + "\n\n" + indent + rng.choice(WORDS)


def select(rng):
    sel = rng.choice(["$count", "$n", "$gender", "NUMBER($n, type: \"ordinal\")", "NUMBER($n)"])
    if sel == "$gender":
        keys = ["male", "female"]
    else:
        keys = rng.choice([["one"], ["zero", "one"], ["one", "few"], ["1", "2"], ["one", "two", "few"]])
    s = "{ " + sel + " ->\n"
    for k in keys:
        s += "    [" + k + "] " + rng.choice(WORDS) + "\n"
    s += "   *[other] " + rng.choice(WORDS) + rng.choice(["", "\n        more text"]) + "\n}"
    return s


def make(rng):
    nl = rng.choice(["\n", "\n", "\r\n"])
    parts = []
    for _ in range(rng.randint(0, 14)):
        r = rng.random()
        mid = rng.choice(IDS)
        if r < 0.10:
            parts.append(rng.choice(["# note", "## group", "### resource", "#bad", "#", "# a\n# b"]))
        elif r < 0.45:
            parts.append(f"{mid} = {pattern(rng)}")
        elif r < 0.55:
            parts.append(f"{mid} = {pattern(rng)}\n    .title = {rng.choice(WORDS)}\n    .accesskey = {rng.choice('LSX')}")
        elif r < 0.62:
            parts.append(f"{mid} =\n    .placeholder = {rng.choice(WORDS)}")
        elif r < 0.72:
            parts.append(f"-{mid} = {rng.choice(['Firefox', 'Brand { $x }', 'Lumen'])}\n    .gender = masculine")
        elif r < 0.84:
            parts.append(f"{mid} = {select(rng)}")
        elif r < 0.88:
            parts.append(f"{mid} = Before {select(rng)} after\n    .t = {rng.choice(WORDS)}")
        elif r < 0.93:
            parts.append(rng.choice(["broken = { $x", "bad id = x", "= nothing", "x = }", "y = { \"unterminated", "{ stray }", "z ="]))
        else:
            parts.append(rng.choice(["", " ", "   ", "\t"]))
    text = nl.join(p.replace("\n", nl) for p in parts) + (nl if rng.random() < 0.7 else "")
    text = text.replace("\r\r", "\r")
    data = text.encode("utf-8")
    return b"\xef\xbb\xbf" + data if rng.random() < 0.2 else data


def golden(data):
    f = get_filter("ftl")
    rec = {"hex": data.hex()}
    for terms in (False, True):
        if terms:
            os.environ["FTL_TRANSLATE_TERMS"] = "1"
        else:
            os.environ.pop("FTL_TRANSLATE_TERMS", None)
        key = "terms" if terms else "main"
        r = {}
        try:
            units = f.parse(data).units
        except Exception as e:  # noqa: BLE001
            r["error"] = type(e).__name__
            rec[key] = r
            continue
        r["units"] = [[u.text, u.style] for u in units]
        n = len(units)
        maps = {
            "all": {str(i): ("T%d {\"x\"} é" + chr(10) + "[b] two" + chr(10) + "  .c *d" + chr(10) + chr(10) + "*e") % i for i in range(n)},
            "third": {str(i): "Zeta %d \u65e5" % i for i in range(0, n, 3)},
            "plain": {str(i): "Plain%d" % i for i in range(n)},
        }
        r["builds"] = {k: f.build(data, m).hex() for k, m in maps.items()}
        if not terms:
            r["localized"] = {loc: f.build_localized(data, maps["all"], loc).hex() for loc in LOCALES}
        rec[key] = r
    os.environ.pop("FTL_TRANSLATE_TERMS", None)
    return rec


def main():
    rng = random.Random(1104)
    cases = []
    for path in sys.argv[2:]:
        data = open(path, "rb").read()
        rec = golden(data)
        rec["name"] = "corpus:" + os.path.basename(path)
        cases.append(rec)
    for i in range(260):
        rec = golden(make(rng))
        rec["name"] = f"random{i}"
        cases.append(rec)
    json.dump(cases, open(sys.argv[1], "w"))
    print(len(cases), "cases")


main()

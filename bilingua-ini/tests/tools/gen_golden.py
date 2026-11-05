"""Builds tests/data/locos_golden.json: random INI-like files and what the LocOS INI filter makes of them.

Run with the Localization OS venv Python (read only use of its filters; nothing is written there):

    C:\\Users\\brent\\code\\local-tms\\.venv\\Scripts\\python.exe gen_golden.py OUT.json [CORPUS_FILE ...]

For each file the golden holds the input bytes (hex), the units LocOS parses (or the error class), and the bytes
LocOS builds with two mappings: every unit translated, and every third unit translated. Deterministic (seed 1104).
"""

import json
import os
import random
import sys
import tempfile

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-ini-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

WORDS = [
    "Save", "Cancel", "Welcome back", "Click \\\"Save\\\" now", 'Press "_QQ_"Go"_QQ_" now', "%d items in %s", "{0}",
    "---", "%@", "", "Caf\u00e9", "\u65e5\u672c\u8a9e", "\u0645\u0631\u062d\u0628\u0627", "\ud55c\uad6d\uc5b4", "<b>Hi</b>",
    "a=b", "100%", "50% off", "x", "  padded  ", "\u00a0nbsp", "\u00bd", "\u2460", "tab\there", "emoji \U0001f600",
]
KEYS = ["title", "COM_X_LOGIN", "k", "a b", "", "dup", "Key_2", "\u00fcber", "x.y"]
COMMENTS = ["; note", "# hash note", ";", "#", "  ; indented", ";; double"]


def make(rng):
    nl = rng.choice(["\n", "\r\n", "\r"])
    lines = []
    for _ in range(rng.randint(0, 25)):
        r = rng.random()
        if r < 0.1:
            lines.append(rng.choice(COMMENTS))
        elif r < 0.2:
            lines.append(rng.choice(["[General]", "[ Spaced ]", "[]", "[unclosed", "[a=b]", "  [Indented]  "]))
        elif r < 0.25:
            lines.append("")
        elif r < 0.3:
            lines.append(rng.choice(["just a stray line", "=", "= =", "  ", "key"]))
        else:
            k = rng.choice(KEYS)
            v = rng.choice(WORDS)
            sep = rng.choice(["=", " = ", "=  ", "\t=\t"])
            if rng.random() < 0.4:
                v = '"' + v + '"'
            lines.append(k + sep + v)
    text = nl.join(lines) + (nl if rng.random() < 0.7 else "")
    kind = rng.choice(["utf8", "utf8", "utf8bom", "utf16le", "utf16be", "cp1252"])
    if kind == "utf8":
        return text.encode("utf-8")
    if kind == "utf8bom":
        return b"\xef\xbb\xbf" + text.encode("utf-8")
    if kind == "utf16le":
        return b"\xff\xfe" + text.encode("utf-16-le")
    if kind == "utf16be":
        return b"\xfe\xff" + text.encode("utf-16-be")
    try:
        return text.encode("cp1252")
    except UnicodeEncodeError:
        return text.encode("utf-8")


def golden(data):
    f = get_filter("ini")
    rec = {"hex": data.hex()}
    try:
        units = [u.text for u in f.parse(data).units]
    except Exception as e:  # noqa: BLE001
        rec["error"] = type(e).__name__
        return rec
    rec["units"] = units
    maps = {
        "all": {str(i): 'T%d "q" \u00e9\nline2' % i for i in range(len(units))},
        "third": {str(i): "Zeta %d \u65e5" % i for i in range(0, len(units), 3)},
        "plain": {str(i): "Plain%d" % i for i in range(len(units))},
    }
    rec["builds"] = {name: f.build(data, m).hex() for name, m in maps.items()}
    rec["records"] = [
        {"key": r.key, "value": r.value, "comment": r.comment, "context": r.context, "unit_index": r.unit_index, "duplicate": r.duplicate}
        for r in f.records(data)
    ]
    return rec


HOSTILE = [bytes.fromhex(h) for h in (
    "6b3d00760a",  # NUL
    "fffe6b003d00",  # UTF-16 LE, odd length
    "fffe00d86b00",  # lone surrogate
    "6b3d810a",  # 0x81 undefined in Windows-1252
    "fffe00006b000000",  # UTF-32 BOM
    "efbbbf6b3dff0a",  # bad UTF-8 after a BOM
    "0102030405060708" * 20 + "e9",  # control bytes, not UTF-8
    "6b3d636166e99d0a",  # 0x9D undefined
)]


def main():
    out, extra = sys.argv[1], sys.argv[2:]
    rng = random.Random(1104)
    cases = []
    for i in range(400):
        cases.append({"name": f"random-{i}", **golden(make(rng))})
    for i, h in enumerate(HOSTILE):
        cases.append({"name": f"hostile-{i}", **golden(h)})
    for path in extra:
        with open(path, "rb") as fh:
            cases.append({"name": os.path.basename(path), **golden(fh.read())})
    with open(out, "w", encoding="utf-8") as fh:
        json.dump(cases, fh, indent=0)
    print(len(cases), "cases,", sum(1 for c in cases if "error" in c), "refused")


if __name__ == "__main__":
    main()

r"""Builds tests/data/locos_golden.json: random XPress Tags files and what the LocOS filter makes of them.

Run with the Localization OS venv Python (read only use of its filters; nothing is written there):

    C:\Users\brent\code\local-tms\.venv\Scripts\python.exe gen_golden.py OUT.json [FILE ...]

For each file the golden holds the input bytes (hex), the unit texts and styles LocOS parses (or the error
class), and the bytes LocOS builds for several translation mappings (hex). Deterministic (seed 1105).
"""

import json
import os
import random
import re
import sys
import tempfile

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-xtg-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

WORDS = ["Save", "Cancel", "Hello", "world", "Account", "settings", "Getting", "started", "Caf\u00e9", "na\u00efve",
         "3", "42", "price", "Gr\u00fc\u00dfe", "stra\u00dfe", "and", "the", "a", "fox", "dog", "item", "list"]
JA = ["\u65e5\u672c\u8a9e", "\u30c6\u30ad\u30b9\u30c8", "\u4fdd\u5b58"]
KO = ["\ud55c\uad6d\uc5b4", "\ud14d\uc2a4\ud2b8"]
ZH = ["\u4e2d\u6587", "\u6587\u5b57"]
AR = ["\u0645\u0631\u062d\u0628\u0627", "\u0634\u0643\u0631\u0627"]
TAGS = ["<B>", "<B$>", "<I>", "<I$>", "<P>", "<$>", '<f"Arial">', "<f$>", "<z12>", "<z$>", '<c"Red">', "<c$>",
        "<@Emph>", "<@$>", "<\\n>", "<\\t>", "<\\i>", "<\\@>", "<\\<>", "<\\\\>", "<\\>>", "<\\#13>", "<*L>", "<*C>",
        '<XI,"Entry","">', '<A(3,"HYP",)[0]>', "<BI>", "<U>", "<U$>", "<//>", "<+>", "<->", "<K>", "<z14>", "<s50>",
        '<f"Times"z10>', "<\\#9>"]
STYLES = ["@Body:", "@Heading:", "@$:", "@:", "@Normal:", ""]
DEFS = ['@Body=[S"",""]<*L*h"Standard"f"Helvetica"z12>', '@Font set 1=[F]<f"Arial">', '~Mark=<BookmarkRef"x">']


def para(rng, words):
    parts = [rng.choice(STYLES)]
    n = rng.randint(1, 9)
    for _ in range(n):
        r = rng.random()
        if r < 0.45:
            parts.append(rng.choice(words))
        elif r < 0.9:
            parts.append(rng.choice(TAGS))
        else:
            parts.append(" ")
        if rng.random() < 0.5:
            parts.append(" ")
    return "".join(parts)


def make(rng, kind):
    words = WORDS
    if kind == "cp932":
        words = JA + WORDS[:6]
    elif kind == "cp949":
        words = KO + WORDS[:6]
    elif kind == "big5":
        words = ZH + WORDS[:6]
    elif kind == "gbk":
        words = ZH + WORDS[:6]
    elif kind == "utf8":
        words = WORDS + JA + AR
    elif kind in ("cp1252", "mac"):
        words = WORDS
    elif kind == "latin1":
        words = [w for w in WORDS if w.isascii() or w in ("na\u00efve", "Gr\u00fc\u00dfe", "stra\u00dfe", "Caf\u00e9")]
    nl = rng.choice(["\r\n", "\n", "\r"])
    enum = {"utf8": 9, "utf16": 8, "cp1252": 1, "mac": 0, "latin1": 2, "cp932": 3, "cp949": 19, "big5": 6, "gbk": 7, "none": None}[kind]
    lines = []
    if enum is not None:
        lines.append(rng.choice(["<v19.00>", "<v8.00>", ""]) + "<e%d>" % enum)
    for _ in range(rng.randint(1, 12)):
        r = rng.random()
        if r < 0.1:
            lines.append(rng.choice(DEFS))
        elif r < 0.2:
            lines.append("")
        else:
            lines.append(para(rng, words))
    return nl.join(lines) + rng.choice([nl, ""])


def encode(rng, text, kind):
    if kind == "utf8":
        return (b"\xef\xbb\xbf" if rng.random() < 0.3 else b"") + text.encode("utf-8")
    if kind == "utf16":
        c = rng.choice(["le", "be", "nobom"])
        if c == "le":
            return b"\xff\xfe" + text.encode("utf-16-le")
        if c == "be":
            return b"\xfe\xff" + text.encode("utf-16-be")
        return text.encode("utf-16-le")
    if kind == "none":
        return text.encode("utf-8") if rng.random() < 0.5 else text.encode("cp1252", "replace")
    codec = {"cp1252": "cp1252", "mac": "mac_roman", "latin1": "latin-1", "cp932": "cp932", "cp949": "cp949",
             "big5": "big5", "gbk": "gbk"}[kind]
    return text.encode(codec, "replace")


TOK = re.compile(r"(\u27e6/?[a-z]?\d+\u27e7)")


def variants(rng, text):
    toks = [t for t in TOK.split(text) if t]
    out = {}
    out["suffix"] = text + " !"
    sh = toks[:]
    rng.shuffle(sh)
    out["shuffle"] = "".join(sh)
    out["drop"] = "".join(t for t in toks if not (TOK.fullmatch(t) and rng.random() < 0.5)) or "x"
    out["dup"] = "".join(t + (t if TOK.fullmatch(t) and rng.random() < 0.4 else "") for t in toks)
    out["plain"] = rng.choice(["Plain <b> @x\\y", "~X=<y", "A\nB", "tab\there", "\u65e5\u672c\u8a9e", "caf\u00e9 \u20ac", "x"])
    out["wrapped"] = "\u27e6g1\u27e7" + text + "\u27e6/g1\u27e7"
    return out


def case(name, data, rng):
    f = get_filter("xtg")
    rec = {"name": name, "input": data.hex(), "units": None, "error": None, "builds": []}
    try:
        pr = f.parse(data)
    except Exception as e:  # noqa: BLE001
        rec["error"] = type(e).__name__
        return rec
    rec["units"] = [[u.text, u.style] for u in pr.units]
    if not pr.units:
        return rec
    for vname in ("suffix", "shuffle", "drop", "dup", "plain", "wrapped"):
        mapping = {}
        for i, u in enumerate(pr.units):
            if rng.random() < 0.7:
                v = variants(rng, u.text)[vname]
                mapping[str(i)] = v
        if not mapping:
            continue
        b = {"variant": vname, "mapping": mapping}
        try:
            b["out"] = f.build(data, mapping).hex()
        except Exception as e:  # noqa: BLE001
            b["error"] = type(e).__name__
        rec["builds"].append(b)
    return rec


def main():
    rng = random.Random(1105)
    out = []
    kinds = ["utf8", "utf8", "utf16", "cp1252", "mac", "latin1", "cp932", "cp949", "big5", "gbk", "none"]
    for i in range(260):
        kind = kinds[i % len(kinds)]
        data = encode(rng, make(rng, kind), kind)
        out.append(case("rand%03d-%s" % (i, kind), data, rng))
    for p in sys.argv[2:]:
        with open(p, "rb") as fh:
            out.append(case(os.path.basename(p), fh.read(), rng))
    with open(sys.argv[1], "w", encoding="utf-8") as fh:
        json.dump(out, fh, ensure_ascii=False)
    n_err = sum(1 for r in out if r["error"])
    print("cases", len(out), "errors", n_err, "builds", sum(len(r["builds"]) for r in out))


main()

r"""Builds tests/data/locos_golden.json: random CSV/TSV files and what the LocOS tabular filter makes of them.

Run with the Localization OS venv Python (read only use of its filters; nothing is written there):

    C:\Users\brent\code\local-tms\.venv\Scripts\python.exe gen_golden.py OUT.json [FILE ...]

Each case holds the input bytes (hex), the call (filter, column mapping, locales, target locale), the units LocOS
parses with their notes (or the error class) and the bytes LocOS builds with three mappings. Deterministic (1104).
"""

import json
import os
import random
import sys
import tempfile

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-csv-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

WORDS = [
    "Save", "Cancel", "Welcome back", "Hello, world", 'He said "hi"', "Line one\nline two", "Line\r\nbreak", "=SUM(A1)",
    '="0012"', "+1+1", "-5", "@user", "123", "", "", "", "Caf\u00e9", "\u65e5\u672c\u8a9e", "\u0645\u0631\u062d\u0628\u0627",
    "\ud55c\uad6d\uc5b4", "<b>Hi</b>", "a;b", "a|b", "  padded  ", "tab\there", "'quoted", "50% off", "x", "Plus one",
    "emoji \U0001f600", "\u2460", "k1", "Done.", "  =lead formula",
]
HEADERS = [
    None, None, ["key", "en", "de", "fr", "notes"], ["source", "target"], ["id", "text"], ["Name", "Count"],
    ["ID", "English", "German"], ["Key", "Source", "Target", "Context", "Max length"], ["en-US", "de-DE", "fr-FR"],
    ["Type", "Identification", "Field", "Locale", "Default content", "Translated content"], ["a", "", "c"],
    ["text"], ["\ufefftarget", "source"],
]
MAPS = [
    None, None, None, None, None, "source=1,target=2", "source=2,target=3", "source=English,target.de=German,key=ID", "source=en,target.de=de,target.fr=fr,key=key,context=notes",
    "header=no,source=2,target=3", "key=1", "source=Nope", "target=3", "delimiter=semicolon", "source=1", "header=yes,source=text",
    "source=2,target=9", "context=2", "colour=red", "source=Source,target=Target,key=Key,context=Context,maxlen=Max length",
]
LOCS = [("en", ["de", "fr"]), ("en-US", ["de-DE", "fr-FR"]), ("", []), ("de", ["en"]), ("en", [])]
BUILD_LOCALES = [None, "de", "fr-FR", "de-DE", "xx"]
DELIMS = [",", ",", ",", ";", "\t", "|"]


def cell(rng, delim, always_quote):
    v = rng.choice(WORDS)
    need = any(c in v for c in (delim, '"', "\r", "\n"))
    if need or always_quote or (rng.random() < 0.08):
        return '"' + v.replace('"', '""') + '"'
    return v


def make(rng, tsv):
    delim = "\t" if tsv else rng.choice(DELIMS)
    hdr = rng.choice(HEADERS)
    width = len(hdr) if hdr else rng.randint(1, 5)
    nl = rng.choice(["\n", "\r\n", "\r"])
    lines = []
    if hdr:
        lines.append(delim.join(hdr))
    for _ in range(rng.randint(0, 12)):
        r = rng.random()
        if r < 0.06:
            lines.append("")
            continue
        w = width if rng.random() < 0.8 else rng.randint(1, width + 2)
        aq = rng.random() < 0.2
        lines.append(delim.join(cell(rng, delim, aq) for _ in range(w)))
    ends = [rng.choice([nl, nl, nl, rng.choice(["\n", "\r\n"])]) for _ in lines]
    text = "".join(ln + e for ln, e in zip(lines, ends))
    if text and rng.random() < 0.25:
        text = text.rstrip("\r\n")
    if rng.random() < 0.03:
        text += '"unterminated, field\nmore'
    kind = rng.choice(["utf8", "utf8", "utf8", "utf8bom", "utf16le", "utf16be", "cp1252"])
    if kind == "utf8":
        return text.encode("utf-8"), delim
    if kind == "utf8bom":
        return b"\xef\xbb\xbf" + text.encode("utf-8"), delim
    if kind == "utf16le":
        return b"\xff\xfe" + text.encode("utf-16-le"), delim
    if kind == "utf16be":
        return b"\xfe\xff" + text.encode("utf-16-be"), delim
    try:
        return text.encode("cp1252"), delim
    except UnicodeEncodeError:
        return text.encode("utf-8"), delim


def golden(data, fmt, columns, src, targets, build_loc):
    f = get_filter(fmt)
    rec = {"hex": data.hex(), "fmt": fmt, "columns": columns, "src": src, "targets": targets, "locale": build_loc}
    # a project's target list always holds the locale being built; Bilingua adds target_lang the same way
    plist = list(targets) + ([build_loc] if build_loc and build_loc not in targets else [])
    try:
        pr = f.parse(data, columns=columns, source_locale=src, target_locales=plist)
    except Exception as e:  # noqa: BLE001
        rec["error"] = type(e).__name__
        return rec
    sk = pr.skeleton
    rec["units"] = [u.text for u in pr.units]
    rec["notes"] = [u.get("note", "") for u in sk["units"]]
    rec["delimiter"] = sk["table"]["delimiter"]
    rec["inplace"] = bool((sk["table"]["plan"] or {}).get("inplace"))
    n = len(pr.units)
    maps = {
        "all": {str(i): 'T%d, "q" \u00e9\nline2' % i for i in range(n)},
        "third": {str(i): "Zeta %d \u65e5" % i for i in range(0, n, 3)},
        "plain": {str(i): "Plain%d" % i for i in range(n)},
        "echo": {str(i): pr.units[i].text for i in range(n)},
        "guard": {str(i): ["=1+1", "+x", "-y", "@z", "\tt", "'q", "\uff1dX"][i % 7] for i in range(n)},
    }
    rec["builds"] = {}
    for name, m in maps.items():
        try:
            rec["builds"][name] = f.build_localized(data, m, build_loc, table=sk["table"]).hex()
        except Exception as e:  # noqa: BLE001
            rec["builds"][name] = "ERR:" + type(e).__name__
    rec["untouched"] = f.build_localized(data, {}, build_loc, table=sk["table"]).hex() == data.hex()
    return rec


HOSTILE = [bytes.fromhex(h) for h in (
    "736f757263650a63616681000020780a",  # binary-ish
    "fffe6b003d00",  # UTF-16 LE odd length
    "fffe00d86b00",  # lone surrogate
    "6b2c810a",  # 0x81 undefined in Windows-1252
    "fffe00006b000000",  # UTF-32 BOM
    "efbbbf6b2cff0a",  # bad UTF-8 after a BOM
    "0102030405060708" * 20 + "e9",
)]


def main():
    out, extra = sys.argv[1], sys.argv[2:]
    rng = random.Random(1104)
    cases = []
    for i in range(800):
        tsv = rng.random() < 0.3
        data, _d = make(rng, tsv)
        src, targets = rng.choice(LOCS)
        cases.append({"name": f"random-{i}", **golden(data, "tsv" if tsv else "csv", rng.choice(MAPS), src, targets, rng.choice(BUILD_LOCALES))})
    for i, h in enumerate(HOSTILE):
        cases.append({"name": f"hostile-{i}", **golden(h, "csv", None, "en", ["de"], "de")})
    for path in extra:
        with open(path, "rb") as fh:
            data = fh.read()
        for j, (fmt, cols, src, targets, loc) in enumerate([
            ("csv", None, "en", ["de"], "de"), ("tsv", None, "en", ["de"], "de"), ("csv", "header=no", "en", [], None),
            ("csv", "header=yes,source=1,target=2", "en", ["de"], "de"),
        ]):
            cases.append({"name": f"{os.path.basename(path)}#{j}", **golden(data, fmt, cols, src, targets, loc)})
    with open(out, "w", encoding="utf-8") as fh:
        json.dump(cases, fh, indent=0)
    print(len(cases), "cases,", sum(1 for c in cases if "error" in c), "refused")


if __name__ == "__main__":
    main()

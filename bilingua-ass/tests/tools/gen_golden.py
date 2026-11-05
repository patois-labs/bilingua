"""Builds tests/data/locos_golden.json: random ASS/SSA files and what the LocOS ASS filter makes of them.

Run with the Localization OS venv Python (read only use of its filters; nothing is written there):

    C:\\Users\\brent\\code\\local-tms\\.venv\\Scripts\\python.exe gen_golden.py OUT.json [CORPUS_FILE ...]

For each file the golden holds the input bytes (hex), the units LocOS parses (or the error class), and the bytes
LocOS builds for several translation mappings (pseudo-translated, reversed token order, tokens dropped, every third
unit, a newline typed in) and for the localized export (de-DE, ja-JP, fil-PH). Deterministic (seed 1105).
"""

import json
import os
import random
import re
import sys
import tempfile

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-ass-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

WORDS = [
    "Hello", "world,", "how are", "you?", "Caf\u00e9", "\u65e5\u672c\u8a9e\u306e\u5b57\u5e55", "\u4e2d\u6587\u5b57\u5e55", "\ud55c\uad6d\uc5b4",
    "\u0645\u0631\u062d\u0628\u0627 \u0628\u0627\u0644\u0639\u0627\u0644\u0645", "\u041f\u0440\u0438\u0432\u0435\u0442", "ok", "x", "a, b, c", "100%", "...",
    "\u00a0nbsp", "\u2028sep", "ff\x0cff", "tab\there", " ", "  ", "emoji \U0001f600", "\u27e6x9\u27e7 literal", "Hi {there", "a} stray",
    "C:\\path", "Isn't it?", "\u00bd", "\u2460",
]
BLOCKS = [
    "{\\an8}", "{\\pos(400,570)}", "{\\i1}", "{\\i0}", "{\\b1}", "{\\b0}", "{\\b700}", "{\\u1}", "{\\u0}", "{\\s1}", "{\\s0}", "{\\r}", "{\\i1\\b1}",
    "{\\i0\\b0}", "{\\i1\\b0}", "{\\fs20\\i1}", "{\\c&H0000FF&}", "{note}", "{}", "{\\p1}", "{\\p0}", "{\\p 2}", "{\\fad(100,200)}", "{\\b01}",
    "{\\b12345}", "{\\i10}", "{\\k20}", "{\\blur2}", "{a{b}",
]
ESC = ["\\N", "\\n", "\\h"]
STYLES = ["Default", "Sign", "*Default", ""]
TITLES = ["The Long Night", "<untitled>", "<Untitled>", "", "  ", "T\u00edtulo", "A, B", "{\\i1}x{\\i0}", "x  ", "\u00a0"]
LOCALES = ["de-DE", "ja-JP", "fil-PH", "x-klingon"]


def text_field(rng):
    out = []
    for _ in range(rng.randint(0, 8)):
        r = rng.random()
        if r < 0.45:
            out.append(rng.choice(WORDS) + rng.choice(["", " ", " "]))
        elif r < 0.8:
            out.append(rng.choice(BLOCKS))
        else:
            out.append(rng.choice(ESC))
    return "".join(out)


def events(rng, nl):
    lines = []
    fmt = rng.choice(["default", "default", "default", "short", "textfirst", "none", "SSA", "bad"])
    names = {
        "default": "Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text",
        "short": "Start, End, Text",
        "textfirst": "Text, Start",
        "none": None,
        "SSA": "Marked, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text",
        "bad": "",
    }[fmt]
    n = {"default": 10, "short": 3, "textfirst": 2, "none": 10, "SSA": 10, "bad": 1}[fmt]
    lines.append(rng.choice(["[Events]", "[events]", " [Events] "]))
    if names is not None:
        lines.append(rng.choice(["Format: ", "format:", "FORMAT :  "]) + names)
    for i in range(rng.randint(0, 8)):
        kind = rng.choice(["Dialogue", "Dialogue", "Dialogue", "Comment", "Dialogue ", "dialogue"])
        fields = ["0", "0:00:%02d.00" % i, "0:00:%02d.50" % (i + 1), rng.choice(STYLES), "Name", "0", "0", "0", rng.choice(["", "!Effect"])]
        fields = fields[: n - 1] if n > 1 else []
        while len(fields) < n - 1:
            fields.append("x")
        if rng.random() < 0.05 and fields:
            fields = fields[:-1]  # too few fields
        sep = rng.choice([": ", ":", " : ", ":\t"])
        lines.append(kind + sep + ",".join(fields + [text_field(rng)]) if n > 1 or True else "")
    return lines


def make(rng):
    nl = rng.choice(["\n", "\r\n", "\r", "\n"])
    lines = []
    if rng.random() < 0.9:
        lines.append(rng.choice(["[Script Info]", "[script info]", "[Script Info] "]))
        if rng.random() < 0.8:
            lines.append(rng.choice(["Title: ", "title:", "Title:", "T\u0130tle: ", "T\u0131tle: ", "Title:\t"]) + rng.choice(TITLES) + rng.choice(["", " ", "\t"]))
        if rng.random() < 0.5:
            lines.append("ScriptType: v4.00+")
        if rng.random() < 0.3:
            lines.append(rng.choice(["Language: en", "language:EN  ", "Language:"]))
        if rng.random() < 0.3:
            lines.append("; a comment")
    for _ in range(rng.randint(0, 2)):
        if rng.random() < 0.7:
            lines.append(rng.choice(["[V4+ Styles]", "[Fonts]", "[Aegisub Project Garbage]"]))
            lines.append("Title: not a unit")
            lines.append("Style: Default,Arial,48")
    if rng.random() < 0.9:
        lines.extend(events(rng, nl))
    if rng.random() < 0.2:
        lines.append("[Script Info]")
        lines.append("Title: second block")
        lines.append("Language: fr")
    text = nl.join(lines) + (nl if rng.random() < 0.7 else "")
    kind = rng.choice(["utf8", "utf8", "utf8", "utf8bom", "utf16le", "utf16be", "cp1252"])
    if kind == "utf8":
        return text.encode("utf-8")
    if kind == "utf8bom":
        return b"\xef\xbb\xbf" + text.encode("utf-8")
    if kind == "utf16le":
        return b"\xff\xfe" + text.encode("utf-16-le", "surrogatepass")
    if kind == "utf16be":
        return b"\xfe\xff" + text.encode("utf-16-be", "surrogatepass")
    try:
        return text.encode("cp1252")
    except UnicodeEncodeError:
        return text.encode("utf-8")


TOKEN = re.compile(r"(\u27e6/?[a-z]?\d+\u27e7)")


def risky(text):
    """A literal brace or backslash in the text (outside tokens): Bilingua writes those safely, LocOS verbatim."""
    plain = TOKEN.sub("", text)
    return "{" in plain or "\\" in plain


def mappings(units):
    pseudo, rev, drop, nlm, third = {}, {}, {}, {}, {}
    for i, t in enumerate(units):
        if risky(t):
            continue
        parts = TOKEN.split(t)
        pseudo[str(i)] = "\u017d" + "".join(p if TOKEN.fullmatch(p) else p.upper() for p in parts)
        rev[str(i)] = "".join(reversed([p for p in parts if p]))
        drop[str(i)] = TOKEN.sub("", t) + "!"
        nlm[str(i)] = "\u017d" + t.replace(" ", "\n", 1) + "\r\nend"
        if i % 3 == 0:
            third[str(i)] = "Zeta " + t
    return {"pseudo": pseudo, "reverse": rev, "drop": drop, "newline": nlm, "third": third}


def golden(data, units_only=False):
    f = get_filter("ass")
    rec = {"hex": data.hex()}
    try:
        units = [u.text for u in f.parse(data).units]
    except Exception as e:  # noqa: BLE001
        rec["error"] = type(e).__name__
        return rec
    rec["units"] = units
    if units_only:
        # LocOS flattens a Title line break only when the header lower-cases to "title" (not for a Turkish dotted
        # or dotless i); Bilingua always does. The units still have to agree.
        rec["units_only"] = True
        return rec
    maps = mappings(units)
    rec["maps"] = maps
    rec["builds"] = {name: f.build(data, m).hex() for name, m in maps.items()}
    rec["builds"]["empty"] = f.build(data, {}).hex()
    loc = {}
    for lc in LOCALES:
        for name in ("empty", "pseudo"):
            m = maps["pseudo"] if name == "pseudo" else {}
            loc[f"{lc}|{name}"] = f.build_localized(data, m, lc).hex()
    rec["localized"] = loc
    return rec


HOSTILE = [bytes.fromhex(h) for h in (
    "5b4576656e74735d0a00440a",
    "fffe5b00",
    "fffe00d85b00",
    "5b4576656e74735d0a8120",
    "efbbbf5b4576656e74735d0aff0a",
    "0102030405060708" * 20 + "e9",
    "",
    "0a0a0a",
)]


def main():
    out, extra = sys.argv[1], sys.argv[2:]
    rng = random.Random(1105)
    cases = []
    for i in range(260):
        data = make(rng)
        variant = any(v.encode(e) in data for v in ("Tİtle", "Tıtle") for e in ("utf-8", "utf-16-le", "utf-16-be") if True)
        cases.append({"name": f"random-{i}", **golden(data, variant)})
    for i, h in enumerate(HOSTILE):
        cases.append({"name": f"hostile-{i}", **golden(h)})
    for path in extra:
        with open(path, "rb") as fh:
            cases.append({"name": os.path.basename(path), **golden(fh.read())})
    with open(out, "w", encoding="utf-8") as fh:
        json.dump(cases, fh, separators=(",", ":"))
    print(len(cases), "cases,", sum(1 for c in cases if "error" in c), "refused")


if __name__ == "__main__":
    main()

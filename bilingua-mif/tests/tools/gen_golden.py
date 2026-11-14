"""Builds tests/data/locos_golden.json: random FrameMaker MIF files and what the LocOS mif filter makes of them.

Run with the Localization OS venv Python (read only use of its filters; nothing is written there):

    C:\\Users\\brent\\code\\local-tms\\.venv\\Scripts\\python.exe gen_golden.py OUT.json CORPUS_DIR [OKAPI_DIR]

CORPUS_DIR: every file in it becomes a case with its bytes (hex). OKAPI_DIR: Okapi's mif test resources; those
files are too big to commit, so a case holds only the file name, its sha1 and the results.
For each case: the units LocOS parses (style, text, the inline_map), or the refusal, and the SHA-1 and size of the
bytes LocOS builds with several mappings (every unit translated, every third, "tok" = own text + " Z", "rev" = the
pieces between tokens in reverse order, "dup" = text twice), plus build_localized for de-DE and ja.
Deterministic (seed 1104).
"""

import hashlib
import json
import os
import random
import re
import sys
import tempfile

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-mif-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import config  # noqa: E402
from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

config.MAX_DOCUMENT_SEGMENTS = 10_000_000

WORDS = [
    "Save", "Cancel", "Welcome back", "Press the button", "Caf\u00e9 au lait", "\u65e5\u672c\u8a9e\u306e\u30c6\u30ad\u30b9\u30c8",
    "\u0645\u0631\u062d\u0628\u0627 \u0628\u0627\u0644\u0639\u0627\u0644\u0645", "\ud55c\uad6d\uc5b4 \ud14d\uc2a4\ud2b8", "\u4e2d\u6587\u6587\u672c",
    "Quote `x' here", "a > b", "back\\slash", "Price 10 $", "Use <b> tags", "x", "---", "...", "100%", "Hello, world.",
    "\u00dcber den Wolken", "\u0420\u0443\u0441\u0441\u043a\u0438\u0439 \u0442\u0435\u043a\u0441\u0442", "emoji \U0001f600 ok",
]
STYLES = ["Body", "Heading1", "Heading2", "CellBody", "Footnote", "Bullet", "Note", ""]
CHARS = ["Tab", "HardSpace", "EmDash", "EnDash", "Bullet", "HardReturn", "SoftHyphen", "DiscHyphen", "NoHyphen", "Cent",
         "HardHyphen", "ThinSpace", "Dagger", "Mystery"]
FONT_TAGS = ["Emphasis", "Strong", "Code"]


def esc(s):
    out = []
    for c in s:
        if c == "\\":
            out.append("\\\\")
        elif c == ">":
            out.append("\\>")
        elif c == "'":
            out.append("\\q")
        elif c == "`":
            out.append("\\Q")
        elif c == "\t":
            out.append("\\t")
        else:
            out.append(c)
    return "".join(out)


def font(rng, kind, nl, ind):
    if kind == "bold":
        props = ["<FTag `'>", "<FWeight `Bold'>", "<FLocked No>"]
    elif kind == "regular":
        props = ["<FTag `'>", "<FWeight `Regular'>", "<FLocked No>"]
    elif kind == "default":
        props = ["<FTag `'>", "<FLocked No>"]
    elif kind == "tag":
        props = ["<FTag `%s'>" % rng.choice(FONT_TAGS), "<FLocked No>"]
    elif kind == "size":
        props = ["<FSize  %d.0 pt>" % rng.choice([10, 12, 14, 16])]
    elif kind == "italic":
        props = ["<FAngle `Italic'>"]
    else:  # roman
        props = ["<FAngle `Regular'>"]
    return ind + "<Font" + nl + "".join(ind + " " + p + nl for p in props) + ind + "> # end of Font"


def statement(rng, nl, ind):
    k = rng.randrange(6)
    if k == 0:
        return ind + "<Variable" + nl + ind + " <VariableName `Product'>" + nl + ind + "> # end of Variable"
    if k == 1:
        return ind + "<Marker" + nl + ind + " <MType 2>" + nl + ind + " <MText `idx'>" + nl + ind + ">"
    if k == 2:
        return ind + "<FNote 3>"
    if k == 3:
        return ind + "<ATbl 1>"
    if k == 4:
        return ind + "<Conditional" + nl + ind + " <InCondition `Draft'>" + nl + ind + ">"
    return ind + "<AFrame 7>"


def xref(rng, nl, ind):
    return [
        ind + "<XRef" + nl + ind + " <XRefName `Heading & Page'>" + nl + ind + "> # end of XRef",
        ind + "<String `Getting started on page 1'>",
        ind + "<XRefEnd >",
    ]


def line(rng, nl, ind, n_items):
    ci = ind + " "
    parts = []
    if rng.random() < 0.7:
        parts.append(ci + "<TextRectID %d>" % rng.randint(1, 9))
    fonts_open = []
    for _ in range(n_items):
        r = rng.random()
        if r < 0.45:
            parts.append(ci + "<String `%s'>" % esc(rng.choice(WORDS)))
        elif r < 0.55:
            parts.append(ci + "<Char %s>" % rng.choice(CHARS))
        elif r < 0.65:
            parts.append(statement(rng, nl, ci))
        elif r < 0.7:
            parts.extend(xref(rng, nl, ci))
        elif r < 0.78 and not fonts_open:
            kind = rng.choice(["bold", "tag", "italic", "size"])
            fonts_open.append(kind)
            parts.append(font(rng, kind, nl, ci))
        elif r < 0.9 and fonts_open:
            kind = fonts_open.pop()
            back = {"bold": "regular", "tag": "default", "italic": "roman", "size": "size"}[kind]
            parts.append(font(rng, back, nl, ci))
        else:
            parts.append(ci + "<String `%s'>" % esc(rng.choice(WORDS)))
    if rng.random() < 0.15:
        parts.append(ci + "<Char HardReturn>")
    return ind + "<ParaLine" + nl + nl.join(parts) + nl + ind + ("> # end of ParaLine" if rng.random() < 0.7 else ">")


def para(rng, nl, ind):
    n_lines = rng.choice([1, 1, 1, 2, 3])
    body = [ind + " <PgfTag `%s'>" % rng.choice(STYLES)]
    if rng.random() < 0.3:
        body.append(ind + " <Unique %d>" % rng.randint(100, 9999))
    if rng.random() < 0.15:
        body.append(ind + " <Pgf" + nl + ind + "  <PgfFont" + nl + ind + "   <FSize  16.0 pt>" + nl + ind + "  >" + nl + ind + " >")
    for j in range(n_lines):
        body.append(line(rng, nl, ind + " ", rng.randint(0, 9)))
        if j == 0 and n_lines > 1 and rng.random() < 0.3:
            body.append(ind + " <PgfNumString `1.'>")
    return ind + "<Para" + nl + nl.join(body) + nl + ind + "> # end of Para"


def make(rng):
    nl = rng.choice(["\n", "\n", "\r\n"])
    head = "<MIFFile %s>" % rng.choice(["2019", "9.00", "10.0", "8.00"]) + rng.choice([" # Generated", ""]) + nl
    out = [head, "<Units Ucm >" + nl]
    if rng.random() < 0.5:
        out.append(
            "<PgfCatalog" + nl + " <Pgf" + nl + "  <PgfTag `Body'>" + nl + "  <PgfLanguage USEnglish>" + nl
            + "  <PgfFont" + nl + "   <FWeight `Regular'>" + nl + "   <FSize  12.0 pt>" + nl + "   <FLanguage USEnglish>"
            + nl + "  >" + nl + " >" + nl + "> # end of PgfCatalog" + nl
        )
    if rng.random() < 0.4:
        out.append(
            "<AFrames" + nl + " <Frame" + nl + "  <ImportObject" + nl + "   <ImportObFileDI `<c\\\\>logo.png'>" + nl
            + "=FrameImage" + nl + "&%v" + nl + "&<MIF >weird data> `not a string" + nl + "=EndInset" + nl + "  >" + nl + " >" + nl + ">" + nl
        )
    out.append("<TextFlow" + nl + " <TFTag `A'>" + nl)
    for _ in range(rng.randint(0, 8)):
        out.append(para(rng, nl, " ") + nl)
    out.append("> # end of TextFlow" + nl)
    if rng.random() < 0.5:
        out.append("# End of MIFFile" + nl)
    text = "".join(out)
    data = text.encode("utf-8")
    if rng.random() < 0.15:
        data = b"\xef\xbb\xbf" + data
    return data


def bad(rng, data):
    """Damage a good file so LocOS must refuse it (or not): truncate, drop a close, add one."""
    k = rng.randrange(5)
    if k == 0:
        return data[: rng.randrange(1, max(2, len(data)))]
    if k == 1:
        i = data.find(b">", rng.randrange(max(1, len(data))))
        return data[:i] + data[i + 1 :] if i >= 0 else data
    if k == 2:
        i = rng.randrange(len(data))
        return data[:i] + b">" + data[i:]
    if k == 3:
        i = rng.randrange(len(data))
        return data[:i] + b"`" + data[i:]
    return data.replace(b"<MIFFile", b"<MIFFile", 1) + b"\n<"


def tokens_split(t):
    return re.split(r"(\u27e6/?[a-z]?\d+\u27e7)", t)


def mappings(texts):
    n = len(texts)
    out = {
        "all": {str(i): 'T%d "q" \u00e9\nline2' % i for i in range(n)},
        "third": {str(i): "Zeta %d \u65e5 `x' > \\" % i for i in range(0, n, 3)},
        "tok": {str(i): t + " Z" for i, t in enumerate(texts)},
        "dup": {str(i): t + " " + t for i, t in enumerate(texts)},
    }
    rev = {}
    for i, t in enumerate(texts):
        parts = tokens_split(t)
        rev[str(i)] = "".join(reversed(parts))
    out["rev"] = rev
    return out


def sha(b):
    return hashlib.sha1(b).hexdigest()


def run_case(f, name, data, keep_hex):
    case = {"name": name, "sha1": sha(data)}
    if keep_hex:
        case["hex"] = data.hex()
    try:
        pr = f.parse(data)
    except Exception as e:  # noqa: BLE001
        case["error"] = type(e).__name__
        case["message"] = str(e)[:200]
        return case
    texts = [u.text for u in pr.units]
    case["units"] = [[u.style, u.text] for u in pr.units]
    case["maps"] = [dict(u.inline_map) for u in pr.units]
    case["skipped"] = pr.skeleton.get("skipped_marker_paragraphs", 0)
    case["version"] = pr.skeleton["mif_version"]
    builds = {}
    for k, m in mappings(texts).items():
        try:
            builds[k] = [sha(f.build(data, m)), len(f.build(data, m))]
        except Exception as e:  # noqa: BLE001
            builds[k] = type(e).__name__
    third = mappings(texts)["third"]
    for loc in ("de-DE", "ja", "he"):
        out = f.build_localized(data, third, loc)
        builds["loc-" + loc] = [sha(out), len(out)]
    out = f.build_localized(data, {}, "de-DE")
    builds["loc-empty"] = [sha(out), len(out)]
    case["builds"] = builds
    return case


def main():
    out_path, corpus = sys.argv[1], sys.argv[2]
    okapi = sys.argv[3] if len(sys.argv) > 3 else None
    f = get_filter("mif")
    cases = []
    for name in sorted(os.listdir(corpus)):
        p = os.path.join(corpus, name)
        if os.path.isfile(p):
            cases.append(run_case(f, name, open(p, "rb").read(), True))
    if okapi:
        for name in sorted(os.listdir(okapi)):
            if name.endswith(".mif"):
                c = run_case(f, name, open(os.path.join(okapi, name), "rb").read(), False)
                c["okapi"] = True
                c.pop("maps", None)
                cases.append(c)
    rng = random.Random(1104)
    for i in range(300):
        data = make(rng)
        if i % 5 == 4:
            data = bad(rng, data)
        cases.append(run_case(f, "random-%03d" % i, data, True))
    with open(out_path, "w", encoding="utf-8") as fh:
        json.dump(cases, fh)
    ok = sum(1 for c in cases if "error" not in c)
    print(f"{len(cases)} cases, {ok} parsed, {len(cases) - ok} refused")


main()

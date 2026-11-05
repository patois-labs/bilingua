"""Builds tests/data/locos_golden.json: random reStructuredText and what the LocOS rst filter makes of it.

Run with the Localization OS venv Python (read only use of its filters; nothing is written there):

    C:\\Users\\brent\\code\\local-tms\\.venv\\Scripts\\python.exe gen_golden.py OUT.json [FILE ...]

For each input the golden holds the bytes (hex), the unit texts and styles LocOS parses (or the error class), the
tables it skipped, and the bytes LocOS builds (length:FNV-1a-64 digests, or the refusal message) with several
mappings of the unit texts (see MAPS). Deterministic (seed 1105). The Rust test applies the same mappings.
"""

import json
import os
import random
import sys
import tempfile

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-rst-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

F = get_filter("rst")


def fnv(b: bytes) -> str:
    h = 0xCBF29CE484222325
    for x in b:
        h = ((h ^ x) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{len(b)}:{h:016x}"


def map_text(name: str, i: int, t: str):
    if name == "echo":
        return t
    if name == "mark":
        return "~" + t + "!"
    if name == "all":
        return f"T{i} *em* a_ b| `q` \\ \u00e9\n\u65e5\u672c\u8a9e"
    if name == "third":
        return f"Zeta {i} \u65e5" if i % 3 == 0 else None
    if name == "cjk":
        return "\u65e5\u672c" + t.replace(" ", "") + "\u3067\u3059"
    if name == "long":
        return t + " " + t
    if name == "short":
        return "Kurz" if i % 2 == 0 else None
    if name == "dash":
        return "- " + t if i % 2 == 1 else None
    raise ValueError(name)


MAPS = ["echo", "mark", "all", "third", "cjk", "long", "short", "dash"]


def run_case(data: bytes) -> dict:
    out: dict = {"hex": data.hex()}
    try:
        r = F.parse(data)
    except Exception as e:  # noqa: BLE001
        out["error"] = type(e).__name__
        return out
    texts = [u.text for u in r.units]
    out["units"] = texts
    out["styles"] = [u.style for u in r.units]
    sk = r.skeleton.get("rst_skipped_tables") or []
    out["skipped"] = [[s["line"], s["reason"]] for s in sk]
    builds = {}
    for m in MAPS:
        mp = {str(i): v for i, t in enumerate(texts) if (v := map_text(m, i, t)) is not None}
        try:
            builds[m] = fnv(F.build(data, mp))
        except Exception as e:  # noqa: BLE001
            builds[m] = {"err": type(e).__name__, "msg": str(e)}
    out["builds"] = builds
    return out


# ------------------------------------------------------------------------------------------ generators

WORDS = ["the", "quick", "brown", "fox", "Install", "Run", "make", "file", "Caf\u00e9", "na\u00efve", "x", "A", "2024", "i", "v"]
CJK = ["\u65e5\u672c\u8a9e", "\u4e2d\u6587\u6807\u9898", "\ud55c\uad6d\uc5b4", "\u0627\u0644\u0639\u0631\u0628\u064a\u0629", "e\u0301t\u00e9", "\u0939\u093f\u0928\u094d\u0926\u0940"]
ATOMS = [
    "*em*", "**strong**", "``lit``", "``a b``", ":ref:`label <x>`", ":ref:`x`", ":py:class:`Foo`", "`ref`_", "`ref`__", "`two words`_",
    "word_", "word__", "snake_case_", "|sub|", "|sub|_", "[1]_", "[#]_", "[*]_", "[#note]_", "[note]_", "_`target`",
    "`Docs <https://e.com>`_", "`Docs <https://e.com>`__", "`<https://e.com>`_", "`1 <https://e.com>`_", "https://example.com/a?b=1.",
    "mailto:a@b.c,", "ftp://x.y/z)", "http://", "\\*", "\\\\", "\\ ", "a\\", "`x`:role:", "`x`:py:class:", ":a:b:`x`", "*", "**", "`", "``",
    "|", "_", "__", "<", ">", "[", "]", ".", ",", "(", ")", "-", ":", "::", "\u2022", "1.", "- ", "* ", "...", "\"", "'", "\u3002", "\u3001",
    "*a **b** c*", "**a *b* c**", "***x***", "** x**", "* x*", "`a``b`", "``a`b``", "x*y*z", "a*b", "2*3*4", "a_b_c", "(*em*)", "\"*em*\"",
    "\u00e9*em*", "\u65e5*em*\u672c", "http://a.b/c`d", "<http://a.b>", "`a <b>`_", "`a <b>`", "`<b>`_",
]
JOIN = ["", " ", " ", " ", "  ", "\n", "\n  ", " \n", "\t"]


def rnd_text(rng, n=None):
    parts = []
    for _ in range(n or rng.randint(1, 8)):
        r = rng.random()
        if r < 0.45:
            parts.append(rng.choice(WORDS))
        elif r < 0.55:
            parts.append(rng.choice(CJK))
        else:
            parts.append(rng.choice(ATOMS))
    s = parts[0]
    for p in parts[1:]:
        s += rng.choice(JOIN) + p
    return s


def rnd_line(rng):
    return rnd_text(rng).replace("\n", " ")


def ind(block: list, n: int) -> list:
    return [(" " * n + ln) if ln.strip() else ln for ln in block]


def title(rng):
    t = rnd_line(rng)
    ch = rng.choice("=-~^\"#*+")
    n = len(t) + rng.choice([0, 0, 0, 1, 3, -1, -3, 8])
    n = max(1, n)
    if rng.random() < 0.3:
        return [ch * n, " " + t if rng.random() < 0.3 else t, ch * n]
    return [t, ch * n]


def para(rng):
    lines = [rnd_text(rng) for _ in range(rng.randint(1, 3))]
    s = "\n".join(lines)
    if rng.random() < 0.15:
        s += rng.choice(["::", " ::", ":"])
        return s.split("\n") + [""] + rng.choice([["   literal text", "   more"], ["> quoted", "> more"], ["   x"], []])
    return s.split("\n")


def bullet(rng):
    out = []
    for _ in range(rng.randint(1, 3)):
        mark = rng.choice(["-", "*", "+", "\u2022", "1.", "#.", "a)", "(i)", "iv.", "A.", "(1)", "2)"])
        body = para(rng)[:3]
        out.append(mark + " " + body[0])
        out += ind(body[1:], len(mark) + 1)
        if rng.random() < 0.3:
            out += ["", " " * (len(mark) + 1) + rnd_line(rng)]
    return out


def deflist(rng):
    return [rnd_line(rng), "   " + rnd_text(rng).replace("\n", " "), "", rnd_line(rng), "   " + rnd_line(rng)]


def fieldlist(rng):
    return [":Author: " + rnd_line(rng), ":Version: 1.0", ":a b:", "    " + rnd_line(rng)]


def optionlist(rng):
    return rng.choice([["-v  " + rnd_line(rng)], ["--long=VAL  " + rnd_line(rng), "-q, --quiet  " + rnd_line(rng)], ["--opt", "    " + rnd_line(rng)], ["/V  " + rnd_line(rng)]])


def lineblock(rng):
    return ["| " + rnd_line(rng) for _ in range(rng.randint(1, 3))] + (["|"] if rng.random() < 0.2 else [])


def directive(rng):
    name = rng.choice(
        ["note", "warning", "tip", "admonition", "versionadded", "versionchanged", "deprecated", "code-block", "code", "list-table", "table",
         "figure", "image", "include", "raw", "meta", "container", "rubric", "topic", "sidebar", "py:function", "unknown", "seealso", "csv-table",
         "only", "math", "literalinclude", "centered", "epigraph", "NOTE"]
    )
    arg = rng.choice(["", rnd_line(rng), "2.1", "python", "2.1 " + rnd_line(rng), "img.png"])
    out = [".. " + name + "::" + ((" " + arg) if arg else "")]
    if name == "meta":
        out += ["   :description: " + rnd_line(rng), "   :keywords: " + rnd_line(rng), "   :robots: noindex", "   :Keywords:", "      " + rnd_line(rng)]
        return out
    if rng.random() < 0.5:
        out.append("   :class: big")
    if name == "list-table":
        out += ["", "   * - " + rnd_line(rng), "     - " + rnd_line(rng), "   * - " + rnd_line(rng), "     - 12"]
        return out
    if rng.random() < 0.8:
        body = para(rng)[:3]
        out += [""] + ind(body, 3)
        if rng.random() < 0.3:
            out += ["", "   - " + rnd_line(rng)]
    return out


def misc(rng):
    return rng.choice(
        [
            [".. " + rnd_line(rng)],
            [".. _tgt: http://example.com", ""],
            [".. |s| replace:: " + rnd_line(rng)],
            [".. [1] " + rnd_line(rng)],
            [".. [#note] " + rnd_line(rng), "   " + rnd_line(rng)],
            [".. [CIT2020] " + rnd_line(rng)],
            ["..", "   " + rnd_line(rng)],
            ["__ http://anon.example.com"],
            ["----"],
            ["-----------"],
            [">>> print(1)", "1"],
            ["    " + rnd_line(rng), "    " + rnd_line(rng)],
            ["", ""],
            ["\u3000", "x"],
        ]
    )


def cell_text(rng):
    return rng.choice(["", "A", "Name", "Cell one", "two words", "\u65e5\u672c\u8a9e", "Longer cell text here", "*em* and ``code``", "x", "e\u0301t\u00e9", "- item", "1. one", "..", "| a"])


def grid(rng):
    ncol, nrow = rng.randint(1, 4), rng.randint(1, 4)
    widths = [rng.randint(5, 16) for _ in range(ncol)]
    sep = "+" + "+".join("-" * w for w in widths) + "+"
    hsep = sep.replace("-", "=")
    out = [sep]
    for r in range(nrow):
        nl = rng.choice([1, 1, 1, 2])
        for k in range(nl):
            cells = []
            for w in widths:
                t = cell_text(rng) if k == 0 else rng.choice(["", "more"])
                fill = (" " + t)[: w]
                cells.append(fill + " " * (w - sum(2 if ord(c) > 0x2e80 else 1 for c in fill if ord(c) != 0x301)))
            out.append("|" + "|".join(cells) + "|")
        out.append(hsep if r == 0 and rng.random() < 0.5 else sep)
    if rng.random() < 0.25:  # a spanning cell
        out = [out[0], "| " + cell_text(rng).ljust(sum(widths) + ncol - 3) + " |"] + [out[0].replace("+", "-", 0)]
        out[2] = out[0]
    if rng.random() < 0.15:
        i = rng.randrange(len(out))
        out[i] = out[i][:-1]  # break it
    return out


def simple(rng):
    ncol = rng.randint(2, 4)
    widths = [rng.randint(4, 12) for _ in range(ncol)]
    border = "  ".join("=" * w for w in widths)
    out = [border]
    for r in range(rng.randint(1, 4)):
        cells = [cell_text(rng) for _ in widths]
        line = ""
        for c, (t, w) in enumerate(zip(cells, widths)):
            line = line.ljust(sum(widths[:c]) + 2 * c)
            line += t[:w] if c < ncol - 1 else t
        out.append(line.rstrip())
        if rng.random() < 0.25:
            out.append(" " * (widths[0] + 2) + "continued")
        if r == 0 and rng.random() < 0.5:
            out.append(border)
        if rng.random() < 0.15:
            out.append("")
    out.append(border)
    if rng.random() < 0.1:
        out[-1] = ""
    return out


def quote(rng):
    return ind(para(rng)[:3], rng.choice([1, 2, 3, 4]))


GEN = [title, title, para, para, para, para, bullet, bullet, deflist, fieldlist, optionlist, lineblock, directive, directive, directive, misc, misc,
       grid, grid, simple, simple, quote]


def document(rng) -> str:
    blocks = []
    for _ in range(rng.randint(1, 9)):
        b = rng.choice(GEN)(rng)
        blocks.append("\n".join(b))
    sep = rng.choice(["\n\n", "\n\n", "\n\n", "\n\n\n", "\n"])
    return sep.join(blocks) + rng.choice(["\n", "\n", "", "\n\n"])


def mutate(rng, s: str) -> str:
    for _ in range(rng.randint(1, 4)):
        if not s:
            break
        i = rng.randrange(len(s))
        r = rng.random()
        if r < 0.3:
            s = s[:i] + s[i + 1 :]
        elif r < 0.5:
            s = s[:i] + rng.choice(["*", "`", "|", "_", ":", "\n", " ", "=", "-", "+", "\\", "::", "..", "\u65e5"]) + s[i:]
        elif r < 0.7:
            lines = s.split("\n")
            j = rng.randrange(len(lines))
            del lines[j]
            s = "\n".join(lines)
        elif r < 0.85:
            lines = s.split("\n")
            j = rng.randrange(len(lines))
            lines.insert(j, lines[j])
            s = "\n".join(lines)
        else:
            s = s[:i] + s[i + 1 :]
    return s


def main():
    out_path = sys.argv[1]
    rng = random.Random(1105)
    cases = []
    for fp in sys.argv[2:]:
        with open(fp, "rb") as fh:
            d = fh.read()
        c = run_case(d)
        c["name"] = "file:" + os.path.basename(fp)
        cases.append(c)
    # inline fuzz: one paragraph, plus a title and a list item holding the same text
    for n in range(1500):
        t = rnd_text(rng, rng.randint(1, 14))
        kind = n % 3
        if kind == 0:
            s = t + "\n"
        elif kind == 1:
            s = "- " + t.replace("\n", "\n  ") + "\n"
        else:
            s = "Para " + t + "\n\nSecond.\n"
        c = run_case(s.encode("utf-8"))
        c["name"] = f"inline-{n}"
        cases.append(c)
    for n in range(900):
        s = document(rng)
        if rng.random() < 0.25:
            s = mutate(rng, s)
        data = s.encode("utf-8")
        r = rng.random()
        if r < 0.12:
            data = data.replace(b"\n", b"\r\n")
        elif r < 0.2:
            data = b"\xef\xbb\xbf" + data
        c = run_case(data)
        c["name"] = f"doc-{n}"
        cases.append(c)
    # specials: depth limits, token cap, long spans, tabs, unusual whitespace, wide tables
    sp = {}
    sp["deep-bullets"] = "".join("  " * k + "- item %d\n\n" % k for k in range(46))
    sp["deep-quotes"] = "".join(" " * (k + 1) + "text %d\n\n" % k for k in range(46))
    sp["deep-mixed"] = "".join("   " * k + ".. note:: n%d\n\n" % k for k in range(45)) + "   " * 45 + "end\n"
    sp["many-tokens"] = " ".join("``c%d``" % k for k in range(1100)) + "\n"
    sp["many-pairs"] = " ".join("*e%d*" % k for k in range(1100)) + "\n"
    sp["nested-pairs"] = "**a *b **c *d* c** b* a**\n"
    sp["long-em"] = "*" + "x" * 2000 + "*\n\n*" + "x" * 2001 + "*\n\n``" + "y" * 1999 + "``\n\n``" + "y" * 2000 + "``\n"
    sp["tabs"] = "Title\n=====\n\n\tindented by a tab\n\n- item\n\tcontinued\n\n:f:\tvalue\n"
    sp["unicode-ws"] = "a\u3000b\u00a0c\n\u3000\nnext\u2028line\n\n\u00a0\u00a0indent?\n"
    sp["wide-title"] = "\u65e5\u672c\u8a9e\u30bf\u30a4\u30c8\u30eb\n=======\n\ne\u0301t\u00e9\n====\n\n\uff21\uff22\uff23\n------\n"
    sp["noeol"] = "last line without newline"
    sp["empty"] = ""
    sp["only-blank"] = "\n\n   \n"
    sp["crlf-table"] = "+----+----+\r\n| a  | b  |\r\n+----+----+\r\n\r\n=====  =====\r\nx      y\r\n=====  =====\r\n"
    sp["cr-only"] = "Title\r=====\r\rPara.\r"
    for name, body in sp.items():
        c = run_case(body.encode("utf-8"))
        c["name"] = "special-" + name
        cases.append(c)
    with open(out_path, "w", encoding="utf-8", newline="\n") as fh:
        json.dump(cases, fh, separators=(",", ":"))
    errs = sum(1 for c in cases for b in c.get("builds", {}).values() if isinstance(b, dict))
    print(len(cases), "cases,", errs, "refused builds")


main()

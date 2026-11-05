"""Builds tests/data/locos_golden.json: random AsciiDoc files and what the LocOS adoc filter makes of them.

Run with the Localization OS venv Python (read only use of its filters; nothing is written there):

    C:\\Users\\brent\\code\\local-tms\\.venv\\Scripts\\python.exe gen_golden.py OUT.json [CORPUS_FILE ...]

For each file the golden holds the input bytes (hex), the unit texts, styles and inline maps LocOS parses (or the
error class), and the length:FNV-1a-64 digests of the bytes LocOS builds with several mappings and localized builds.
Deterministic (seed 1105).
"""

import json
import os
import random
import re
import sys
import tempfile

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-adoc-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import config  # noqa: E402
from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

config.MAX_DOCUMENT_SEGMENTS = 10_000_000

WORDS = ["Open", "the", "dashboard", "and", "click", "New", "project", "Caf\u00e9", "install", "guide", "Windows", "macOS",
         "2026", "a", "of", "to", "x", "\u65e5\u672c\u8a9e", "\u0645\u0631\u062d\u0628\u0627", "Hello", "world."]
MARKS = [
    "*bold*", "_it_", "#mark#", "`code`", "``dcode``", "+pass+", "++pp++", "+++raw+++", "**ub**", "__ui__", "##um##",
    "^sup^", "~sub~", "{attr}", "{set:foo:bar}", "{counter:n}", "\\{esc}", "\\*not*", "link:a.html[text]", "link:a.html[]",
    "xref:b.adoc[Section B^]", "https://example.com", "https://example.com[Example site]", "https://example.com[site^]",
    "mailto:a@b.co[Mail me]", "footnote:[A note.]", "image:x.png[Alt]", "kbd:[Ctrl+C]", "btn:[OK]", "menu:File[Open]",
    "<<anchor,see here>>", "<<anchor>>", "[[id1]]", "[.role]#styled#", "[.big]*bold role*", "pass:q[<b>x</b>]", "stem:[x^2]",
    "icon:tip[]", "indexterm:[term]", "http://a.org,", "irc://x.y/z", "a +\ncontinued", "\\| pipe", "a | b", "100%", "%s", "{0}",
    "file:///tmp/x[F]", "footnoteref:[id,text]", "link:u[a=b]", "link:u[\"q\"]", "*a* b*", "_a_b_", "H~2~O", "x^2^", "**", "``",
]


def para(r, n):
    out = []
    for _ in range(n):
        out.append(r.choice(MARKS) if r.random() < 0.3 else r.choice(WORDS))
    return " ".join(out)


BLOCKS = [
    lambda r: "= " + para(r, 3) + r.choice(["", " =", " =="]),
    lambda r: "== " + para(r, 3) + r.choice(["", " ==", " =="]),
    lambda r: "### " + para(r, 2),
    lambda r: para(r, r.randint(2, 12)),
    lambda r: para(r, r.randint(2, 6)) + "\n" + para(r, r.randint(2, 6)) + "\n" + para(r, 3),
    lambda r: "NOTE: " + para(r, 5),
    lambda r: "WARNING: " + para(r, 5) + "\ncontinued " + para(r, 3),
    lambda r: ".Block title " + para(r, 2) + "\n" + para(r, 5),
    lambda r: "\n".join(r.choice(["* ", "** ", "- ", ". ", ".. ", "1. ", "a. ", "ii) ", "<1> ", "* [x] ", "* [ ] "]) + para(r, 4) for _ in range(r.randint(1, 5))),
    lambda r: "CPU:: " + para(r, 4) + "\nHard drive::\n" + para(r, 4) + "\nterm:::  def " + para(r, 2) + "\nq;; " + para(r, 2),
    lambda r: "std::vector is " + para(r, 3),
    lambda r: "[source,python]\n----\nprint('x')  # *not* a mark\n----",
    lambda r: "----\ncode " + para(r, 3) + "\n----",
    lambda r: "....\nliteral\n....",
    lambda r: "++++\n<p>raw</p>\n++++",
    lambda r: "////\ncomment " + para(r, 3) + "\n////",
    lambda r: "```js\nfence\n```",
    lambda r: "[source]\n" + para(r, 3),
    lambda r: "[NOTE]\n====\n" + para(r, 5) + "\n====",
    lambda r: "====\n" + para(r, 5) + "\n\n" + para(r, 4) + "\n====",
    lambda r: "****\n" + para(r, 5) + "\n****",
    lambda r: "[quote]\n____\n" + para(r, 5) + "\n____",
    lambda r: "--\n" + para(r, 5) + "\n--",
    lambda r: "[listing]\n====\n" + para(r, 3) + "\n====",
    lambda r: "include::partials/x.adoc[]",
    lambda r: "ifdef::env[]\n" + para(r, 4) + "\nendif::[]",
    lambda r: "ifeval::[{n} > 1]\n" + para(r, 3) + "\nendif::[]",
    lambda r: ":attr: " + para(r, 3),
    lambda r: ":uri-x: https://x.example.org",
    lambda r: ":long: first \\\n  second \\\n  third",
    lambda r: "// " + para(r, 3),
    lambda r: "image::pic.png[" + para(r, 2) + "]",
    lambda r: "[cols=\"1,2\"]\n|===\n|Name |Description\n\n|" + para(r, 1) + "\n|" + para(r, 4) + " \\| x\n\n2+^.^|" + para(r, 2) + "\na|" + para(r, 3) + " *b*\nl|literal cell\n|===",
    lambda r: "|===\n|A|B|C\n|" + para(r, 1) + "|" + para(r, 1) + "|" + para(r, 2) + "\n.2+|span " + para(r, 1) + "|x\n3*|dup\n|===",
    lambda r: "|===\na|\n!===\n!" + para(r, 2) + "!" + para(r, 2) + "\n!===\n|===",
    lambda r: "[format=csv]\n|===\ncsv,cell\n|===",
    lambda r: ",===\na,b\n,===",
    lambda r: "'''",
    lambda r: "<<<",
    lambda r: "* * *",
    lambda r: "[[anchor-" + str(r.randint(1, 9)) + "]]",
    lambda r: "[.role]\n" + para(r, 4),
    lambda r: " indented literal " + para(r, 3),
    lambda r: para(r, 2) + "\n" + r.choice(["======", "------", "~~~~~~", "^^^^^^", "++++++", "==", "---", "-"]),
    lambda r: "Two Line Title\n--------------\n" + para(r, 3),
    lambda r: ":description: Learn " + para(r, 3) + "\n:keywords: " + para(r, 2),
    lambda r: ":lang: en",
    lambda r: "Jane Doe <jane@example.com>\nv1.2, 2026-10-01",
    lambda r: "+\n" + para(r, 2),
    lambda r: "\\" + r.choice(["include::x[]", "ifdef::y[]"]),
    lambda r: "== " + r.choice(MARKS) + " title",
    lambda r: "* " + " ".join(r.choice(MARKS) for _ in range(4)),
]


def make(r):
    parts = []
    if r.random() < 0.3:
        parts.append("// leading comment")
    if r.random() < 0.5:
        parts.append(BLOCKS[0](r))
        if r.random() < 0.6:
            parts[-1] += (
                "\nAuthor Name <a@b.c>\n:description: Doc about " + para(r, 3)
                + r.choice(["", " \\\n  wrapped " + para(r, 2)]) + "\n:keywords: " + para(r, 2) + "\n:toc:"
            )
    for _ in range(r.randint(1, 14)):
        parts.append(r.choice(BLOCKS)(r))

    def sep():
        return r.choice(["\n\n", "\n\n", "\n\n", "\n", "\n\n\n"])

    text = parts[0]
    for p in parts[1:]:
        text += sep() + p
    text += r.choice(["\n", "\n", "", "\n\n"])
    nl = r.choice(["\n", "\n", "\n", "\r\n", "\r"])
    text = text.replace("\n", nl)
    if r.random() < 0.3:  # structural mutation
        chars = list(text)
        for _ in range(r.randint(1, 4)):
            if not chars:
                break
            p = r.randrange(len(chars))
            op = r.choice(["del", "ins", "dup"])
            if op == "del":
                del chars[p : p + r.randint(1, 6)]
            elif op == "ins":
                chars[p:p] = list(r.choice(["*", "_", "`", "[", "]", "{", "}", "<<", ">>", "----", "|===", "\n", " +\n", "::", "\\", "link:", "https://", "====", "//"]))
            else:
                chars[p:p] = chars[p : p + r.randint(1, 30)]
        text = "".join(chars)
    data = text.encode("utf-8")
    if r.random() < 0.15:
        data = b"\xef\xbb\xbf" + data
    return data


TOK = re.compile("\u27e6/?[a-z]?\\d+\u27e7")
MAPS = {
    "all": lambda i, t: "FR " + t,
    "third": lambda i, t: "Zeta %d \u65e5" % i if i % 3 == 0 else None,
    "plain": lambda i, t: "Plain%d" % i,
    "echo": lambda i, t: t,
    "cjk": lambda i, t: "".join("\u3042" + x for x in TOK.findall(t)) + "\u3002" if TOK.search(t) else "\u65e5\u672c\u8a9e%d" % i,
    "zh": lambda i, t: "\u8bf7" + re.sub(r"\s+", "", t) + "\u3002",
    "sep": lambda i, t: "A | B , C : D ! " + t + "\\",
    "block": lambda i, t: ["== X ", "* X ", "// x ", ".T ", "NOTE: x ", "[x] ", "-- ", "image::x[] ", "|=== ", "term:: ", "X "][i % 11] + t,
    "space": lambda i, t: "  " + t.replace(" ", "  ") + "\n\nend   \n",
    "lead": lambda i, t: " \t" + t + " + \\",
}
LOCALES = ["de-DE", "ja-JP", "ar-SA", "zh-Hant", "fr", "pt_BR", "bad value"]


def digest(b):
    h = 0xCBF29CE484222325
    for x in b:
        h = ((h ^ x) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return "%d:%016x" % (len(b), h)


def golden(data):
    f = get_filter("adoc")
    rec = {"hex": data.hex()}
    try:
        res = f.parse(data)
    except Exception as e:  # noqa: BLE001
        rec["error"] = type(e).__name__
        return rec
    units = [u.text for u in res.units]
    rec["units"] = units
    rec["styles"] = [u.style for u in res.units]
    rec["maps"] = [u.inline_map for u in res.units]
    builds = {}
    for name, fn in MAPS.items():
        m = {}
        for i, t in enumerate(units):
            v = fn(i, t)
            if v is not None:
                m[str(i)] = v
        builds[name] = digest(f.build(data, m))
    rec["builds"] = builds
    loc = {}
    if units:
        for k, lc in enumerate(LOCALES):
            name = ["all", "cjk", "plain", "zh", "block", "echo", "all"][k]
            m = {str(i): MAPS[name](i, t) for i, t in enumerate(units) if MAPS[name](i, t) is not None}
            loc[lc] = digest(f.build_localized(data, m, lc))
            loc[lc + "|map"] = name
    rec["localized"] = loc
    return rec


HOSTILE = [bytes.fromhex(h) for h in ("2d2d2d2d2d424547494e2050475020", "00010203", "efbbbfff", "ff", "")] + [
    b"*" * 3000 + b"\n",
    b"{" * 3000 + b"\n",
    b"|===\n" + b"|" * 3000 + b"\n|===\n",
    b"----\nnever closed\n" * 50,
]


def main():
    out, extra = sys.argv[1], sys.argv[2:]
    r = random.Random(1105)
    cases = []
    for i in range(600):
        cases.append({"name": f"random-{i}", **golden(make(r))})
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

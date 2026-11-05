"""Builds tests/data/locos_golden.json: random wiki pages and what the LocOS wiki filters make of them.

Run with the Localization OS venv Python (read only use of its filters; nothing is written there):

    C:\\Users\\brent\\code\\local-tms\\.venv\\Scripts\\python.exe gen_golden.py OUT.json [CORPUS_FILE ...]

For each file the golden holds the dialect, the template allow-list in force, the input bytes (hex), the unit texts
LocOS parses (or the error class), and the bytes LocOS builds (length:FNV-1a-64 digests) with several mappings.
Deterministic (seed 1105).
"""

import json
import os
import random
import sys
import tempfile

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-wiki-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import config  # noqa: E402
from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

WORDS = [
    "Paris", "capital", "museum", "the", "of", "and", "river", "Caf\u00e9", "cr\u00e8me", "Zo\u00eb", "\u65e5\u672c\u8a9e", "\u4e2d\u6587\u6587\u672c",
    "\ud55c\uad6d\uc5b4", "\u0645\u0631\u062d\u0628\u0627", "bank", "hello", "world", "N\u00fcrnberg", "100", "x", "item", "text", "page",
]
MW_INLINE = [
    "'''bold'''", "''it''", "'''''both'''''", "[[Page]]", "[[Page|label text]]", "[[:Category:X|cat]]", "[[File:a.png|thumb|Caption [[b|n]] here]]",
    "[[File:a.png|thumb|200px]]", "[[Help:Contents|]]", "[https://x.org label words]", "[https://x.org]", "https://example.com/a?b=1", "{{tpl|a=b}}",
    "{{{1|d}}}", "{{#if:x|y|z}}", "<ref>Note text</ref>", "<ref name=a />", "<nowiki>[[x]]</nowiki>", "<!-- c -->", "&nbsp;", "&#169;", "<br/>",
    '<span style="x">t</span>', "__NOTOC__", "~~~~", "[[Category:Z]]", "%s", "{0}", "{{Quote|author=A|text=To be}}", "{{note|Remember this}}",
    "'''b ''i'' b'''", "[[a|[[b|c]] d]]", "<code>x()</code>", "<math>x^2</math>", "[mailto:a@b.c mail me]", "&bogus;",
]
DW_INLINE = [
    "**bold**", "//it//", "__u__", "''mono''", "[[ns:page|label text]]", "[[page]]", "{{img.png?20|Cap words}}", "{{img.png}}", "((footnote text))",
    "https://x.org/a", "<a@b.com>", "\\\\ ", "%%raw **x**%%", "<nowiki>x</nowiki>", "<code>c</code>", "~~NOTOC~~", "<del>t</del>", "<sub>x</sub>",
    "**__//''all''//__**", "[[http://x.org|site]]", "{{rss>http://x.org/f 5|Feed}}", "%s", "{0}",
]
ALLOW = ["", "Quote:text", "Note:1, Infobox_city:caption, Template:quote:text", "Tpl:a"]


def words(rng, lo=2, hi=7):
    return " ".join(rng.choice(WORDS) for _ in range(rng.randint(lo, hi)))


def sentence(rng, pool):
    parts = []
    for _ in range(rng.randint(1, 4)):
        parts.append(words(rng, 1, 4))
        if rng.random() < 0.6:
            parts.append(rng.choice(pool))
    return " ".join(parts) + rng.choice([".", "", "!", " ..."])


def mw_page(rng):
    out = []
    for _ in range(rng.randint(2, 14)):
        r = rng.random()
        if r < 0.1:
            n = rng.randint(1, 4)
            out.append("=" * n + " " + words(rng, 1, 4) + " " + rng.choice(["", "<!-- n -->", "{{tpl}}"]) + " " + "=" * n)
        elif r < 0.35:
            for _ in range(rng.randint(1, 3)):
                out.append(sentence(rng, MW_INLINE))
        elif r < 0.5:
            for _ in range(rng.randint(1, 3)):
                out.append(rng.choice(["*", "**", "#", "##", ":", "*#"]) + " " + sentence(rng, MW_INLINE))
        elif r < 0.55:
            out.append("; " + words(rng, 1, 3) + rng.choice([" : ", ":", " :[http://x.y z] "]) + sentence(rng, MW_INLINE))
        elif r < 0.7:
            out.append('{| class="wikitable"')
            if rng.random() < 0.4:
                out.append("|+ " + words(rng))
            for _ in range(rng.randint(1, 3)):
                out.append("|-")
                out.append(rng.choice(["! ", "| ", '| style="a" | ']) + " || ".join(sentence(rng, MW_INLINE[:12]) for _ in range(rng.randint(1, 3))))
                if rng.random() < 0.3:
                    out.append("! " + words(rng, 1, 2) + " !! " + words(rng, 1, 2))
            out.append("|}")
        elif r < 0.75:
            out.append("<!-- " + words(rng) + " -->")
        elif r < 0.8:
            out.append("{{Infobox city\n| name = " + words(rng, 1, 2) + "\n| caption = " + words(rng) + "\n}}")
        elif r < 0.84:
            out.append(rng.choice(["<pre>\n" + words(rng) + "\n</pre>", '<syntaxhighlight lang="x">\ncode\n</syntaxhighlight>', "<nowiki>" + words(rng) + "</nowiki>"]))
        elif r < 0.88:
            out.append(rng.choice([" " + words(rng), "#REDIRECT [[" + words(rng, 1, 2) + "]]", "[[Category:" + words(rng, 1, 2) + "]]", "----", "---- " + words(rng, 1, 2), "__TOC__"]))
        elif r < 0.93:
            out.append("{{Quote|author=A|text=" + words(rng) + "}}")
        else:
            out.append("")
        if rng.random() < 0.3:
            out.append("")
    return "\n".join(out) + rng.choice(["\n", "", "\n\n"])


def dw_page(rng):
    out = []
    for _ in range(rng.randint(2, 14)):
        r = rng.random()
        if r < 0.1:
            n = rng.randint(1, 6)
            out.append("=" * n + " " + words(rng, 1, 4) + " " + "=" * n)
        elif r < 0.35:
            for _ in range(rng.randint(1, 3)):
                out.append(sentence(rng, DW_INLINE))
        elif r < 0.5:
            for _ in range(rng.randint(1, 3)):
                out.append(rng.choice(["  * ", "  - ", "    * ", "\t* "]) + sentence(rng, DW_INLINE))
        elif r < 0.65:
            out.append("^ " + " ^ ".join(words(rng, 1, 2) for _ in range(rng.randint(1, 3))) + " ^")
            out.append("| " + " | ".join(rng.choice([words(rng, 1, 3), " ::: ", sentence(rng, DW_INLINE[:10])]) for _ in range(rng.randint(1, 3))) + " |")
        elif r < 0.72:
            out.append("> " + sentence(rng, DW_INLINE))
        elif r < 0.8:
            out.append(rng.choice(["<code>\n" + words(rng) + "\n</code>", "<file php x.php>\n<?php 1; ?>\n</file>", "<html>\n<b>x</b>\n</html>"]))
        elif r < 0.85:
            out.append(rng.choice(["  " + words(rng), "----", "~~NOTOC~~", "<code>"]))
        else:
            out.append("")
        if rng.random() < 0.3:
            out.append("")
    return "\n".join(out) + rng.choice(["\n", "", "\n\n"])


def mutate(rng, text):
    chars = list(text)
    for _ in range(rng.randint(1, 4)):
        if not chars:
            break
        p = rng.randrange(len(chars))
        op = rng.choice(["del", "ins", "dup"])
        if op == "del":
            del chars[p : p + rng.randint(1, 6)]
        elif op == "ins":
            chars[p:p] = list(rng.choice(["[[", "]]", "{{", "}}", "'''", "''", "<!--", "-->", "<nowiki>", "|", "||", "{|", "|}", "**", "//", "((", "))", "%%", "<code>", "\\\\", "\n", "=", " ", "<", "&", "http://x.y"]))
        else:
            chars[p : p + rng.randint(1, 30)] = chars[p : p + rng.randint(1, 30)] * 2
    return "".join(chars)


def make(rng, dialect):
    text = (mw_page if dialect == "wiki" else dw_page)(rng)
    if rng.random() < 0.35:
        text = mutate(rng, text)
    nl = rng.choice(["\n", "\n", "\r\n"])
    text = text.replace("\n", nl)
    kind = rng.choice(["utf8", "utf8", "utf8", "bom", "cp1252"])
    if kind == "bom":
        return b"\xef\xbb\xbf" + text.encode("utf-8")
    if kind == "cp1252":
        try:
            return text.encode("cp1252")
        except UnicodeEncodeError:
            return text.encode("utf-8")
    return text.encode("utf-8")


MAPS = {
    "all": lambda i, t: "T%d | = [[x]] ''y'' {{z}} * \u00e9\nline2 ~~~~ __TOC__ <b> [http://q.r s] **b** //i// \\\\ " % i,
    "third": lambda i, t: "Zeta %d \u65e5" % i if i % 3 == 0 else None,
    "plain": lambda i, t: "Plain%d" % i,
    "echo": lambda i, t: t + "!",
    "pre": lambda i, t: "FR " + t,
    "list": lambda i, t: "* " + t if i % 2 else "= " + t,
    "cjk": lambda i, t: "\u65e5\u672c" + t.replace(" ", "") + "\u8a9e",
}


def digest(b):
    h = 0xCBF29CE484222325
    for x in b:
        h = ((h ^ x) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return "%d:%016x" % (len(b), h)


INERT = [
    ("<nowiki/>", ""), ("<nowiki></nowiki>", ""), ("<nowiki>", ""), ("</nowiki>", ""), ("{{!}}", "|"), ("{{=}}", "="),
    ("%%|%%", "|"), ("%%^%%", "^"), ("%%:::%%", ":::"),
]


def relaxed(b):
    """The build with every inert wrapper and separator escape taken out (Bilingua writes a few more than LocOS where
    a translation would otherwise create markup; the words must still be the same)."""
    s = b.decode("utf-8", "replace")
    for a, r in INERT:
        s = s.replace(a, r)
    return s.encode("utf-8")


def golden(data, dialect, allow):
    config.WIKI_TEMPLATE_PARAMS = allow
    f = get_filter(dialect)
    rec = {"dialect": dialect, "allow": allow, "hex": data.hex()}
    try:
        res = f.parse(data)
    except Exception as e:  # noqa: BLE001
        rec["error"] = type(e).__name__
        return rec
    units = [u.text for u in res.units]
    rec["units"] = units
    builds = {}
    for name, fn in MAPS.items():
        m = {}
        for i, t in enumerate(units):
            v = fn(i, t)
            if v is not None:
                m[str(i)] = v
        try:
            out = f.build(data, m)
            builds[name] = digest(out)
            builds[name + "~inert"] = digest(relaxed(out))
        except Exception as e:  # noqa: BLE001
            builds[name] = "error:" + type(e).__name__
    rec["builds"] = builds
    return rec


def main():
    out, extra = sys.argv[1], sys.argv[2:]
    rng = random.Random(int(os.environ.get("GOLDEN_SEED", 1105)))
    cases = []
    for i in range(int(os.environ.get("GOLDEN_N", 300))):
        d = "wiki" if i % 2 == 0 else "dokuwiki"
        allow = rng.choice(ALLOW) if d == "wiki" else ""
        cases.append({"name": f"random-{i}", **golden(make(rng, d), d, allow)})
    for h in ("", "0a", "efbbbf", "ff", "89504e470d0a1a0a0000000d49484452", "7b7b", "3c212d2d", "e9"):
        for d in ("wiki", "dokuwiki"):
            cases.append({"name": f"hostile-{h}-{d}", **golden(bytes.fromhex(h), d, "")})
    for path in extra:
        d = "dokuwiki" if path.endswith(".dokuwiki") else "wiki"
        with open(path, "rb") as fh:
            cases.append({"name": os.path.basename(path), **golden(fh.read(), d, "")})
    with open(out, "w", encoding="utf-8") as fh:
        json.dump(cases, fh, indent=0)
    print(len(cases), "cases,", sum(1 for c in cases if "error" in c), "refused")


if __name__ == "__main__":
    main()

"""Builds tests/data/locos_golden.json: random HTML Help files (.hhc, .hhk, .hhp) in several encodings and what the
LocOS htmlhelp filters make of them.

Run with the Localization OS venv Python (read only use of its filters; nothing is written there):

    C:\\Users\\brent\\code\\local-tms\\.venv\\Scripts\\python.exe gen_golden.py OUT.json [CORPUS_FILE ...]

For each file the golden holds the input bytes (hex), the unit texts LocOS parses (or the error class), and for several
mappings the length:FNV-1a-64 digest of the bytes build() and build_localized() make (or the error class).
Deterministic (seed 1105).
"""

import json
import os
import random
import sys
import tempfile

LOCOS = os.environ.get("LOCOS_DIR", r"C:\Users\brent\code\local-tms")
os.environ["LOC_TMS_DATA_DIR"] = tempfile.mkdtemp(prefix="locos-hh-golden-")
sys.dont_write_bytecode = True
sys.path.insert(0, LOCOS)

from app import config  # noqa: E402
from app import filters as _f  # noqa: E402,F401
from app.filters.base import get_filter  # noqa: E402

config.MAX_DOCUMENT_SEGMENTS = 10_000_000

EN = ["Open", "the", "dashboard", "Getting", "started", "Print", "settings", "Caf\u00e9", "install", "guide", "Windows",
      "2026", "a", "of", "x", "Hello", "world", "%1", "{0}", "&amp;", "&lt;b&gt;", "&quot;q&quot;", "&#233;", "&reg;",
      "&nbsp;", "&copy", "&bogus;", "---", "...", "\u00fc\u00f1"]
RU = ["\u041f\u0435\u0447\u0430\u0442\u044c", "\u043d\u0430\u0441\u0442\u0440\u043e\u0439\u043a\u0438", "\u0421\u043f\u0440\u0430\u0432\u043a\u0430", "\u0444\u0430\u0439\u043b"]
JA = ["\u5370\u5237", "\u8a2d\u5b9a", "\u30d8\u30eb\u30d7", "\u30d5\u30a1\u30a4\u30eb"]


def words(r, pool, n):
    return " ".join(r.choice(pool) for _ in range(n))


def esc(s, q):
    """Raw attribute text for value s: sometimes entity-escaped, sometimes already has entities."""
    return s


def sitemap(r, pool, charset, index):
    nl = r.choice(["\r\n", "\n", "\n", "\r\n", "\r"])
    out = ['<!DOCTYPE HTML PUBLIC "-//IETF//DTD HTML//EN">', "<HTML>", "<HEAD>"]
    if charset:
        out.append(r.choice([
            f'<meta http-equiv="Content-Type" content="text/html; charset={charset}">',
            f'<META NAME="Content-Type" CONTENT="text/html;charset={charset}">',
            f"<meta charset={charset}>",
        ]))
    out.append('<meta name="GENERATOR" content="Microsoft&reg; HTML Help Workshop 4.1">')
    out.append("<!-- Sitemap 1.0 -->")
    out.append("</HEAD><BODY>")
    if r.random() < 0.6:
        out.append('<OBJECT type="text/site properties">')
        if r.random() < 0.5:
            out.append('\t<param name="ImageType" value="Folder">')
        out.append(r.choice(['\t<param name="Font" value="Tahoma,8,0">', '\t<param name="Font" value="MS Sans Serif,10,204">',
                             "\t<param name=Font value='Arial, 9 ,1'>", '\t<param name="Font" value="Tahoma,8">',
                             '\t<param name="font" value="Segoe UI,9,x">']))
        out.append("</OBJECT>")
    out.append("<UL>")
    names = []
    for _ in range(r.randint(1, 14)):
        depth = r.random() < 0.3
        if depth:
            out.append("<UL>")
        out.append('\t<LI> <OBJECT type="text/sitemap">')
        n = r.choice(["Name", "NAME", "name"])
        txt = words(r, pool, r.randint(1, 5))
        names.append(txt)
        q = r.choice(['"', '"', '"', "'"])
        if q == "'":
            txt2 = txt.replace("'", "&#39;")
        else:
            txt2 = txt.replace('"', "&quot;")
        out.append(f"\t\t<param name={q}{n}{q} value={q}{txt2}{q}>" if r.random() < 0.8 else f"\t\t<PARAM NAME={n} VALUE={q}{txt2}{q}>")
        if index and r.random() < 0.4:
            out.append(f'\t\t<param name="Name" value="{words(r, pool, 2)}">')
        if index and r.random() < 0.3:
            out.append(f'\t\t<param name="Keyword" value="{words(r, pool, 2)}">')
        if index and names and r.random() < 0.35:
            out.append(f'\t\t<param name="See Also" value="{r.choice(names).replace(chr(34), "&quot;")}">')
        if r.random() < 0.7:
            out.append(f'\t\t<param name="Local" value="html\\{r.choice(["a", "b", "c"])}.htm">')
        if r.random() < 0.3:
            out.append('\t\t<param name="ImageNumber" value="2">')
        if r.random() < 0.15:
            out.append('\t\t<param name="Comment" value="not translated">')
        if r.random() < 0.1:
            out.append("\t\t<param name=Name value=bare>")
        out.append("\t\t</OBJECT>")
        if depth:
            out.append("</UL>")
    out.append("</UL>")
    out.append("</BODY></HTML>")
    text = nl.join(out) + r.choice([nl, ""])
    if r.random() < 0.25:  # structural mutation
        chars = list(text)
        for _ in range(r.randint(1, 4)):
            if not chars:
                break
            p = r.randrange(len(chars))
            op = r.choice(["del", "ins", "dup"])
            if op == "del":
                del chars[p : p + r.randint(1, 8)]
            elif op == "ins":
                chars[p:p] = list(r.choice(['"', "'", "<", ">", "=", "</OBJECT>", "<!--", "-->", "<OBJECT type=text/sitemap>", "&", "&#", ";", "/", "<param ", " value="]))
            else:
                chars[p:p] = chars[p : p + r.randint(1, 40)]
        text = "".join(chars)
    return text


def project(r, pool):
    nl = r.choice(["\r\n", "\n", "\r\n", "\r", "\u2028", "\x0b"])
    out = ["[OPTIONS]", "Compatibility=1.1 or later", "Compiled file=manual.chm", "Contents file=toc.hhc",
           "Default topic=start.htm", "Index file=index.hhk"]
    if r.random() < 0.8:
        out.append(r.choice(["Language=0x409 English (United States)", "Language=0x419 Russian", "Language=0x411", "language = 0x40c",
                             "Language=1049", "Language=garbage"]))
    if r.random() < 0.9:
        out.append(r.choice(["Title=", "title = ", "TITLE="]) + words(r, pool, r.randint(1, 4)) + r.choice(["", "  ", "\t"]))
    if r.random() < 0.2:
        out.append("; Title=comment")
    out.append("")
    out.append("[WINDOWS]")
    for _ in range(r.randint(0, 3)):
        out.append(r.choice(['main="', 'Help = "', "pop=\"", "x=", '=" ']) + words(r, pool, 2) + '","toc.hhc","index.hhk","start.htm",,,,,0x23520,,0x387e,,,,,,,,0')
    out.append("")
    out.append("[FILES]")
    out.append("start.htm")
    out.append("title=not an option here")
    if r.random() < 0.3:
        out.append("[options]")
        out.append("Title=" + words(r, pool, 2))
    out.append("")
    text = nl.join(out)
    if r.random() < 0.2:
        chars = list(text)
        for _ in range(r.randint(1, 3)):
            if chars:
                p = r.randrange(len(chars))
                chars[p : p + r.randint(0, 3)] = list(r.choice(["=", '"', "[", "]", ";", "\n", ""]))
        text = "".join(chars)
    return text


def encode(r, text, kind):
    """(bytes, note): the text in a random encoding that can hold it."""
    choice = r.choice(["cp1252", "cp1252", "utf8", "utf8bom", "utf16le", "utf16be", "ascii"])
    if kind == "ru":
        choice = r.choice(["cp1251", "cp1251", "utf8", "utf16le"])
    if kind == "ja":
        choice = r.choice(["cp932", "cp932", "utf8"])
    try:
        if choice == "utf8":
            return text.encode("utf-8")
        if choice == "utf8bom":
            return b"\xef\xbb\xbf" + text.encode("utf-8")
        if choice == "utf16le":
            return b"\xff\xfe" + text.encode("utf-16-le")
        if choice == "utf16be":
            return b"\xfe\xff" + text.encode("utf-16-be")
        if choice == "ascii":
            return text.encode("ascii", "replace")
        return text.encode(choice)
    except UnicodeEncodeError:
        return text.encode("utf-8")


MAPS = {
    "all": lambda i, t: "FR " + t,
    "third": lambda i, t: "Zeta %d \u65e5\u672c" % i if i % 3 == 0 else None,
    "plain": lambda i, t: "Plain%d" % i,
    "echo": lambda i, t: t,
    "cjk": lambda i, t: "\u65e5\u672c\u8a9e%d" % i,
    "ru": lambda i, t: "\u041f\u0440\u0438\u0432\u0435\u0442 " + t,
    "sym": lambda i, t: "A & \"B\" <c> 'd' " + t,
    "nl": lambda i, t: t + "\nline2\r\n",
    "typo": lambda i, t: "Aide\u202f: " + t + "\u2011\u200b",
    "pad": lambda i, t: "  " + t + "  ",
}
LOCALES = ["de-DE", "ja-JP", "ru-RU", "zh-Hant", "ar-SA", "vi-VN", "fr", "hi-IN", "ko-KR", "el-GR", "bad value", "en-US", "pt_BR", "zh"]


def digest(b):
    h = 0xCBF29CE484222325
    for x in b:
        h = ((h ^ x) * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return "%d:%016x" % (len(b), h)


def attempt(fn):
    try:
        return digest(fn())
    except Exception as e:  # noqa: BLE001
        return "ERR:" + type(e).__name__


def golden(fmt, data):
    f = get_filter(fmt)
    rec = {"fmt": fmt, "hex": data.hex()}
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
        builds[name] = attempt(lambda m=m: f.build(data, m))
    rec["builds"] = builds
    loc = {}
    names = list(MAPS)
    for k, lc in enumerate(LOCALES):
        name = names[k % len(names)]
        m = {str(i): MAPS[name](i, t) for i, t in enumerate(units) if MAPS[name](i, t) is not None}
        loc[lc] = attempt(lambda m=m, lc=lc: f.build_localized(data, m, lc))
        loc[lc + "|map"] = name
    # a second pass with the Russian map so ANSI targets are exercised on every file
    for lc in ("ru-RU", "ja-JP", "fr-FR", "vi-VN"):
        m = {str(i): "\u041f\u0440\u0438\u0432\u0435\u0442 %d" % i if lc != "fr-FR" else "Salut \u00e9\u00e8 %d" % i for i, t in enumerate(units)}
        if lc == "ja-JP":
            m = {str(i): "\u65e5\u672c\u8a9e%d" % i for i in range(len(units))}
        if lc == "vi-VN":
            m = {str(i): "Vi\u1ec7t Nam %d" % i for i in range(len(units))}
        loc[lc + "#2"] = attempt(lambda m=m, lc=lc: f.build_localized(data, m, lc))
    rec["localized"] = loc
    return rec


HOSTILE = [b"", b"\x00\x01\x02\x03", b"\xef\xbb\xbf\xff", b"\xff", b"\xff\xfe\x00", b"\xfe\xff\xd8\x00",
           b"<" * 3000, b"<!-- " * 1000, b'<OBJECT type="text/sitemap"><param name="Name" value="a">' * 200,
           b"[OPTIONS]\r\n" + b"Title=x\r\n" * 300, b"[WINDOWS]\r\n" + b'a="x","y"\r\n' * 300]


def main():
    out, extra = sys.argv[1], sys.argv[2:]
    r = random.Random(1105)
    cases = []
    for i in range(520):
        k = r.choice(["hhc", "hhk", "hhp"])
        flavor = r.choice(["en", "en", "en", "ru", "ja"])
        pool = {"en": EN, "ru": RU + EN[:3], "ja": JA + EN[:3]}[flavor]
        if k == "hhp":
            text = project(r, pool)
            charset = None
        else:
            charset = {"en": r.choice(["windows-1252", "", "iso-8859-1", ""]), "ru": "windows-1251", "ja": "shift_jis"}[flavor]
            text = sitemap(r, pool, charset or None, k == "hhk")
        data = encode(r, text, flavor)
        cases.append({"name": f"random-{i}", **golden(k, data)})
    for i, h in enumerate(HOSTILE):
        for k in ("hhc", "hhp"):
            cases.append({"name": f"hostile-{k}-{i}", **golden(k, h)})
    for path in extra:
        ext = path.rsplit(".", 1)[-1].lower()
        with open(path, "rb") as fh:
            cases.append({"name": os.path.basename(path), **golden(ext, fh.read())})
    with open(out, "w", encoding="utf-8") as fh:
        json.dump(cases, fh, indent=0)
    print(len(cases), "cases,", sum(1 for c in cases if "error" in c), "refused")


if __name__ == "__main__":
    main()
